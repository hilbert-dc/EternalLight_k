use super::{RuntimeClient};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

fn work_choice_rpc(
    client: &Result<RuntimeClient, String>,
    tool: &str,
    arguments: Value,
    call: &str,
) -> Value {
    super::transport_fixture::rpc(
        &json!({"jsonrpc":"2.0","id":call,"method":"tools/call","params":{
            "name":tool,"arguments":arguments,"_meta":{
                "rootThreadId":"root","callerThreadId":"root","parentThreadId":null,
                "sessionId":"root","turnId":"turn","callId":call
            }
        }}),
        client,
    )
    .unwrap()
}

fn work_choice_base() -> Value {
    json!({"projectId":"project","taskId":"decision","threadId":"root","expectedRevision":2,
        "instruction":"PRIVATE_REJECTED_INSTRUCTION","idempotencyKey":"choice-key",
        "expectedWorkVersion":27,"expectedSourceDigest":"source-digest",
        "chapterAcceptance":{"requirements":[{"id":"saved-chapter","text":"PRIVATE_AUTHOR_REQUIREMENT"}],
            "length":{"requirement":"PRIVATE_LENGTH_REQUIREMENT","minCharacters":600,"maxCharacters":1100}}})
}

fn work_choice_proposal() -> Value {
    json!({"operation":"generateChapter","target":{"chapterId":"chapter-one",
        "scope":{"type":"selectedScenes","sourceRevisionId":"draft-frozen","groupIds":["scene-one"]}}})
}

fn work_choice_no_dispatch(client: &Result<RuntimeClient, String>, listener: &TcpListener) {
    assert!(!client.as_ref().unwrap().request_dispatched.get());
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

fn work_choice_receive(listener: &TcpListener) -> (std::net::TcpStream, String, Value) {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(std::time::Instant::now() < deadline, "request was not sent");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    };
    socket.set_nonblocking(false).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 2048];
    loop {
        let count = socket.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = String::from_utf8(bytes[..end].to_vec()).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or_default();
            if bytes.len() >= end + 4 + length {
                let body = if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                };
                return (socket, header, body);
            }
        }
    }
}

fn work_choice_reply(socket: &mut std::net::TcpStream, payload: Value) {
    let payload = payload.to_string();
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    )
    .unwrap();
}

