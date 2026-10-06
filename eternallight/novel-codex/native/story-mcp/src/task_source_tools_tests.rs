use super::{RuntimeClient, handle_message};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

fn invoke(client: &RuntimeClient, name: &str, args: Value, meta: Value) -> Value {
    handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args,"_meta":meta}}),&Ok(RuntimeClient {
        publication_gate:super::publication_gate::PublicationGate::default(),host:client.host.clone(),port:client.port,candidate_token:client.candidate_token.clone(),capability:client.capability.clone(),operation:client.operation.clone(),
        read_progress: Default::default(),
            argument_validation: Default::default(),
    })).unwrap()
}
fn response(listener: &TcpListener, first: String, payload: Value) -> std::thread::JoinHandle<()> {
    let listener = listener.try_clone().unwrap();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 2048];
        loop {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(header_end) = bytes
                .windows(/*size*/ 4)
                .position(|part| part == b"\r\n\r\n")
            {
                let header = String::from_utf8_lossy(&bytes[..header_end]);
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|s| s.parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                if bytes.len() >= header_end + 4 + length {
                    break;
                }
            }
        }
        assert_eq!(
            String::from_utf8_lossy(&bytes).lines().next().unwrap(),
            first
        );
        let payload = payload.to_string();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        )
        .unwrap();
    })
}
fn text(result: &Value) -> Value {
    serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}
