use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{RuntimeClient};

fn client(listener: &TcpListener) -> RuntimeClient {
    RuntimeClient {
        tool_binding: Default::default(), request_dispatched: Default::default(),
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "synthetic-candidate".into(),
        capability: "synthetic-capability".into(),
        operation: "generateChapter".into(),
    }
}

fn arguments() -> Value {
    json!({"projectId":"project","taskId":"writer","threadId":"root","expectedRevision":1,
        "idempotencyKey":"checkpoint","expectedCandidateRevisionId":null,
        "payload":{"chapterId":"chapter-one","chapterNumber":1,"title":"门缝里的信",
            "body":"信到了。","completeness":"chapterComplete"}})
}

fn root() -> Value {
    json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root",
        "turnId":"turn","callId":"save-checkpoint"})
}

fn rpc_call(client: &RuntimeClient, args: &Value) -> Value {
    super::transport_fixture::rpc(
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"save_draft_checkpoint","arguments":args,"_meta":root()}}),
        &Ok(RuntimeClient {
        tool_binding: Default::default(), request_dispatched: Default::default(),
            publication_gate: super::publication_gate::PublicationGate::default(),
            read_progress: Default::default(),
            completion_receipt: Default::default(),
            argument_validation: Default::default(),
            host: client.host.clone(),
            port: client.port,
            candidate_token: client.candidate_token.clone(),
            capability: client.capability.clone(),
            operation: client.operation.clone(),
        }),
    )
    .unwrap()
}

fn read_request(stream: &mut TcpStream) -> (String, Value) {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    loop {
        let mut bytes = [0; 4096];
        let count = stream.read(&mut bytes).unwrap();
        assert!(count > 0);
        request.extend_from_slice(&bytes[..count]);
        if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&request[..end]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            if request.len() >= end + 4 + length {
                return (
                    headers.to_owned(),
                    if length == 0 { Value::Null } else { serde_json::from_slice(&request[end + 4..end + 4 + length]).unwrap() },
                );
            }
        }
    }
}

fn saved_receipt() -> Value {
    json!({"chapterId":"chapter-one","checkpointId":"checkpoint-stored",
        "revisionId":"draft-stored","version":1,"completeness":"chapterComplete"})
}

fn saved_dto(body: &Value) -> Value {
    let mut result = saved_receipt();
    result["body"] = body["payload"]["body"].clone();
    result["sceneGroups"] = body["payload"]["sceneGroups"].clone();
    result
}