#[test]
fn work_action_choice_catalog_and_rpc_reject_ambiguous_or_null_choices_before_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = Ok(super::query_plan_schema_tests::client(&listener));
    let catalog = super::transport_fixture::rpc(
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        &client,
    )
    .unwrap();
    let tool = catalog["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "request_work_action")
        .unwrap();
    let schema = &tool["inputSchema"];
    assert_eq!(
        schema["oneOf"],
        json!([{"required":["actionKey"]},{"required":["proposal"]}])
    );
    let instruction = schema["description"].as_str().unwrap();
    assert!(instruction.len() <= 600);
    for meaning in [
        "exactly one",
        "omit the other",
        "frozen target",
        "approved author scope",
    ] {
        assert!(instruction.contains(meaning));
    }
    assert_eq!(schema["properties"]["actionKey"]["type"], "string");
    assert_eq!(schema["properties"]["proposal"]["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["required"],
        json!([
            "instruction",
            "expectedWorkVersion",
            "expectedSourceDigest"
        ])
    );
    for other in catalog["result"]["tools"].as_array().unwrap() {
        if other["name"] == "request_generation_handoff" {
            assert!(other["inputSchema"].get("oneOf").is_none());
            assert!(other["inputSchema"].get("description").is_none());
        }
    }
    let proposal = work_choice_proposal();
    for (choice, reason, path) in [
        (
            json!({"actionKey":"available-action","proposal":proposal}),
            "constraint",
            "/oneOf",
        ),
        (json!({}), "constraint", "/oneOf"),
        (
            json!({"actionKey":null}),
            "type",
            "/properties/actionKey/type",
        ),
        (
            json!({"proposal":null}),
            "type",
            "/properties/proposal/type",
        ),
        (
            json!({"actionKey":null,"proposal":null}),
            "type",
            "/properties/actionKey/type",
        ),
        (
            json!({"actionKey":"available-action","proposal":null}),
            "type",
            "/properties/proposal/type",
        ),
        (
            json!({"actionKey":null,"proposal":proposal}),
            "type",
            "/properties/actionKey/type",
        ),
    ] {
        let client = Ok(super::query_plan_schema_tests::client(&listener));
        let mut arguments = work_choice_base();
        arguments
            .as_object_mut()
            .unwrap()
            .extend(choice.as_object().unwrap().clone());
        let original = arguments.clone();
        let result = work_choice_rpc(
            &client,
            "request_work_action",
            arguments.clone(),
            "invalid-choice",
        );
        let feedback = &result["result"]["structuredContent"];
        assert_eq!(result["result"]["isError"], true);
        assert_eq!(feedback["code"], "invalid_tool_arguments");
        assert_eq!(feedback["attempt"], 1);
        assert_eq!(feedback["reason"], reason);
        assert_eq!(feedback["schemaPath"], path);
        if reason == "constraint" {
            assert_eq!(feedback["instancePath"], "");
            assert_eq!(feedback["fieldInstruction"], instruction);
        }
        assert_eq!(feedback["recoveryOperation"]["phase"], "beforeDispatch");
        assert_eq!(feedback["recoveryOperation"]["outcome"], "rejected");
        assert_eq!(feedback["recoveryOperation"]["operationId"], Value::Null);
        assert!(!feedback.to_string().contains("PRIVATE_"));
        assert_eq!(arguments, original);
        work_choice_no_dispatch(&client, &listener);
    }
}

#[test]
fn work_action_choice_same_task_correction_forwards_each_route_once_without_rewriting() {
    for route in ["actionKey", "proposal"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = Ok(super::query_plan_schema_tests::client(&listener));
        let mut both = work_choice_base();
        both["actionKey"] = json!("available-action");
        both["proposal"] = work_choice_proposal();
        let first = work_choice_rpc(&client, "request_work_action", both, "first-choice");
        assert_eq!(first["result"]["structuredContent"]["attempt"], 1);
        work_choice_no_dispatch(&client, &listener);
        let read = work_choice_rpc(
            &client,
            "read_creative_strategy",
            json!({"threadId":"root","strategyId":"catalog"}),
            "method-read",
        );
        assert!(read["result"].get("isError").is_none());
        work_choice_no_dispatch(&client, &listener);
        let second = work_choice_rpc(
            &client,
            "request_work_action",
            work_choice_base(),
            "second-choice",
        );
        assert_eq!(second["result"]["structuredContent"]["attempt"], 2);
        work_choice_no_dispatch(&client, &listener);

        let mut corrected = work_choice_base();
        corrected[route] = if route == "actionKey" {
            json!("available-action")
        } else {
            work_choice_proposal()
        };
        let original = corrected.clone();
        let mut expected = corrected.clone();
        expected.as_object_mut().unwrap().remove("projectId");
        let receiver = std::thread::spawn(move || {
            let (mut socket, header, body) = work_choice_receive(&listener);
            assert_eq!(
                header.lines().next().unwrap(),
                "POST /api/projects/project/tasks/decision/work-decision/actions HTTP/1.1"
            );
            for field in [
                "X-Novel-Root-Thread: root",
                "X-Novel-Tool-Call: corrected-choice",
            ] {
                assert!(header.contains(field));
            }
            assert_eq!(body, expected);
            work_choice_reply(&mut socket, json!({"selected":true}));
            drop(socket);
            // The proof read observes absence; this transport fixture never invents a saved receipt.
            let (mut socket, header, body) = work_choice_receive(&listener);
            assert!(
                header
                    .lines()
                    .next()
                    .unwrap()
                    .starts_with("GET /api/projects/project/tasks/recovery-operations?")
            );
            assert_eq!(body, Value::Null);
            work_choice_reply(&mut socket, json!({"operation":null}));
            drop(socket);
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        });
        let result = work_choice_rpc(
            &client,
            "request_work_action",
            corrected.clone(),
            "corrected-choice",
        );
        assert!(result["result"].get("isError").is_none());
        assert_eq!(text(&result)["selected"], true);
        assert_eq!(corrected, original);
        receiver.join().unwrap();
    }
}

#[test]
fn work_action_choice_read_success_does_not_reset_three_attempt_limit_or_allow_bypass() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = Ok(super::query_plan_schema_tests::client(&listener));
    let mut invalid = work_choice_base();
    invalid["actionKey"] = json!("available-action");
    invalid["proposal"] = work_choice_proposal();
    for attempt in 1..=3 {
        let result = work_choice_rpc(
            &client,
            "request_work_action",
            invalid.clone(),
            &format!("invalid-{attempt}"),
        );
        let feedback = &result["result"]["structuredContent"];
        assert_eq!(result["result"]["isError"], true);
        assert_eq!(feedback["limit"], 3);
        if attempt < 3 {
            assert_eq!(feedback["code"], "invalid_tool_arguments");
            assert_eq!(feedback["attempt"], attempt);
            let read = work_choice_rpc(
                &client,
                "read_creative_strategy",
                json!({"threadId":"root","strategyId":"catalog"}),
                &format!("read-{attempt}"),
            );
            assert!(read["result"].get("isError").is_none());
        } else {
            assert_eq!(feedback["code"], "tool_argument_correction_limit");
            assert_eq!(feedback["originTool"], "request_work_action");
        }
        work_choice_no_dispatch(&client, &listener);
    }
    let mut corrected = work_choice_base();
    corrected["actionKey"] = json!("available-action");
    for (tool, args) in [
        ("request_work_action", corrected),
        (
            "read_creative_strategy",
            json!({"threadId":"root","strategyId":"catalog"}),
        ),
    ] {
        let result = work_choice_rpc(&client, tool, args, "after-limit");
        assert_eq!(result["result"]["isError"], true);
        assert_eq!(
            result["result"]["structuredContent"]["code"],
            "tool_argument_correction_limit"
        );
        assert_eq!(
            result["result"]["structuredContent"]["originTool"],
            "request_work_action"
        );
        work_choice_no_dispatch(&client, &listener);
    }
}

