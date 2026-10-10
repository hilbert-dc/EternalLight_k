use std::io::{Read, Write};
use std::net::TcpListener;

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::RuntimeClient;

#[test]
fn exact_evidence_handler_retains_bound_source_and_never_corrects_quotes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let make_client = || RuntimeClient {
        request_dispatched: Default::default(),
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "synthetic-candidate".into(),
        capability: "synthetic-capability".into(),
        operation: "extractCommitments".into(),
    };
    let mut client = make_client();
    let args = json!({"projectId":"project","taskId":"extract","threadId":"root","expectedRevision":1,
        "sourceObjectId":"draft-chapter","sourceRevisionId":"source","quote":"原🙂文\n照抄"});
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"resolve-call"});
    for (field, value) in [
        ("snapshotId", json!("foreign")),
        ("quote", json!("字".repeat(2001))),
        ("quote", json!(null)),
        ("sourceRevisionId", json!("")),
    ] {
        // Independent malformed-shape cases each start a fresh task executor.
        let client = make_client();
        let mut invalid = args.clone();
        invalid[field] = value;
        let feedback: Value = serde_json::from_str(
            &client
                .call("resolve_story_evidence", &invalid, &meta)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(feedback["code"], "invalid_tool_arguments");
        assert_eq!(feedback["attempt"], 1);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    let mut child = meta.clone();
    child["callerThreadId"] = json!("child");
    child["parentThreadId"] = json!("root");
    assert!(
        client
            .call("resolve_story_evidence", &args, &child)
            .is_err()
    );
    client.operation = "generateChapter".into();
    assert!(client.call("resolve_story_evidence", &args, &meta).is_err());
    client.operation = "extractCommitments".into();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut mistyped = args.clone();
    mistyped["quote"] = Value::Null;
    let feedback: Value = serde_json::from_str(
        &client
            .call("resolve_story_evidence", &mistyped, &meta)
            .unwrap_err(),
    )
    .unwrap();
    assert_eq!(feedback["code"], "invalid_tool_arguments");
    assert_eq!(feedback["attempt"], 1);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(false).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let (headers, payload) = loop {
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break (
                        headers.to_owned(),
                        serde_json::from_slice::<Value>(&bytes[end + 4..]).unwrap(),
                    );
                }
            }
        };
        let response = json!({"received":payload}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        headers
    });
    let mut expected = args;
    expected.as_object_mut().unwrap().remove("projectId");
    expected.as_object_mut().unwrap().remove("taskId");
    assert_eq!(client.call("resolve_story_evidence",&json!({"projectId":"project","taskId":"extract","threadId":"root","expectedRevision":1,
        "sourceObjectId":"draft-chapter","sourceRevisionId":"source","quote":"原🙂文\n照抄"}),&meta).unwrap(),(200,json!({"received":expected})));
    let headers = server.join().unwrap();
    assert!(headers.starts_with("POST /api/projects/project/tasks/extract/source-evidence "));
    assert!(headers.contains("X-Novel-Root-Thread: root\r\n"));
    assert!(headers.contains("X-Novel-Tool-Name: resolve_story_evidence\r\n"));
}