fn respond(stream: &mut TcpStream, status: &str, body: &Value) {
    let body = body.to_string();
    write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

#[test]
fn malformed_draft_payloads_never_dispatch_and_the_same_task_can_save_after_correction() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = client(&listener);
    let spec = client
        .operation_tools()
        .into_iter()
        .find(|tool| tool["name"] == "save_draft_checkpoint")
        .unwrap();
    let transport = &spec["inputSchema"]["properties"]["payload"];
    let transport_validator = jsonschema::options().build(transport).unwrap();
    let mut malformed = Vec::new();
    // A semantic omission passes the transport but produces a real MCP error before HTTP.
    for field in super::draft_schema::payload()["required"]
        .as_array()
        .unwrap()
    {
        let field = field.as_str().unwrap();
        let mut payload = arguments()["payload"].clone();
        payload.as_object_mut().unwrap().remove(field);
        assert!(transport_validator.is_valid(&payload));
        malformed.push((
            payload,
            format!("payload.{field}"),
            "required field".to_owned(),
        ));
    }
    let group = json!({"id":"stable","body":"信到了。","locked":false});
    for field in ["id", "body", "locked"] {
        let mut payload = arguments()["payload"].clone();
        let mut missing = group.clone();
        missing.as_object_mut().unwrap().remove(field);
        payload.as_object_mut().unwrap().remove("body");
        payload["sceneGroups"] = json!([missing]);
        malformed.push((
            payload,
            format!("payload.sceneGroups[0].{field}"),
            "required field".to_owned(),
        ));
    }
    let mut wrong_type = arguments()["payload"].clone();
    wrong_type["body"] = json!({"private-key":"private prose"});
    malformed.push((wrong_type, "payload.body".into(), "type \"string\"".into()));
    let mut duplicate = arguments()["payload"].clone();
    duplicate.as_object_mut().unwrap().remove("body");
    duplicate["sceneGroups"] = json!([group.clone(), group.clone()]);
    malformed.push((
        duplicate,
        "payload.sceneGroups[1].id".into(),
        "unique group ID".into(),
    ));
    let mut both = arguments()["payload"].clone();
    both["sceneGroups"] = json!([group]);
    let mut neither = arguments()["payload"].clone();
    neither.as_object_mut().unwrap().remove("body");
    for payload in [both, neither] {
        malformed.push((
            payload,
            "payload".into(),
            "provide exactly one of body or sceneGroups".into(),
        ));
    }
    let mut oversized = arguments()["payload"].clone();
    oversized.as_object_mut().unwrap().remove("body");
    oversized["sceneGroups"] = json!([
        {"id":"first","body":"字".repeat(60001),"locked":false},
        {"id":"second","body":"字".repeat(60001),"locked":false}
    ]);
    malformed.push((oversized, "payload.body".into(), "maxLength 120000".into()));
    for (payload, path, expected) in malformed {
        let mut args = arguments();
        args["payload"] = payload;
        let response = rpc_call(&client, &args);
        let diagnostic =
            super::query_plan_schema_tests::parameter_error(&response, "save_draft_checkpoint");
        assert_eq!(
            diagnostic["detail"],
            json!({"code":"invalid_draft_payload","path":path,"expected":expected})
        );
        assert!(!response.to_string().contains("private prose"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    listener.set_nonblocking(false).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = super::recovery_proof::accept_business_request(&listener);
        let request = read_request(&mut stream);
        respond(&mut stream, "201 Created", &saved_dto(&request.1));
        request
    });
    let reply = rpc_call(&client, &arguments());
    assert_eq!(reply["id"],1);
    let receipt: Value = serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    for field in ["chapterId","checkpointId","revisionId","version"] {
        assert_eq!(receipt[field],saved_receipt()[field]);
    }
    assert_eq!(receipt["saved"],true);
    assert_eq!(receipt["nextAction"],"complete_task");
    assert!(receipt["instruction"].as_str().unwrap().contains("pre-edit draft"));
    let (headers, body) = server.join().unwrap();
    assert!(headers.starts_with("POST /api/projects/project/drafts/checkpoints HTTP/1.1"));
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("x-novel-root-thread: root\r\n")
    );
    let mut expected = arguments();
    expected.as_object_mut().unwrap().remove("projectId");
    assert_eq!(body, expected);
}