#[test]
fn workspace_locator_contract_is_bounded_and_filters_never_apply_to_body_reads() {
    let base = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":0,"sourceId":format!("source-{}","a".repeat(64)),"view":"workspace","chapterNumber":999,"objectKind":"draft","query":"第七日 线索"});
    let (method, path, body) =
        super::task_source_tools::request("read_source_content", &base).unwrap();
    assert_eq!(method, "GET");
    assert!(body.is_none());
    assert!(path.contains("view=workspace"));
    assert!(path.contains("chapterNumber=999"));
    assert!(path.contains("objectKind=draft"));
    assert!(!path.contains("第七日"));
    let specs = super::task_source_tools::specs();
    let spec = specs
        .iter()
        .find(|s| s["name"] == "read_source_content")
        .unwrap();
    assert_eq!(
        spec["inputSchema"]["properties"]["view"]["enum"],
        json!(["content", "raw", "index", "workspace", "locate", "passages", "scene"])
    );
    for (key, value) in [
        ("view", json!("index")),
        ("chapterNumber", json!(0)),
        ("objectKind", json!("private")),
        ("query", json!("长".repeat(121))),
    ] {
        let mut bad = base.clone();
        bad[key] = value;
        assert!(
            super::task_source_tools::request("read_source_content", &bad).is_err(),
            "{key}"
        )
    }
    let mut raw=base.clone(); raw["view"]=json!("raw");
    assert!(super::task_source_tools::request("read_source_content", &raw).is_err());
}

fn invoke(client: &RuntimeClient, name: &str, args: Value, meta: Value) -> Value {
    super::transport_fixture::rpc(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args,"_meta":meta}}),&Ok(RuntimeClient {
        tool_binding: Default::default(), request_dispatched: Default::default(),
        publication_gate:super::publication_gate::PublicationGate::default(),host:client.host.clone(),port:client.port,candidate_token:client.candidate_token.clone(),capability:client.capability.clone(),operation:client.operation.clone(),
        read_progress: Default::default(),
            completion_receipt: Default::default(),
            argument_validation: Default::default(),
    })).unwrap()
}
fn response(listener: &TcpListener, first: String, payload: Value) -> std::thread::JoinHandle<()> {
    response_status(listener, first, 200, payload)
}
fn response_status(
    listener: &TcpListener,
    first: String,
    status: u16,
    payload: Value,
) -> std::thread::JoinHandle<()> {
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
            "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        )
        .unwrap();
    })
}
fn text(result: &Value) -> Value {
    serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}
