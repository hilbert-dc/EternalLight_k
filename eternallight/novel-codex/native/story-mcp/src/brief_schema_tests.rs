use std::io::{Read, Write};
use std::net::TcpListener;

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{RuntimeClient, handle_message};

fn arguments() -> Value {
    json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,
        "expectedCandidateRevisionId":null,"idempotencyKey":"brief-save",
        "payload":{"premise":"潮汐信使","promise":"揭示预言代价","stakes":"","endingDirection":"保留记忆"}})
}

fn call(client: &RuntimeClient, args: Value) -> Value {
    let client = Ok(RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: client.host.clone(),
        port: client.port,
        candidate_token: client.candidate_token.clone(),
        capability: client.capability.clone(),
        operation: client.operation.clone(),
    });
    handle_message(
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"save_brief","arguments":args,"_meta":{"rootThreadId":"root",
        "callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"save-brief"}}}),
        &client,
    )
    .unwrap()
}

#[test]
fn malformed_brief_contract_never_dispatches_and_correction_preserves_exact_payload_and_cas() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "synthetic".into(),
        capability: "synthetic-capability".into(),
        operation: "generateBrief".into(),
    };
    let tool = client
        .exposed_tools()
        .into_iter()
        .find(|tool| tool["name"] == "save_brief")
        .unwrap();
    let validator = jsonschema::validator_for(&tool["inputSchema"]).unwrap();
    let mut malformed = Vec::new();
    for payload in [
        json!({}),
        json!({"premise":null}),
        json!({"premise":12}),
        json!({"premise":"简报","promise":12}),
        json!({"premise":"简报","stakes":null}),
        json!({"premise":"简报","endingDirection":[]}),
        json!({"chapterNumber":1,"kind":"draft","content":"章节正文"}),
        json!({"premise":"简报","title":"不属于简报"}),
    ] {
        let mut args = arguments();
        args["payload"] = payload;
        malformed.push(args);
    }
    for args in malformed {
        assert!(!validator.is_valid(&args));
        let result = call(&client, args);
        let diagnostic = super::query_plan_schema_tests::parameter_error(&result, "save_brief");
        assert!(
            diagnostic["schemaPath"]
                .as_str()
                .unwrap()
                .starts_with("/properties/payload")
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    listener.set_nonblocking(/*nonblocking*/ false).unwrap();
    let mut cases = vec![arguments()];
    let mut existing = arguments();
    existing["expectedCandidateRevisionId"] = json!("brief-existing");
    existing["payload"] = json!({"premise":"","promise":" \n\t"});
    cases.push(existing);
    let mut string_null = arguments();
    string_null["expectedCandidateRevisionId"] = json!("null");
    cases.push(string_null);
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
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
                                .starts_with("POST /api/projects/project/artifacts/brief HTTP/1.1")
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
                    "404 Not Found",
                    r#"{"error":{"code":"not_found","message":"CandidateNotFound"}}"#,
                )
            } else {
                ("201 Created", r#"{"revisionId":"brief-saved"}"#)
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
