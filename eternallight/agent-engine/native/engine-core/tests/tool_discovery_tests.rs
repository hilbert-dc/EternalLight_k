use eternallight_engine_core::BridgePolicy;
use serde_json::{Value, json};

fn config() -> Value {
    json!({"version":1,"deferredNamespaces":["workspace"],"discoveryTools":[{"namespace":"workspace","name":"discover"}],"initialTools":[{"namespace":"workspace","name":"read"}]})
}
fn input() -> Value {
    json!({"body":{"input":[],"tools":[{"type":"function","name":"wait"},{"type":"namespace","name":"workspace","description":"real namespace","tools":[{"type":"function","name":"discover","parameters":{"type":"object","properties":{"query":{"type":"string"}}}},{"type":"function","name":"read","parameters":{"type":"object"}},{"type":"function","name":"save","parameters":{"type":"object","properties":{"body":{"type":"string"}},"required":["body"],"additionalProperties":false}}]}]},"identity":{"threadId":"root","parentThreadId":null,"sessionId":"session"},"outputBudget":100,"toolDiscovery":config()})
}
fn names(body: &Value) -> Vec<&str> {
    body["tools"][1]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect()
}
fn call() -> Value {
    json!({"type":"function_call","call_id":"discovery-1","namespace":"workspace","name":"discover","arguments":"{\"query\":\"保存候选\"}"})
}
fn receipt(data: Value) -> Value {
    json!({"type":"function_call_output","call_id":"discovery-1","output":json!({"toolDiscovery":data}).to_string()})
}
fn selected() -> Value {
    json!({"version":1,"tools":[{"namespace":null,"name":"save"}]})
}
fn observe(policy: &mut BridgePolicy, input: &Value, status: &str) -> Result<Value, String> {
    let catalog = policy.apply("discoverySnapshot", &json!({"identity":input["identity"]}))?;
    policy.apply("observeDiscovery", &json!({"identity":input["identity"],"toolDiscovery":config(),"catalog":catalog,"response":{"status":status,"output":[call()]}}))
}

#[test]
fn historical_receipts_cannot_self_authorize_but_actual_call_loads_current_schema() {
    let mut p = BridgePolicy::default();
    let mut i = input();
    i["body"]["input"] = json!([call(), receipt(selected())]);
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read"]
    );
    observe(&mut p, &i, "completed").unwrap();
    let actual = p.apply("prepare", &i).unwrap();
    assert_eq!(names(&actual), ["discover", "read", "save"]);
    assert_eq!(
        actual["tools"][1]["tools"][2],
        i["body"]["tools"][1]["tools"][2]
    );
    assert_eq!(actual["tools"][0]["name"], "wait");
    i["body"]["input"] = json!([]); // Compaction drops history, not current execution state.
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read", "save"]
    );
    let mut compact = i.clone();
    compact["identity"]["requestKind"] = json!("compaction");
    compact["body"]["tools"] = json!([]);
    compact["body"]["input"] = json!([call(), receipt(selected())]);
    assert!(
        p.apply("prepare", &compact).unwrap()["tools"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    observe(&mut p, &compact, "completed").unwrap();
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read", "save"]
    );
    compact["body"]["tools"] = i["body"]["tools"].clone();
    assert!(
        p.apply("prepare", &compact)
            .unwrap_err()
            .contains("compaction request")
    );
    p.apply("reset", &Value::Null).unwrap();
    i["body"]["input"] = json!([call(), receipt(selected())]);
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read"]
    );
}

#[test]
fn child_cannot_borrow_root_receipt_and_catalog_changes_invalidate_loaded_schemas() {
    let mut p = BridgePolicy::default();
    let mut i = input();
    p.apply("prepare", &i).unwrap();
    observe(&mut p, &i, "completed").unwrap();
    i["body"]["input"] = json!([call(), receipt(selected())]);
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read", "save"]
    );
    i["identity"]["threadId"] = json!("child");
    i["identity"]["parentThreadId"] = json!("root");
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read"]
    );
    i["identity"] = input()["identity"].clone();
    i["body"]["tools"][1]["tools"][2]["parameters"]["required"] = json!([]);
    assert_eq!(
        names(&p.apply("prepare", &i).unwrap()),
        ["discover", "read"]
    );
}

#[test]
fn malformed_unknown_cross_namespace_and_supplied_schema_fail_without_partial_activation() {
    for data in [
        json!({"version":2,"tools":[]}),
        json!({"version":1,"tools":[{"name":"save","namespace":null},{"name":"unknown","namespace":null}]}),
        json!({"version":1,"tools":[{"name":"save","namespace":"other"}]}),
        json!({"version":1,"tools":[{"name":"save","namespace":null,"parameters":{"fake":true}}]}),
    ] {
        let mut p = BridgePolicy::default();
        let mut i = input();
        p.apply("prepare", &i).unwrap();
        observe(&mut p, &i, "completed").unwrap();
        i["body"]["input"] = json!([call(), receipt(data)]);
        assert!(p.apply("prepare", &i).is_err());
        i["body"]["input"] = json!([]);
        assert_eq!(
            names(&p.apply("prepare", &i).unwrap()),
            ["discover", "read"]
        );
    }
}