#[test]
fn passages_rpc_batches_original_excerpts_and_continuation_retains_projection() {
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    let client=Ok(super::query_plan_schema_tests::client(&listener));
    let cursor=format!("{}:4","a".repeat(64));
    let source=format!("source-{}","b".repeat(64));
    let item=json!({"sourceId":source,"revisionId":"r","chapterNumber":1,"contentHash":"hash",
        "excerpt":{"text":"原文🙂","characterRange":{"start":20,"end":23,"total":23},"editAnchor":{"groupId":"g","sourceRevisionId":"r","start":20,"end":23}}});
    let page=json!({"items":[item],"coverage":{"completeSource":false},"nextCursor":cursor});
    let prefix="/api/projects/project/tasks/task/task-source-content?threadId=root&expectedRevision=0&view=passages&edge=end&characters=80";
    let server=response(&listener,format!("GET {prefix} HTTP/1.1"),page.clone());
    let reply=work_choice_rpc(&client,"read_source_content",json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"view":"passages","edge":"end","characters":80}),"passages-first");
    server.join().unwrap();
    let value=text(&reply);
    assert_ne!(reply["result"]["isError"],true);
    assert_eq!(value["items"][0]["excerpt"],page["items"][0]["excerpt"]);
    assert_eq!(value["nextRead"]["arguments"],json!({"expectedRevision":0,"view":"passages","edge":"end","characters":80,"cursor":cursor}));
    let server=response(&listener,format!("GET {prefix}&cursor={} HTTP/1.1",super::percent_encode(&cursor)),json!({"items":[],"nextCursor":null}));
    let last=work_choice_rpc(&client,"read_source_content",value["nextRead"]["arguments"].clone(),"passages-last");
    server.join().unwrap();
    assert_eq!(text(&last)["nextRead"],Value::Null);
    assert!(text(&last).get("nextCursor").is_none());
}
#[test]
fn scene_rpc_continuation_preserves_source_and_group() {
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    let client=Ok(super::query_plan_schema_tests::client(&listener));
    let source=format!("source-{}","b".repeat(64));
    let cursor=format!("{}:2:8","a".repeat(64));
    let prefix=format!("/api/projects/project/tasks/task/task-source-content?threadId=root&expectedRevision=0&sourceId={source}&view=scene&groupId=scene-one");
    let server=response(&listener,format!("GET {prefix} HTTP/1.1"),json!({"entries":[{"text":"原文"}],"nextCursor":cursor}));
    let reply=work_choice_rpc(&client,"read_source_content",json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"sourceId":source,"view":"scene","groupId":"scene-one"}),"scene-first");
    server.join().unwrap();
    let value=text(&reply);
    assert_ne!(reply["result"]["isError"],true);
    assert_eq!(value["nextRead"]["arguments"],json!({"expectedRevision":0,"sourceId":source,"view":"scene","groupId":"scene-one","cursor":cursor}));
    let server=response(&listener,format!("GET {prefix}&cursor={} HTTP/1.1",super::percent_encode(&cursor)),json!({"entries":[],"nextCursor":null}));
    let last=work_choice_rpc(&client,"read_source_content",value["nextRead"]["arguments"].clone(),"scene-last");
    server.join().unwrap();
    assert_eq!(text(&last)["nextRead"],Value::Null);
    for args in [json!({"sourceId":source,"view":"scene"}),json!({"sourceId":source,"view":"scene","groupId":"scene-one","quote":"x"})] {
        assert!(super::task_source_tools::request("read_source_content",&args).is_err());
    }
}
#[test]
fn workspace_continuation_is_callable_without_reconstructing_filters() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = Ok(super::query_plan_schema_tests::client(&listener));
    let cursor = format!("{}:7", "a".repeat(64));
    let args = json!({"projectId":"project","taskId":"task","threadId":"root",
        "expectedRevision":0,"view":"workspace","chapterNumber":52,"objectKind":"draft",
        "query":"draft-chapter","revisionId":"revision-52"});
    let path = "/api/projects/project/tasks/task/task-source-content?threadId=root&expectedRevision=0&view=workspace&chapterNumber=52&objectKind=draft&query=draft-chapter&revisionId=revision-52";
    let page = json!({"items":[],"coverage":{"complete":false},"contentRead":false,"nextCursor":cursor});
    let server = response(&listener, format!("GET {path} HTTP/1.1"), page.clone());
    let first = work_choice_rpc(&client, "read_source_content", args, "first-page");
    server.join().unwrap();
    let first_page = text(&first);
    assert_ne!(first["result"]["isError"], true);
    assert!(first_page.get("nextCursor").is_none());
    assert_eq!(first_page["items"], page["items"]);
    assert_eq!(first_page["coverage"], page["coverage"]);
    assert_eq!(first_page["contentRead"], false);
    let continuation = &first_page["nextRead"];
    assert_eq!(continuation["arguments"], json!({"expectedRevision":0,"view":"workspace",
        "chapterNumber":52,"objectKind":"draft","query":"draft-chapter",
        "revisionId":"revision-52","cursor":cursor}));
    let last_page = json!({"items":[],"coverage":{"complete":true},"contentRead":false,"nextCursor":null});
    let server = response(&listener,
        format!("GET {path}&cursor={} HTTP/1.1", super::percent_encode(&cursor)), last_page.clone());
    // Execute the returned request verbatim: identity still comes from the bound transport.
    let second = work_choice_rpc(&client, continuation["tool"].as_str().unwrap(),
        continuation["arguments"].clone(), "second-page");
    server.join().unwrap();
    let mut expected = last_page;
    expected.as_object_mut().unwrap().remove("nextCursor");
    expected["nextRead"] = Value::Null;
    assert_eq!(text(&second), expected);
    assert_ne!(second["result"]["isError"], true);
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

        assert_eq!(
            text(&invoke(&client, "read_source_content", {args["view"]=json!("raw"); args}, root.clone())),
            {let mut expected=page.clone();expected["nextRead"]=Value::Null;expected.as_object_mut().unwrap().remove("nextCursor");expected}
        );
        receiver.join().unwrap();
    }
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    for (tool, field, value) in [
        ("read_context", "purpose", json!("writer")),
        ("read_context", "byteBudget", json!(60001)),
        ("read_context", "budget", Value::Null),
        ("read_source_content", "sourceRevisionId", json!("arbitrary")),
    ] {
        let client = super::query_plan_schema_tests::client(&listener);
        let mut args = binding.clone();
        if tool == "read_source_content" {
            args["sourceId"] = json!(source);
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

    assert_eq!(
        invoke(
            &client,
            "read_source_content",
            args,
            json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"other-parent","sessionId":"root"})
        )["result"]["isError"],
        true
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn trusted_direct_child_reads_sources_without_adding_root_read_progress() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = Ok(super::query_plan_schema_tests::client(&listener));
    let source = format!("source-{}", "a".repeat(64));
    let args = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"sourceId":source});
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    let page = json!({"sourceId":source,"contentHash":"hash","entries":[{"path":"/body","value":"实际原文"}],"nextCursor":null,"contentRead":true});
    for meta in [
        child.clone(),
        child.clone(),
        child,
        json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root"}),
    ] {
        let server = response(
            &listener,
            format!(
                "GET /api/projects/project/tasks/task/task-source-content?threadId=root&expectedRevision=0&sourceId={source} HTTP/1.1"
            ),
            page.clone(),
        );
        let reply = super::transport_fixture::rpc(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read_source_content","arguments":args,"_meta":meta}}), &client).unwrap();
        assert_eq!(
            text(&reply),
            {let mut expected=page.clone();expected["nextRead"]=Value::Null;expected.as_object_mut().unwrap().remove("nextCursor");expected},
            "child observations must not contribute to root repeated-read hints"
        );
        server.join().unwrap();
    }
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
    let mut registered = text(&invoke(
        &client,
        "request_generation_handoff",
        proposal.clone(),
        root.clone(),
    ));
    let instruction = registered
        .as_object_mut()
        .unwrap()
        .remove("instruction")
        .unwrap();
    assert!(
        instruction
            .as_str()
            .unwrap()
            .contains("registered, not executed")
    );
    assert!(instruction.as_str().unwrap().len() < 600);
    assert_eq!(
        registered,
        json!({"proposalId":"proposal-id","targetChapters":2,"state":"proposed","resumeRun":null,"approvalRequired":true})
    );
    receiver.join().unwrap();
    let resume = json!({"id":"generation-existing","expectedVersion":7});
    proposal["resumeRun"] = resume.clone();
    let receiver = response(
        &listener,
        "POST /api/projects/project/tasks/task/generation-handoff HTTP/1.1".into(),
        json!({"proposal":{"proposalId":"resume-id","projectId":"project","sourceTaskId":"task","threadId":"root","targetChapters":2,"resumeRun":resume}}),
    );
    let selected = text(&invoke(
        &client,
        "request_generation_handoff",
        proposal.clone(),
        root.clone(),
    ));
    assert_eq!(selected["approvalRequired"], false);
    assert_eq!(selected["resumeRun"], resume);
    assert!(
        selected["instruction"]
            .as_str()
            .unwrap()
            .contains("continuation goal")
    );
    assert!(selected.to_string().len() < 1200);
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
fn native_source_errors_preserve_chinese_recovery_without_silently_retrying() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = super::query_plan_schema_tests::client(&listener);
    client.operation = "advisory".into();
    let args = json!({"projectId":"project", "taskId":"task", "threadId":"thread", "expectedRevision":1,
        "sourceId":format!("source-{}", "a".repeat(64))});
    let meta = json!({"rootThreadId":"thread","callerThreadId":"thread","sessionId":"thread"});
    for view in ["raw", "content"] {
        let tool = "read_source_content";
        let mut args = args.clone(); args["view"] = json!(view);
        let endpoint = if view == "raw" {
            "task-source"
        } else {
            "task-source-content"
        };
        for (status, code) in [
            (409, "task_source_catalog_required"),
            (409, "task_source_not_in_catalog"),
            (400, "task_source_cursor_invalid"),
        ] {
            let original = json!({"error":{"code":code,"message":"请先调用 read_context 获取本轮来源目录。",
                "details":{"taskId":"task","retryable":true,"sourceIsolation":"currentTaskOnly"},
                "recovery":{"tool":"read_context","arguments":{"expectedRevision":1},"discardPreviousSourceIdAndCursor":true}}});
            let receiver = response_status(
                &listener,
                format!(
                    "GET /api/projects/project/tasks/task/{endpoint}?threadId=thread&expectedRevision=1&sourceId={} HTTP/1.1",
                    args["sourceId"].as_str().unwrap()
                ),
                status,
                original.clone(),
            );
            let result = invoke(&client, tool, args.clone(), meta.clone());
            assert_eq!(result["result"]["isError"], true);
            assert_eq!(
                text(&result),
                original,
                "The MCP must forward Native recovery without guessing from error text"
            );
            receiver.join().unwrap();
        }
    }
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
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
    let plan = json!({"failedTaskId":"failed","cause":"参数错误","change":"读取结构后只提交必要字段","verification":"核对保存回执"});
    args["recoveryPlan"] = plan.clone();
    let (_, _, body) = super::task_source_tools::request("request_work_action", &args).unwrap();
    assert_eq!(body.unwrap()["recoveryPlan"], plan);
    args["recoveryPlan"]["unexpected"] = json!(true);
    assert!(super::task_source_tools::request("request_work_action", &args).is_err());
    args["recoveryPlan"] = plan;
    args["recoveryPlan"]["change"] = json!("x".repeat(601));
    assert!(super::task_source_tools::request("request_work_action", &args).is_err());
    let tool = super::task_source_tools::specs()
        .into_iter()
        .find(|tool| tool["name"] == "request_work_action")
        .unwrap();
    assert_eq!(
        tool["inputSchema"]["properties"]["recoveryPlan"]["additionalProperties"],
        false
    );
    let description = tool["description"].as_str().unwrap();
    assert!(description.contains("existing approved planning target"));
    assert!(description.contains("prior same-operation Task is unnecessary"));
    assert!(description.contains("Brief-only authorization never permits Outline or Plan"));
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
        let mut invalid = base.clone();
        invalid["proposal"] = bad;
        assert!(super::task_source_tools::request("request_work_action", &invalid).is_err());
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = super::query_plan_schema_tests::client(&listener);
    let receiver = response(
        &listener,
        "POST /api/projects/project/tasks/decision/work-decision/actions HTTP/1.1".into(),
        json!({"step":{"operation":"generateChapter"}}),
    );
    let mut valid = base;
    valid["proposal"] = proposal;
    let result = invoke(
        &client,
        "request_work_action",
        valid,
        json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root"}),
    );
    assert_eq!(text(&result)["step"]["operation"], "generateChapter");
    receiver.join().unwrap();
}

