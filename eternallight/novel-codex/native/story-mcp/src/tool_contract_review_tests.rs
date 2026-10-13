use super::*;

fn client() -> RuntimeClient {
    RuntimeClient {
        tool_binding: Default::default(),
        request_dispatched: Default::default(),
        completion_receipt: Default::default(),
        publication_gate: Default::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: 1,
        candidate_token: "synthetic-candidate".into(),
        capability: "synthetic-capability".into(),
        operation: "advisory".into(),
    }
}

fn meta() -> Value {
    json!({"sessionId":"root","rootThreadId":"root","callerThreadId":"root",
        "parentThreadId":null,"turnId":"turn","callId":"call"})
}

#[test]
fn tool_contract_review_actual_tools_return_declared_constraints_without_dispatch() {
    for (name, extra, path, expected) in [
        (
            "complete_task",
            json!({"summary":42}),
            "/summary",
            json!({"type":"string"}),
        ),
        (
            "read_work_issues",
            json!({"issueId":null,"cursor":0,"limit":0}),
            "/limit",
            json!({"minimum":1}),
        ),
        (
            "read_source_content",
            json!({"sourceId":format!("source-{}", "a".repeat(64)),"view":"PRIVATE_VALUE"}),
            "/view",
            json!({"enum":["content","raw","index","workspace","locate","passages","scene"]}),
        ),
    ] {
        let client = client();
        let mut args =
            json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let feedback: Value =
            serde_json::from_str(&client.call(name, &args, &meta()).unwrap_err()).unwrap();
        assert_eq!(feedback["code"], "invalid_tool_arguments");
        assert_eq!(feedback["instancePath"], path);
        assert_eq!(feedback["expected"], expected);
        assert!(!feedback.to_string().contains("PRIVATE_VALUE"));
        assert!(!client.request_dispatched.get());
        assert_eq!(feedback["attempt"], 1);
    }
}

#[test]
fn tool_contract_review_nullable_report_object_exposes_missing_field() {
    let tool = tool_specs()
        .into_iter()
        .find(|t| t["name"] == "submit_revision_outcome")
        .unwrap();
    let schema = &tool["inputSchema"]["properties"]["payload"];
    let mut report = json!({"writerTaskId":"writer","chapterId":"chapter","originalRevisionId":null,
        "draftRevisionId":"saved","knowledgeBatchId":null,"uncoveredRequirements":[],
        "outcomes":[{"id":"criterion","status":"supported","reason":"PRIVATE_REASON",
            "issue":{"issueId":"problem","newIssue":null},
            "evidence":[{"groupId":"group","quote":"PRIVATE_QUOTE"}]}]});
    let validation = argument_validation::ArgumentValidation::default();
    let error: Value = serde_json::from_str(
        &validation
            .validate("submit_revision_outcome", &report, schema)
            .unwrap_err(),
    )
    .unwrap();
    assert_eq!(error["instancePath"], "/outcomes/0/issue");
    assert_eq!(error["missingField"], "resolutionOf");
    assert!(!error.to_string().contains("PRIVATE_"));
    report["outcomes"][0]["issue"]["resolutionOf"] = json!([]);
    validation
        .validate("submit_revision_outcome", &report, schema)
        .unwrap();
    report["outcomes"][0]["issue"] = Value::Null;
    validation
        .validate("submit_revision_outcome", &report, schema)
        .unwrap();
}