#[test]
fn valid_checkpoints_preserve_optional_groups_metadata_empty_text_and_exact_cas() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = client(&listener);
    let mut cases = vec![arguments()];
    let mut empty = arguments();
    empty["payload"]["body"] = json!("");

    empty["payload"]["completeness"] = json!("partial");
    cases.push(empty.clone());
    empty["payload"].as_object_mut().unwrap().remove("body");
    empty["payload"]["sceneGroups"] = json!([]);
    cases.push(empty);
    let mut grouped = arguments();
    grouped["payload"].as_object_mut().unwrap().remove("body");
    grouped["payload"]["sourcePlanRefs"] = json!({"localOnly":true,"planRevisionId":null});
    grouped["payload"]["sceneGroups"] = json!([
        {"id":"受保护原始身份-".repeat(100),"sceneId":null,"body":" \n信到了。\t ","locked":true,
            "sceneOrder":1,"povCharacterId":"character-one","protectedOrigin":{"revisionId":"previous"}}
    ]);
    grouped["expectedCandidateRevisionId"] = json!("previous");
    cases.push(grouped);
    let mut planned = arguments();
    planned["payload"].as_object_mut().unwrap().remove("body");
    planned["payload"]["sceneGroups"] = json!([
        {"id":"actual-one","sceneId":"bound-scene-one","body":" \n信到了。 ","locked":false},
        {"id":"transition","sceneId":null,"body":"夜色渐深。\n ","locked":false}
    ]);
    cases.push(planned);
    let mut literal_null = arguments();
    literal_null["expectedCandidateRevisionId"] = json!("null");
    cases.push(literal_null);
    let count = cases.len();
    let rejected = json!({"error":{"code":"conflict","message":"TaskInputStale"}});
    let rejection = rejected.clone();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..count {
            let (mut stream, _) = super::recovery_proof::accept_business_request(&listener);
            requests.push(read_request(&mut stream).1);
            if index == count - 1 {
                respond(&mut stream, "409 Conflict", &rejection);
            } else {
                respond(&mut stream, "201 Created", &saved_dto(requests.last().unwrap()));
            }
        }
        requests
    });
    for (index, args) in cases.iter().enumerate() {
        let (status, receipt) = client.call("save_draft_checkpoint", args, &root()).unwrap();
        if index == count - 1 {
            assert_eq!((status, receipt), (409, rejected.clone()));
        } else {
            assert_eq!(status, 201);
            for field in ["chapterId", "checkpointId", "revisionId", "version"] {
                assert_eq!(receipt[field], saved_receipt()[field]);
            }
            assert_eq!(receipt["saved"], true);
            assert_eq!(receipt["nextAction"], "complete_task");
            assert!(receipt["instruction"].as_str().unwrap().contains("pre-edit draft"));
            assert!(receipt.get("body").is_none());
            assert!(receipt.get("sceneGroups").is_none());
            assert!(receipt.to_string().len() < 1000);
        }
    }
    let expected = cases
        .into_iter()
        .enumerate()
        .map(|(index, mut args)| {
            // Literal expected Runtime full texts verify the boundary independently.
            match index {
                2 => args["payload"]["body"] = json!(""),
                3 => args["payload"]["body"] = json!("信到了。"),
                4 => args["payload"]["body"] = json!("信到了。\n\n---\n\n夜色渐深。"),
                0 | 1 | 5 => {}
                _ => unreachable!("fixed test cases"),
            }
            args.as_object_mut().unwrap().remove("projectId");
            args
        })
        .collect::<Vec<_>>();
    assert_eq!(server.join().unwrap(), expected);
}

#[test]
fn long_saved_prose_returns_a_bounded_receipt_without_changing_the_write() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = client(&listener);
    let mut args = arguments();
    args["payload"]["body"] = json!("沈照将灯芯藏进旧木箱，姐姐的名字仍在铜牌上。".repeat(250));
    let server = std::thread::spawn(move || {
        let (mut stream, _) = super::recovery_proof::accept_business_request(&listener);
        let (_, request) = read_request(&mut stream);
        let mut dto = saved_dto(&request);
        dto["sceneGroups"] = json!([{"id":"opening","body":dto["body"],"locked":false}]);
        assert!(dto.to_string().len() > 28000);
        respond(&mut stream, "201 Created", &dto);
        request
    });
    let (status, receipt) = client.call("save_draft_checkpoint", &args, &root()).unwrap();
    assert_eq!(status, 201);
    for field in ["chapterId", "checkpointId", "revisionId", "version"] {
        assert_eq!(receipt[field], saved_receipt()[field]);
    }
    assert_eq!(receipt["saved"], true);
    assert_eq!(receipt["nextAction"], "complete_task");
    assert!(receipt["instruction"].as_str().unwrap().contains("frozen input"));
    assert!(receipt.to_string().len() < 1000);
    let mut expected = args;
    expected.as_object_mut().unwrap().remove("projectId");
    assert_eq!(server.join().unwrap(), expected);
}