#[test]
fn source_first_page_omits_cursor_and_rejects_ambiguous_sentinels_with_guidance() {
    for view in ["content", "raw", "index"] {
        let name = "read_source_content";
        let spec = super::task_source_tools::specs()
            .into_iter()
            .find(|s| s["name"] == name)
            .unwrap();
        let args = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":0,"sourceId":format!("source-{}","a".repeat(64)),"view":view});
        super::argument_validation::ArgumentValidation::default()
            .validate(name, &args, &spec["inputSchema"])
            .unwrap();
        assert!(
            !super::task_source_tools::request(name, &args)
                .unwrap()
                .1
                .contains("cursor=")
        );
        for bad in [json!(null), json!("null"), json!("0"), json!("first")] {
            let mut invalid = args.clone();
            invalid["cursor"] = bad;
            let error = super::argument_validation::ArgumentValidation::default()
                .validate(name, &invalid, &spec["inputSchema"])
                .unwrap_err();
            let feedback: Value = serde_json::from_str(&error).unwrap();
            assert!(
                feedback["fieldInstruction"]
                    .as_str()
                    .unwrap()
                    .contains("Omit this field")
            );
        }
        let mut next = args.clone();
        let cursor = if view == "content" { format!("{}:12:37", "b".repeat(64)) }
            else { format!("{}:12", "b".repeat(64)) };
        next["cursor"] = json!(cursor);
        super::argument_validation::ArgumentValidation::default()
            .validate(name, &next, &spec["inputSchema"])
            .unwrap();
        let path = super::task_source_tools::request(name, &next).unwrap().1;
        assert!(path.ends_with(&format!("&cursor={}", super::percent_encode(&cursor))));
    }
}