#[test]
fn all_bound_operations_derive_purpose_and_forward_task_owned_source_pages() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = super::query_plan_schema_tests::client(&listener);
    let binding =
        json!({"projectId":"project/中","taskId":"task ?","threadId":"root","expectedRevision":17});
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root"});
    let source = format!("source-{}", "a".repeat(/*n*/ 64));
    let page = json!({"sourceId":source,"sectionPath":"/relevantText","contentHash":"hash","payloadEncoding":"jsonUtf8","byteRange":{"start":0,"end":10,"total":10},"payloadPart":"[\"原文\"]","nextCursor":null,"endOfInputFromCursor":true,"byteLimit":6500});
    for (operation, purpose) in [
        ("advisory", "planner"),
        ("generateBrief", "planner"),
        ("generateOutline", "planner"),
        ("generatePlan", "planner"),
        ("generatePropertyRules", "planner"),
        ("reviewPlanningSeat", "planner"),
        ("generateChapter", "writer"),
        ("modelKnowledge", "critic"),
        ("extractCommitments", "critic"),
        ("assessSceneOutcome", "critic"),
        ("assessRevisionOutcome", "critic"),
        ("assessChapterStructure", "critic"),
        ("reviseCommitment", "critic"),
    ] {
        client.operation = operation.into();
        let receiver = response(
            &listener,
            format!(
                "GET /api/projects/project%2F%E4%B8%AD/context?taskId=task%20%3F&threadId=root&projectRevision=17&purpose={purpose}&budget=24&byteBudget=24000 HTTP/1.1"
            ),
            json!({"taskSources":{"entries":[{"sourceId":source}],"textDelivered":false}}),
        );
        assert_eq!(
            text(&invoke(
                &client,
                "read_context",
                binding.clone(),
                root.clone()
            ))["taskSources"]["textDelivered"],
            false
        );
        receiver.join().unwrap();
        let receiver = response(
            &listener,
            format!(
                "GET /api/projects/project%2F%E4%B8%AD/tasks/task%20%3F/task-source?threadId=root&expectedRevision=17&sourceId={source} HTTP/1.1"
            ),
            page.clone(),
        );
        let mut args = binding.clone();
        args["sourceId"] = json!(source);
        args["cursor"] = Value::Null;
        assert_eq!(
            text(&invoke(&client, "read_task_source", args, root.clone())),
            page
        );
        receiver.join().unwrap();
    }
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    for (tool, field, value) in [
        ("read_context", "purpose", json!("writer")),
        ("read_context", "byteBudget", json!(60001)),
        ("read_context", "budget", Value::Null),
        ("read_task_source", "sourceRevisionId", json!("arbitrary")),
    ] {
        let client = super::query_plan_schema_tests::client(&listener);
        let mut args = binding.clone();
        if tool == "read_task_source" {
            args["sourceId"] = json!(source);
            args["cursor"] = Value::Null;
        }
        args[field] = value;
        assert_eq!(
            invoke(&client, tool, args, root.clone())["result"]["isError"],
            true
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    let mut args = binding;
    args["sourceId"] = json!(source);
    args["cursor"] = Value::Null;
    assert_eq!(
        invoke(
            &client,
            "read_task_source",
            args,
            json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"})
        )["result"]["isError"],
        true
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn work_requests_forward_only_bound_action_keys_and_compact_generation_proposals() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = super::query_plan_schema_tests::client(&listener);
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root"});
    let binding =
        json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0});
    let receiver=response(&listener,"GET /api/projects/project/tasks/task/work-decision?threadId=root&expectedRevision=0 HTTP/1.1".into(),json!({"canFinish":false}));
    assert_eq!(
        text(&invoke(
            &client,
            "read_work_status",
            binding.clone(),
            root.clone()
        )),
        json!({"canFinish":false})
    );
    receiver.join().unwrap();
    let mut args = binding.clone();
    args["actionKey"] = json!("ready-action");
    args["instruction"] = json!("仅按授权改稿");
    args["idempotencyKey"] = json!("same-request");
    args["expectedWorkVersion"] = json!(2);
    args["expectedSourceDigest"] = json!("frozen");
    let receiver = response(
        &listener,
        "POST /api/projects/project/tasks/task/work-decision/actions HTTP/1.1".into(),
        json!({"actionKey":"ready-action","step":{"operation":"generateChapter"}}),
    );
    assert_eq!(
        text(&invoke(
            &client,
            "request_work_action",
            args.clone(),
            root.clone()
        ))["actionKey"],
        "ready-action"
    );
    receiver.join().unwrap();
    let mut proposal = binding.clone();
    for (key, value) in [
        ("targetChapters", json!(2)),
        ("instruction", json!("依作者要求继续两章")),
        ("expectedIntentRevision", json!(0)),
        ("idempotencyKey", json!("proposal")),
    ] {
        proposal[key] = value;
    }
    let receiver = response(
        &listener,
        "POST /api/projects/project/tasks/task/generation-handoff HTTP/1.1".into(),
        json!({"proposal":{"proposalId":"proposal-id","projectId":"project","sourceTaskId":"task","threadId":"root","targetChapters":2,"authorIntent":"原始作者正文".repeat(1000)}}),
    );
    assert_eq!(
        text(&invoke(
            &client,
            "request_generation_handoff",
            proposal.clone(),
            root.clone()
        )),
        json!({"proposalId":"proposal-id","targetChapters":2,"state":"proposed","approvalRequired":true})
    );
    receiver.join().unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    for (tool, mut args) in [
        ("request_work_action", args),
        ("request_generation_handoff", proposal),
    ] {
        args["target"] = json!({"chapterId":"foreign"});
        assert_eq!(
            invoke(&client, tool, args, root.clone())["result"]["isError"],
            true
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn invalid_source_cursor_returns_an_exact_repair_without_silently_retrying() {
    let args = json!({"projectId":"project", "taskId":"task", "threadId":"thread", "expectedRevision":1,
        "sourceId":format!("source-{}", "a".repeat(64)), "cursor":"null"});
    let original = json!({"error":{"code":"invalid_request", "message":"InvalidContent: Task source cursor does not match the frozen task and UTF-8 range", "details":{}, "recovery":null}});
    let mut response = (400, original.clone());
    super::task_source_tools::add_read_recovery("read_task_source", &args, &mut response);
    assert_eq!(response.0, 400);
    assert_eq!(response.1["error"]["message"], original["error"]["message"]);
    let mut expected = args.clone();
    expected["cursor"] = Value::Null;
    assert_eq!(response.1["error"]["recovery"]["arguments"], expected);
    assert_eq!(response.1["error"]["recovery"]["retryUnchanged"], false);
    for status in [200, 403, 409, 500] {
        let mut response = (status, original.clone());
        super::task_source_tools::add_read_recovery("read_task_source", &args, &mut response);
        assert_eq!(
            response.1, original,
            "Never convert authorization, stale-source or transport errors into cursor repairs"
        );
    }
}

#[test]
fn work_status_cursor_is_optional_bounded_and_encoded_without_coercion() {
    let mut args =
        json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3});
    assert!(
        !super::task_source_tools::request("read_work_status", &args)
            .unwrap()
            .1
            .contains("cursor=")
    );
    args["cursor"] = json!("1:abcdef");
    let request = super::task_source_tools::request("read_work_status", &args).unwrap();
    assert!(request.1.ends_with("&cursor=1%3Aabcdef"));
    for invalid in [json!(""), json!(4), json!("x".repeat(101))] {
        args["cursor"] = invalid;
        assert!(super::task_source_tools::request("read_work_status", &args).is_err());
    }
    let specs = super::task_source_tools::specs();
    let spec = specs
        .iter()
        .find(|spec| spec["name"] == "read_work_status")
        .unwrap();
    assert_eq!(
        spec["inputSchema"]["properties"]["cursor"]["maxLength"],
        100
    );
    assert!(
        !spec["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("cursor"))
    );
}

#[test]
fn work_recovery_plan_is_strict_bounded_and_optional() {
    let mut args = json!({"projectId":"project","taskId":"decision","threadId":"root","expectedRevision":0,"actionKey":"action","instruction":"修正后继续","idempotencyKey":"key","expectedWorkVersion":1,"expectedSourceDigest":"digest"});
    assert!(super::task_source_tools::request("request_work_action", &args).is_ok());
    let plan=json!({"failedTaskId":"failed","cause":"参数错误","change":"读取结构后只提交必要字段","verification":"核对保存回执"});
    args["recoveryPlan"]=plan.clone();
    let (_, _, body)=super::task_source_tools::request("request_work_action", &args).unwrap();
    assert_eq!(body.unwrap()["recoveryPlan"],plan);
    args["recoveryPlan"]["unexpected"]=json!(true);
    assert!(super::task_source_tools::request("request_work_action", &args).is_err());
    args["recoveryPlan"]=plan; args["recoveryPlan"]["change"]=json!("x".repeat(601));
    assert!(super::task_source_tools::request("request_work_action", &args).is_err());
    let tool=super::task_source_tools::specs().into_iter().find(|tool|tool["name"]=="request_work_action").unwrap();
    assert_eq!(tool["inputSchema"]["properties"]["recoveryPlan"]["additionalProperties"],false);
}

#[test]
fn agent_work_proposals_are_exclusive_and_cannot_supply_execution_authority() {
    let base = json!({"projectId":"project","taskId":"decision","threadId":"root","expectedRevision":0,
        "instruction":"只修正已授权章节中的时间矛盾","idempotencyKey":"local-repair",
        "expectedWorkVersion":1,"expectedSourceDigest":"digest"});
    let proposal = json!({"operation":"generateChapter","target":{"chapterId":"chapter-one",
        "scope":{"type":"selectedScenes","groupIds":["scene-one"],"sourceRevisionId":"draft-current"}}});
    let mut args = base.clone();
    assert!(super::task_source_tools::request("request_work_action", &args).is_err());
    args["proposal"] = proposal.clone();
    let (_, _, body) = super::task_source_tools::request("request_work_action", &args).unwrap();
    assert_eq!(body.unwrap()["proposal"], proposal);
    args["actionKey"] = json!("another-action");
    assert!(super::task_source_tools::request("request_work_action", &args).is_err());
    for bad in [
        json!({"operation":"publishChapter","target":{"chapterId":"chapter-one"}}),
        json!({"operation":"generateChapter","target":{"chapterId":"chapter-one","runId":"forged"}}),
        json!({"operation":"generateChapter","target":{"chapterId":"chapter-one","scope":{"type":"selectedScenes","groupIds":[]}}}),
        json!({"operation":"generateChapter","target":{"chapterId":"chapter-one","scope":{"type":"wholeChapter","grantId":"forged"}}}),
    ] {
        let mut invalid = base.clone(); invalid["proposal"] = bad;
        assert!(super::task_source_tools::request("request_work_action", &invalid).is_err());
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = super::query_plan_schema_tests::client(&listener);
    let receiver = response(&listener, "POST /api/projects/project/tasks/decision/work-decision/actions HTTP/1.1".into(), json!({"step":{"operation":"generateChapter"}}));
    let mut valid = base; valid["proposal"] = proposal;
    let result = invoke(&client, "request_work_action", valid, json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root"}));
    assert_eq!(text(&result)["step"]["operation"], "generateChapter");
    receiver.join().unwrap();
}
