use super::RuntimeClient;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn advisory_hypothesis_calls_forward_only_current_scoped_actions_and_exact_tokens() {
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
        operation: "advisory".into(),
    };
    let mut client = make_client();
    let base = json!({"projectId":"project","taskId":"advisory","threadId":"root","expectedRevision":0,"action":"readBaseline","entityId":"master","limit":2});
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"hypothesis-call"});
    for (field, value) in [
        ("snapshotId", json!("foreign")),
        ("hypothesisId", json!("foreign")),
        ("limit", json!(17)),
        ("action", json!("publish")),
    ] {
        let client = make_client();
        let mut bad = base.clone();
        bad[field] = value;
        assert!(
            client
                .call("simulate_story_hypothesis", &bad, &meta)
                .is_err()
        );
    }
    let mut child = meta.clone();
    child["callerThreadId"] = json!("child");
    child["parentThreadId"] = json!("root");
    assert!(
        client
            .call("simulate_story_hypothesis", &base, &child)
            .is_err()
    );
    client.operation = "generateChapter".into();
    assert!(
        client
            .call("simulate_story_hypothesis", &base, &meta)
            .is_err()
    );
    client.operation = "advisory".into();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(false).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        let body: Value = serde_json::from_slice(&bytes[end + 4..]).unwrap();
                        requests.push((headers.to_owned(), body.clone()));
                        let response = json!({"received":body}).to_string();
                        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
                        break;
                    }
                }
            }
        }
        requests
    });
    let simulated = json!({"projectId":"project","taskId":"advisory","threadId":"root","expectedRevision":0,"action":"simulate","baselineToken":"exact opaque token🙂","logicalKey":"once","intent":"如果师父已死","limit":1,"assumptions":[{"kind":"assumeClaim","subjectId":"master","predicate":"alive","value":false,"storyTime":1}]});
    for input in [&base, &simulated] {
        let mut expected = input.clone();
        expected.as_object_mut().unwrap().remove("projectId");
        expected.as_object_mut().unwrap().remove("taskId");
        assert_eq!(
            client
                .call("simulate_story_hypothesis", input, &meta)
                .unwrap(),
            (200, json!({"received":expected}))
        );
    }
    for (headers, _) in server.join().unwrap() {
        assert!(headers.starts_with("POST /api/projects/project/tasks/advisory/hypothesis "));
        assert!(headers.contains("X-Novel-Tool-Name: simulate_story_hypothesis\r\n"));
        assert!(headers.contains("X-Novel-Root-Thread: root\r\n"));
    }
}