#[test]
fn conversation_run_view_is_explicit_and_cannot_reuse_a_work_cursor() {
    let mut args = json!({"projectId":"project","taskId":"post-edit","threadId":"thread","expectedRevision":3,"view":"conversationRuns"});
    assert!(
        super::task_source_tools::request("read_work_status", &args)
            .unwrap()
            .1
            .ends_with("&view=conversationRuns")
    );
    args["cursor"] = json!("1:abcdef");
    assert!(super::task_source_tools::request("read_work_status", &args).is_err());
    args["cursor"] = Value::Null;
    args["view"] = json!("all");
    assert!(super::task_source_tools::request("read_work_status", &args).is_err());
}

#[test]
fn source_index_is_explicit_and_never_routes_raw_reads() {
    let args = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":0,"sourceId":format!("source-{}","a".repeat(64)),"view":"index"});
    let spec = super::task_source_tools::specs()
        .into_iter()
        .find(|s| s["name"] == "read_source_content")
        .unwrap();
    super::argument_validation::ArgumentValidation::default()
        .validate("read_source_content", &args, &spec["inputSchema"])
        .unwrap();
    assert!(
        super::task_source_tools::request("read_source_content", &args)
            .unwrap()
            .1
            .ends_with("&view=index")
    );
    let mut raw=args.clone();raw["view"]=json!("raw");
    assert!(super::task_source_tools::request("read_source_content", &raw).unwrap().1.contains("/task-source?"));
    let mut invalid = args;
    invalid["view"] = json!("all");
    assert!(super::task_source_tools::request("read_source_content", &invalid).is_err());
}

#[test]
fn chapter_finishing_reuses_persisted_assessment_and_direct_review_keeps_evidence() {
    let tool = super::task_source_tools::specs().into_iter()
        .find(|t| t["name"] == "request_work_action").unwrap();
    let schema = &tool["inputSchema"];
    let validator = jsonschema::validator_for(schema).unwrap();
    let finish = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":0,
        "expectedWorkVersion":0,"expectedSourceDigest":"digest","actionKey":"key",
        "instruction":"结束已通过核验的章节","idempotencyKey":"finish"});
    assert!(validator.is_valid(&finish));
    let mut duplicated = finish.clone();
    duplicated["chapterReview"] = json!({"sourceRevisionId":"saved","assessmentRevisionId":"report",
        "checks":[{"criterionId":"timeline","issueId":null}],
        "length":{"requirement":"无篇幅限制","minCharacters":null,"maxCharacters":null}});
    assert!(!validator.is_valid(&duplicated));
    let mut direct = finish.clone();
    direct["chapterReview"] = json!({"outcomes":[{"id":"timeline","status":"supported",
        "reason":"原文明确时间","evidence":[{"groupId":"scene","quote":"子时"}],"issue":null}],
        "uncoveredRequirements":[]});
    assert!(validator.is_valid(&direct));
    let (_,path,body)=super::task_source_tools::request("request_work_action",&direct).unwrap();
    assert!(path.ends_with("/work-decision/actions"));
    assert_eq!(body.unwrap()["chapterReview"],direct["chapterReview"]);
    direct["chapterReview"]["outcomes"][0]["evidence"][0]["quote"]=json!("");
    assert!(!validator.is_valid(&direct));
    assert_eq!(schema["properties"]["chapterAcceptance"]["properties"]["requirements"]["uniqueItems"],true);
}

