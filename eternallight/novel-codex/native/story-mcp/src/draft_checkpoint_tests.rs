use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{RuntimeClient, handle_message};

fn client(listener: &TcpListener) -> RuntimeClient {
    RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
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
    handle_message(
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"save_draft_checkpoint","arguments":args,"_meta":root()}}),
        &Ok(RuntimeClient {
            publication_gate: super::publication_gate::PublicationGate::default(),
            read_progress: Default::default(),
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
                    serde_json::from_slice(&request[end + 4..]).unwrap(),
                );
            }
        }
    }
}

#[test]
fn malformed_draft_payloads_never_dispatch_and_the_same_task_can_save_after_correction() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = client(&listener);
    let spec = client
        .exposed_tools()
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
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        request
    });
    assert_eq!(
        rpc_call(&client, &arguments()),
        json!({"jsonrpc":"2.0","id":1,
        "result":{"content":[{"type":"text","text":"{}"}]}})
    );
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
    let response_body = rejected.to_string();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..count {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_request(&mut stream).1);
            let (status, body) = if index == count - 1 {
                ("409 Conflict", response_body.as_str())
            } else {
                ("201 Created", "{}")
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    for (index, args) in cases.iter().enumerate() {
        let expected = if index == count - 1 {
            (409, rejected.clone())
        } else {
            (201, json!({}))
        };
        assert_eq!(
            client.call("save_draft_checkpoint", args, &root()),
            Ok(expected)
        );
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