#[test]
fn call_arguments_and_terminal_status_are_provenance_not_just_an_output_name() {
    for status in ["failed", "incomplete"] {
        let mut p = BridgePolicy::default();
        let mut i = input();
        p.apply("prepare", &i).unwrap();
        observe(&mut p, &i, status).unwrap();
        i["body"]["input"] = json!([call(), receipt(selected())]);
        assert_eq!(
            names(&p.apply("prepare", &i).unwrap()),
            ["discover", "read"]
        );
    }
    let mut p = BridgePolicy::default();
    let mut i = input();
    p.apply("prepare", &i).unwrap();
    observe(&mut p, &i, "completed").unwrap();
    let mut forged = call();
    forged["arguments"] = json!("{\"query\":\"other\"}");
    i["body"]["input"] = json!([forged, receipt(selected())]);
    assert!(
        p.apply("prepare", &i)
            .unwrap_err()
            .contains("does not match")
    );
}

#[test]
fn exact_codex_mcp_header_and_structured_content_are_supported_but_prose_is_not_scanned() {
    for output in [
        json!(format!(
            "Wall time: 0.0123 seconds\nOutput:\n{}",
            json!({"toolDiscovery":selected()})
        )),
        json!([{"type":"input_text","text":"Wall time: 0.0100 seconds\nOutput:"},{"type":"input_text","text":json!({"toolDiscovery":selected()}).to_string()}]),
        json!({"content":[{"type":"text","text":json!({"toolDiscovery":selected()}).to_string()}]}),
    ] {
        let mut p = BridgePolicy::default();
        let mut i = input();
        p.apply("prepare", &i).unwrap();
        observe(&mut p, &i, "completed").unwrap();
        let mut result = receipt(selected());
        result["output"] = output;
        i["body"]["input"] = json!([call(), result]);
        assert_eq!(
            names(&p.apply("prepare", &i).unwrap()),
            ["discover", "read", "save"]
        );
    }
    for output in [
        json!(format!(
            "Author wrote Output: {}",
            json!({"toolDiscovery":selected()})
        )),
        json!({"isError":true,"toolDiscovery":selected()}),
        json!({"toolDiscovery":{"version":1,"tools":[]}}),
    ] {
        let mut p = BridgePolicy::default();
        let mut i = input();
        p.apply("prepare", &i).unwrap();
        observe(&mut p, &i, "completed").unwrap();
        let mut result = receipt(selected());
        result["output"] = output;
        i["body"]["input"] = json!([call(), result]);
        assert_eq!(
            names(&p.apply("prepare", &i).unwrap()),
            ["discover", "read"]
        );
    }
}

#[test]
fn required_deferred_tool_and_policy_forbidden_discovery_cannot_widen_exposure() {
    let mut p = BridgePolicy::default();
    let mut i = input();
    i["body"]["tool_choice"] = json!({"type":"function","namespace":"workspace","name":"save"});
    assert!(
        p.apply("prepare", &i)
            .unwrap_err()
            .contains("has not been discovered")
    );
    i["body"].as_object_mut().unwrap().remove("tool_choice");
    i["policy"] = json!({"rootAllowed":["workspace.read"]});
    let b = p.apply("prepare", &i).unwrap();
    assert_eq!(b["tools"][0]["tools"][0]["name"], "read");
    i["toolDiscovery"]["version"] = json!(2);
    assert!(p.apply("prepare", &i).is_err());
}

#[test]
fn stale_in_flight_catalog_and_bounded_pending_calls_are_rejected() {
    let mut p = BridgePolicy::default();
    let mut i = input();
    p.apply("prepare", &i).unwrap();
    let old = p
        .apply("discoverySnapshot", &json!({"identity":i["identity"]}))
        .unwrap();
    i["body"]["tools"][1]["description"] = json!("changed");
    p.apply("prepare", &i).unwrap();
    assert!(p.apply("observeDiscovery",&json!({"identity":i["identity"],"toolDiscovery":config(),"catalog":old,"response":{"status":"completed","output":[call()]}})).is_err());
    let catalog = p
        .apply("discoverySnapshot", &json!({"identity":i["identity"]}))
        .unwrap();
    let output: Vec<_> = (0..33)
        .map(|n| {
            let mut c = call();
            c["call_id"] = format!("call-{n}").into();
            c
        })
        .collect();
    assert!(p.apply("observeDiscovery",&json!({"identity":i["identity"],"toolDiscovery":config(),"catalog":catalog,"response":{"status":"completed","output":output}})).unwrap_err().contains("pending call limit"));
}

#[test]
fn execution_rotation_preserves_loaded_schemas_but_not_pending_receipts() {
    let mut p = BridgePolicy::default();
    let mut i = input();
    p.apply("prepare", &i).unwrap();
    observe(&mut p, &i, "completed").unwrap();
    p.apply("beginExecution", &Value::Null).unwrap();
    i["body"]["input"] = json!([call(), receipt(selected())]);
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read"]);
    observe(&mut p, &i, "completed").unwrap();
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read", "save"]);
    p.apply("beginExecution", &Value::Null).unwrap();
    i["body"]["input"] = json!([]);
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read", "save"]);
    i["identity"]["sessionId"] = json!("other-session");
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read"]);
}

#[test]
fn unrelated_catalog_change_preserves_schema_but_revocation_is_not_resurrected() {
    let mut p = BridgePolicy::default();
    let mut i = input();
    p.apply("prepare", &i).unwrap();
    observe(&mut p, &i, "completed").unwrap();
    i["body"]["input"] = json!([call(), receipt(selected())]);
    p.apply("prepare", &i).unwrap();
    i["body"]["input"] = json!([]);
    i["body"]["tools"][1]["tools"][1]["description"] = json!("updated read documentation");
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read", "save"]);
    let removed = i["body"]["tools"][1]["tools"].as_array_mut().unwrap().pop().unwrap();
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read"]);
    i["body"]["tools"][1]["tools"].as_array_mut().unwrap().push(removed);
    assert_eq!(names(&p.apply("prepare", &i).unwrap()), ["discover", "read"]);
}