#[test]
fn work_issue_read_is_task_bound_bounded_and_preserves_trusted_child_identity() {
    let args = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":3,"issueId":null,"cursor":0,"limit":16});
    let spec = super::task_source_tools::specs()
        .into_iter()
        .find(|t| t["name"] == "read_work_issues")
        .unwrap();
    let validator = jsonschema::validator_for(&spec["inputSchema"]).unwrap();
    assert!(validator.is_valid(&args));
    let (method, path, body) =
        super::task_source_tools::request("read_work_issues", &args).unwrap();
    assert_eq!(method, "GET");
    assert!(body.is_none());
    assert_eq!(
        path,
        "/api/projects/project/tasks/task/work-issues?threadId=root&expectedRevision=3&cursor=0&limit=16"
    );
    for (field, value) in [
        ("workTaskId", json!("forged-work")),
        ("cursor", json!(null)),
        ("cursor", json!("0")),
        ("cursor", json!(-1)),
        ("limit", json!(17)),
        ("issueId", json!("正文问题")),
    ] {
        let mut bad = args.clone();
        bad[field] = value;
        assert!(!validator.is_valid(&bad), "{field}");
    }
    let mut missing = args.clone();
    missing.as_object_mut().unwrap().remove("issueId");
    assert!(!validator.is_valid(&missing));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 2048];
        while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
            let size = socket.read(&mut buffer).unwrap();
            assert!(size > 0);
            bytes.extend_from_slice(&buffer[..size]);
        }
        let header = String::from_utf8_lossy(&bytes);
        assert_eq!(
            header.lines().next().unwrap(),
            format!("GET {path} HTTP/1.1")
        );
        for expected in [
            "X-Novel-Root-Thread: root",
            "X-Novel-Caller-Thread: child",
            "X-Novel-Parent-Thread: root",
            "X-Novel-Tool-Call: issues-read",
        ] {
            assert!(
                header.contains(expected),
                "missing trusted header {expected}: {header}"
            );
        }
        let payload = json!({"entries":[],"totalCount":0,"nextCursor":null}).to_string();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        )
        .unwrap();
    });
    let client = RuntimeClient {
        tool_binding: Default::default(), request_dispatched: Default::default(),
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port,
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "assessRevisionOutcome".into(),
    };
    let meta = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root","turnId":"turn","callId":"issues-read"});
    let result = invoke(&client, "read_work_issues", args.clone(), meta);
    assert!(result["result"].get("isError").is_none());
    worker.join().unwrap();
    let foreign = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"other","sessionId":"root","turnId":"turn","callId":"foreign-issues"});
    let result = invoke(&client, "read_work_issues", args, foreign);
    assert_eq!(result["result"]["isError"], true);
}