#[test]
fn tool_contract_review_discovery_modes_share_schema_and_mcp_correction() {
    let client = Ok(client());
    let message = |args: Value| {
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"discover_creative_capabilities","arguments":args,"_meta":meta()}})
    };
    let invalid = json!({"threadId":"root","query":"PRIVATE_QUERY","roleId":"candidate_writer"});
    let listed = super::transport_fixture::rpc(
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        &client,
    )
    .unwrap();
    let tool = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "discover_creative_capabilities")
        .unwrap();
    assert!(
        !jsonschema::validator_for(&tool["inputSchema"])
            .unwrap()
            .is_valid(&invalid)
    );
    let rejected = super::transport_fixture::rpc(&message(invalid), &client).unwrap();
    assert_eq!(rejected["result"]["isError"], true);
    let visible: Value =
        serde_json::from_str(rejected["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(visible, rejected["result"]["structuredContent"]);
    assert_eq!(visible["code"], "invalid_tool_arguments");
    assert_eq!(
        visible["expected"],
        json!({"not":{"required":["query","roleId"]}})
    );
    assert!(!visible.to_string().contains("PRIVATE_QUERY"));
    for args in [
        json!({"threadId":"root"}),
        json!({"threadId":"root","query":"正文"}),
        json!({"threadId":"root","roleId":"candidate_writer"}),
    ] {
        let accepted = super::transport_fixture::rpc(&message(args), &client).unwrap();
        assert_ne!(accepted["result"]["isError"], true, "{accepted}");
    }
    assert!(!client.as_ref().unwrap().request_dispatched.get());
}

#[test]
fn tool_contract_review_union_selection_uses_unique_discriminators_not_guesses() {
    let schema = json!({"type":"object","properties":{"rows":{"type":"array","items":{"$ref":"#/$defs/row"}}},
    "$defs":{"row":{"oneOf":[
        {"type":"object","properties":{"kind":{"const":"state"},"value":{"type":"integer"}},"required":["kind","value"]},
        {"type":"object","properties":{"kind":{"const":"event"},"time":{"type":"integer"}},"required":["kind","time"]}
    ]}}});
    let diagnostic = |schema: &Value, value: &Value| -> Value {
        serde_json::from_str(
            &argument_validation::ArgumentValidation::default()
                .validate("test", value, schema)
                .unwrap_err(),
        )
        .unwrap()
    };
    let selected = diagnostic(
        &schema,
        &json!({"rows":[{"kind":"event","time":"PRIVATE_VALUE"}]}),
    );
    assert_eq!(selected["instancePath"], "/rows/0/time");
    assert_eq!(selected["expected"], json!({"type":"integer"}));
    assert!(!selected.to_string().contains("PRIVATE_VALUE"));
    let ambiguous = diagnostic(&schema, &json!({"rows":[{}]}));
    assert_eq!(ambiguous["instancePath"], "/rows/0");
    assert!(
        ambiguous.get("missingField").is_none(),
        "Do not choose a semantic branch for the model"
    );
    let overlapping = json!({"anyOf":[{"type":"object","required":["left"]},{"type":"object","required":["right"]}]});
    assert!(
        diagnostic(&overlapping, &json!({}))
            .get("missingField")
            .is_none()
    );
    let untyped = json!({"anyOf":[{"type":"object","required":["left"]},{"required":["right"]}]});
    assert!(
        diagnostic(&untyped, &json!({}))
            .get("missingField")
            .is_none()
    );
}

#[test]
fn tool_contract_review_large_expectations_preserve_location_and_diagnostic_budget() {
    let schema = json!({"type":"object","properties":{"mode":{"enum":["x".repeat(2000)]}}});
    let error = argument_validation::ArgumentValidation::default()
        .validate("test", &json!({"mode":"PRIVATE_VALUE"}), &schema)
        .unwrap_err();
    assert!(error.len() <= 1600);
    let error: Value = serde_json::from_str(&error).unwrap();
    assert_eq!(error["instancePath"], "/mode");
    assert_eq!(error["reason"], "enum");
    assert_eq!(error["expectedOmitted"], true);
    assert!(error.get("expected").is_none());
    assert!(!error.to_string().contains("PRIVATE_VALUE"));
}

#[test]
fn optional_query_guidance_is_deferred_without_changing_any_validation_contract() {
    for operation in ["advisory", "generateChapter", "assessRevisionOutcome"] {
        let mut client = client();
        client.operation = operation.into();
        let admitted = client.operation_tools();
        let raw = admitted
            .iter()
            .find(|t| t["name"] == "read_context")
            .unwrap();
        let full = tool_binding::model_tool(raw.clone());
        let shown = client
            .exposed_tools()
            .into_iter()
            .find(|t| t["name"] == "read_context")
            .unwrap();
        let guide =
            tool_guidance::read(admitted.clone(), "read_context", Some("queryPlan")).unwrap();
        assert_eq!(
            guide["fieldGuidance"]["description"],
            full["inputSchema"]["properties"]["queryPlan"]["description"]
        );
        let original_size = full["inputSchema"]["properties"]["queryPlan"]
            .to_string()
            .len();
        let shown_size = shown["inputSchema"]["properties"]["queryPlan"]
            .to_string()
            .len();
        assert!(original_size > shown_size + 1000);
        assert!(
            shown["inputSchema"]["properties"]["queryPlan"]["description"]
                .as_str()
                .unwrap()
                .contains("read_tool_guide")
        );
        let mut expected = full["inputSchema"].clone();
        expected["properties"]["queryPlan"]["description"] =
            shown["inputSchema"]["properties"]["queryPlan"]["description"].clone();
        assert_eq!(
            shown["inputSchema"], expected,
            "all non-description constraints must be identical"
        );
        let query = admitted.iter().find(|t| t["name"] == "query_story");
        if let Some(query) = query {
            let shown_query = client
                .exposed_tools()
                .into_iter()
                .find(|t| t["name"] == "query_story")
                .unwrap();
            assert_eq!(
                shown_query["inputSchema"],
                tool_binding::model_tool(query.clone())["inputSchema"]
            );
        }
    }
}
