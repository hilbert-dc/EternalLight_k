use std::io::{Read, Write};
use std::net::TcpListener;

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{RuntimeClient, handle_message};

fn arguments() -> Value {
    json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,
        "expectedCandidateRevisionId":null,"idempotencyKey":"plan-save","payload":{"chapters":[{
            "chapterPlanId":"chapter-plan-1","chapterNumber":1,"goal":"送出回信","povCharacterId":null,"storyTime":null,
            "scenes":[{"id":"scene-1","goal":"核对邮戳","beats":[],"locked":true,"expectedEffects":[{
                "id":"effect-1","dimension":"knowledge","description":"发现日期变化","commitmentRefs":[]
            }]}],"protectedOrigin":{"revisionId":"original"}}],"authorMetadata":{"retain":true}}})
}

fn call(client: &RuntimeClient, args: Value) -> Value {
    let client = Ok(RuntimeClient {
        request_dispatched: Default::default(),
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: client.host.clone(),
        port: client.port,
        candidate_token: client.candidate_token.clone(),
        capability: client.capability.clone(),
        operation: client.operation.clone(),
    });
    handle_message(
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"save_chapter_plans","arguments":args,"_meta":{"rootThreadId":"root",
        "callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"save-plan"}}}),
        &client,
    )
    .unwrap()
}

#[test]
fn chapter_plan_contract_rejects_bad_calls_before_http_and_forwards_corrected_payload_and_cas() {
    // This receiver observes real MCP dispatch; Runtime semantics are tested separately.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let client = RuntimeClient {
        request_dispatched: Default::default(),
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "synthetic".into(),
        capability: "synthetic-capability".into(),
        operation: "generatePlan".into(),
    };
    let tool = client
        .exposed_tools()
        .into_iter()
        .find(|tool| tool["name"] == "save_chapter_plans")
        .unwrap();
    let validator = jsonschema::validator_for(&tool["inputSchema"]).unwrap();
    let mut malformed = Vec::new();
    for payload in [
        json!({"plans":[]}),
        json!({"chapterPlans":[]}),
        json!({"chapters":[]}),
    ] {
        let mut args = arguments();
        args["payload"] = payload;
        malformed.push(args);
    }
    for (path, value) in [
        ("/payload/chapters/0/chapterNumber", json!("first")),
        ("/payload/chapters/0/scenes/0/expectedEffects", json!([])),
        (
            "/payload/chapters/0/scenes/0/expectedEffects/0/dimension",
            json!("fact"),
        ),
        (
            "/payload/chapters/0/scenes/0/expectedEffects/0/commitmentRefs",
            json!([{"type":"proposal","proposalId":"fake","secret-key":"secret-value"}]),
        ),
        ("/payload/chapters/0/povCharacterId", json!("")),
        ("/payload/chapters/0/storyTime", json!("tomorrow")),
    ] {
        let mut args = arguments();
        *args.pointer_mut(path).unwrap() = value;
        malformed.push(args);
    }
    let mut missing = arguments();
    missing["payload"]["chapters"][0]
        .as_object_mut()
        .unwrap()
        .remove("scenes");
    malformed.push(missing);
    for args in malformed {
        assert!(!validator.is_valid(&args));
        let result = call(&client, args);
        let diagnostic =
            super::query_plan_schema_tests::parameter_error(&result, "save_chapter_plans");
        assert!(
            diagnostic["schemaPath"]
                .as_str()
                .unwrap()
                .starts_with("/properties/payload")
        );
        let text = diagnostic.to_string();
        assert!(!text.contains("secret-key") && !text.contains("secret-value"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    listener.set_nonblocking(/*nonblocking*/ false).unwrap();
    let mut existing = arguments();
    existing["expectedCandidateRevisionId"] = json!("plan-existing");
    existing["payload"]["sourceOutlineRevisionId"] = json!("outline-candidate");
    existing["payload"]["chapters"][0]["scenes"][0]["beats"] =
        json!([{"id":"beat","description":"检查回信","locked":true}]);
    existing["payload"]["chapters"][0]["scenes"][0]["expectedEffects"][0]["commitmentRefs"] = json!([
        {"type":"commitment","commitmentId":"promise","revisionId":"commitment-source"},
        {"type":"proposal","reportRevisionId":"report","proposalId":"letter","sourceRevisionId":"draft-source"}]);
    let mut string_null = arguments();
    string_null["expectedCandidateRevisionId"] = json!("null");
    let cases = vec![arguments(), existing, string_null];
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..3 {
            let (mut stream, _) = super::recovery_proof::accept_business_request(&listener);
            stream
                .set_read_timeout(
                    /*dur*/ Some(std::time::Duration::from_secs(/*secs*/ 5)),
                )
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&request[..end]).unwrap();
                    let length = header
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        assert!(
                            header
                                .starts_with("POST /api/projects/project/artifacts/plan HTTP/1.1")
                        );
                        assert!(
                            header
                                .to_ascii_lowercase()
                                .contains("x-novel-root-thread: root\r\n")
                        );
                        requests.push(
                            serde_json::from_slice::<Value>(&request[end + 4..end + 4 + length])
                                .unwrap(),
                        );
                        break;
                    }
                }
            }
            let (status, body) = if index == 2 {
                (
                    "409 Conflict",
                    r#"{"error":{"code":"candidate_revision_conflict","message":"Candidate baseline does not match"}}"#,
                )
            } else {
                ("201 Created", r#"{"revisionId":"plan-saved"}"#)
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
        assert!(validator.is_valid(args));
        let result = call(&client, args.clone());
        if index == 2 {
            assert_eq!(result["result"]["isError"], json!(true));
        } else {
            assert!(result["result"]["isError"].is_null());
        }
    }
    let expected = cases
        .into_iter()
        .map(|mut args| {
            args.as_object_mut().unwrap().remove("projectId");
            args
        })
        .collect::<Vec<_>>();
    assert_eq!(server.join().unwrap(), expected);
}