#[test]
fn invalid_success_receipt_binding_never_claims_a_saved_result() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = client(&listener);
    let mut cases = vec![];
    let mut wrong = saved_receipt();
    wrong["chapterId"] = json!("another-chapter");
    cases.push(wrong);
    for field in ["chapterId", "checkpointId", "revisionId", "version"] {
        let mut missing = saved_receipt();
        missing.as_object_mut().unwrap().remove(field);
        cases.push(missing);
    }
    let mut invalid = saved_receipt();
    invalid["version"] = json!("1");
    cases.push(invalid);
    let count = cases.len();
    let server = std::thread::spawn(move || {
        for dto in cases {
            let (mut stream, _) = super::recovery_proof::accept_business_request(&listener);
            read_request(&mut stream);
            respond(&mut stream, "201 Created", &dto);
        }
    });
    for _ in 0..count {
        assert!(client.call("save_draft_checkpoint", &arguments(), &root()).unwrap_err()
            .contains("inspect the stored checkpoint before retrying"));
    }
    server.join().unwrap();
}

#[test]
fn omitted_candidate_cas_refreshes_task_owned_binding_and_replay_keeps_model_hash() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = client(&listener);
    let mut args = arguments();
    for field in ["projectId","taskId","threadId","expectedRevision","idempotencyKey","expectedCandidateRevisionId"] {
        args.as_object_mut().unwrap().remove(field);
    }
    let expected_hash=super::operation_digest::operation_digest("save_draft_checkpoint",&args).unwrap();
    let server=std::thread::spawn(move || {
        for index in 0..7 {
            let (mut stream,_)=listener.accept().unwrap();
            let (headers,body)=read_request(&mut stream);
            if [0,2,4].contains(&index) {
                assert!(headers.starts_with("GET /api/tool-binding HTTP"));
                let revision=match index {0=>Value::Null,2=>json!("draft-one"),_=>json!("draft-two")};
                respond(&mut stream,"200 OK",&json!({"projectId":"project","taskId":"writer","threadId":"root","operation":"generateChapter","inputRevision":1,
                    "output":{"objectId":"draft-chapter-one","expectedCandidateRevisionId":revision}}));
            } else {
                assert!(headers.starts_with("POST /api/projects/project/drafts/checkpoints HTTP"));
                if index==6 {
                    assert_eq!(body["expectedCandidateRevisionId"],"null");
                    respond(&mut stream,"409 Conflict",&json!({"error":{"code":"candidate_expectation_mismatch"}}));
                } else {
                    assert!(headers.contains(&format!("X-Novel-Request-Hash: {expected_hash}")));
                    assert_eq!(body["expectedCandidateRevisionId"],match index {1=>Value::Null,3=>json!("draft-one"),_=>json!("draft-two")});
                    let mut receipt=saved_dto(&body);
                    receipt["revisionId"]=json!(if index==3 {"draft-two"} else {"draft-one"});
                    respond(&mut stream,"201 Created",&receipt);
                }
            }
        }
    });
    let mut first=root(); first["samplingRequestId"]=json!("sample-one"); let mut second=root(); second["samplingRequestId"]=json!("sample-two");second["callId"]=json!("second-save");
    assert_eq!(client.call_model("save_draft_checkpoint",&args,&first).unwrap().1["revisionId"],"draft-one");
    assert_eq!(client.call_model("save_draft_checkpoint",&args,&second).unwrap().1["revisionId"],"draft-two");
    assert_eq!(client.call_model("save_draft_checkpoint",&args,&first).unwrap().1["revisionId"],"draft-one");
    args["expectedCandidateRevisionId"]=json!("null");
    assert_eq!(client.call_model("save_draft_checkpoint",&args,&second).unwrap().0,409);
    server.join().unwrap();
}