#[test]
fn report_read_and_pause_are_root_coordinator_tools_with_exact_binding() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = super::query_plan_schema_tests::client(&listener);
    let binding =
        json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0});
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root"});
    let mut read = binding.clone();
    read["writerTaskId"] = json!("writer/one");
    let report = json!({"state":"current","assessment":{"writerTaskId":"writer/one","chapterId":"chapter-one","originalRevisionId":null,"draftRevisionId":"saved","knowledgeBatchId":null,"uncoveredRequirements":[],"outcomes":[{"id":"timeline","status":"unmet","reason":"当前正文尚未满足前后时间关系","issue":{"issueId":"timeline-problem","newIssue":{"summary":"前后时间关系未满足"},"resolutionOf":[]},"evidence":[{"groupId":"scene-one","quote":"实际正文"}]}]}});
    let server=response(&listener,"GET /api/projects/project/tasks/task/revision-outcome?threadId=root&expectedRevision=0&writerTaskId=writer%2Fone HTTP/1.1".into(),report.clone());
    assert_eq!(
        text(&invoke(
            &client,
            "read_revision_outcome",
            read.clone(),
            root.clone()
        )),
        report
    );
    server.join().unwrap();
    let mut pause = binding;
    pause["runId"] = json!("run-one");
    pause["expectedVersion"] = json!(8);
    pause["expectedIntentRevision"] = json!(0);
    pause["instruction"] = json!("先保留成果并暂停");
    pause["idempotencyKey"] = json!("pause");
    let receipt = json!({"receipt":{"status":"paused","version":9}});
    let server = response(
        &listener,
        "POST /api/projects/project/tasks/task/generation-control HTTP/1.1".into(),
        receipt.clone(),
    );
    assert_eq!(
        text(&invoke(
            &client,
            "pause_generation_run",
            pause.clone(),
            root.clone()
        )),
        receipt
    );
    server.join().unwrap();
    listener.set_nonblocking(true).unwrap();
    for operation in ["generateChapter", "assessRevisionOutcome", "modelKnowledge"] {
        let mut worker = super::query_plan_schema_tests::client(&listener);
        worker.operation = operation.into();
        for (tool, args) in [
            ("read_revision_outcome", read.clone()),
            ("pause_generation_run", pause.clone()),
        ] {
            assert_eq!(
                invoke(&worker, tool, args, root.clone())["result"]["isError"],
                true
            );
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
    for (tool, args) in [
        ("read_revision_outcome", read),
        ("pause_generation_run", pause),
    ] {
        assert_eq!(
            invoke(
                &client,
                tool,
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
}

#[test]
fn workspace_locator_binds_current_task_without_source_and_carries_exact_revision() {
    let args = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":0,"view":"workspace","chapterNumber":31,"revisionId":"revision-31"});
    let spec = super::task_source_tools::specs().into_iter().find(|s|s["name"]=="read_source_content").unwrap();
    let validator = super::argument_validation::ArgumentValidation::default();
    validator.validate("read_source_content", &args, &spec["inputSchema"]).unwrap();
    let (_, path, _) = super::task_source_tools::request("read_source_content", &args).unwrap();
    assert!(path.contains("revisionId=revision-31"));
    assert!(!path.contains("sourceId="));
    let mut missing = args.clone(); missing.as_object_mut().unwrap().remove("view");
    assert!(validator.validate("read_source_content", &missing, &spec["inputSchema"]).is_err());
    assert!(super::task_source_tools::request("read_source_content", &missing).is_err());
    let mut body = args; body["view"]=json!("content"); body["sourceId"]=json!(format!("source-{}", "a".repeat(64)));
    assert!(super::task_source_tools::request("read_source_content", &body).is_err());
}

#[test]
fn locate_parallel_missing_sources_are_one_correction_opportunity_with_exact_feedback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = Ok(super::query_plan_schema_tests::client(&listener));
    let invoke = |batch: &str, call: usize| {
        super::transport_fixture::rpc(&json!({"jsonrpc":"2.0","id":call,"method":"tools/call","params":{
            "name":"read_source_content",
            "arguments":{"projectId":"project","taskId":"decision","threadId":"root","expectedRevision":2,
                "view":"locate","groupId":"scene-one","quote":"PRIVATE_PROSE"},
            "_meta":{"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn",
                "callId":format!("call-{call}"),"samplingRequestId":batch}
        }}), &client).unwrap()["result"]["structuredContent"].clone()
    };
    for call in 0..8 {
        let feedback = invoke("sampling-one", call);
        assert_eq!(feedback["code"], "invalid_tool_arguments");
        assert_eq!(feedback["attempt"], 1);
        assert_eq!(feedback["missingField"], "sourceId");
        assert_eq!(feedback["reason"], "required");
        assert!(!feedback.to_string().contains("PRIVATE_PROSE"));
        work_choice_no_dispatch(&client, &listener);
    }
    // An unrelated valid call in the same sample cannot erase its failures.
    client.as_ref().unwrap().argument_validation.complete_preflight("read_source_content");
    assert_eq!(invoke("sampling-two", 8)["attempt"], 2);
    assert_eq!(invoke("sampling-three", 9)["code"], "tool_argument_correction_limit");
    work_choice_no_dispatch(&client, &listener);
}

#[test]
fn later_valid_sampling_preflight_resets_only_the_corrected_tool() {
    let validation = super::argument_validation::ArgumentValidation::default();
    let schema = json!({"type":"object","required":["sourceId"]});
    validation.bind_sampling_batch(&json!({"samplingRequestId":"one"})).unwrap();
    for tool in ["read_source_content", "other"] {
        assert!(validation.validate(tool, &json!({}), &schema).is_err());
    }
    validation.bind_sampling_batch(&json!({"samplingRequestId":"two"})).unwrap();
    validation.validate("read_source_content", &json!({"sourceId":"source"}), &schema).unwrap();
    validation.complete_preflight("read_source_content");
    validation.bind_sampling_batch(&json!({"samplingRequestId":"three"})).unwrap();
    let feedback = |tool| serde_json::from_str::<Value>(&validation.validate(tool, &json!({}), &schema).unwrap_err()).unwrap();
    assert_eq!(feedback("read_source_content")["attempt"], 1);
    assert_eq!(feedback("other")["attempt"], 2);
}

#[test]
fn model_arguments_cannot_supply_missing_trusted_sampling_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = Ok(super::query_plan_schema_tests::client(&listener));
    let result = super::handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"read_source_content","arguments":{"view":"locate","quote":"private","samplingRequestId":"forged"},
        "_meta":{"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call"}
    }}), &client).unwrap();
    let error=&result["result"]["structuredContent"];
    assert_eq!(error["code"], "tool_binding_rejected");
    assert_eq!(error["error"]["details"]["instancePath"], "/samplingRequestId");
    work_choice_no_dispatch(&client, &listener);
}
