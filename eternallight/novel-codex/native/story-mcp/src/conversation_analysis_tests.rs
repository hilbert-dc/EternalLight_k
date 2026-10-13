use std::io::{Read, Write};
use std::net::TcpListener;

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::RuntimeClient;

#[test]
fn advisory_analysis_tools_forward_bound_identity_and_reject_overrides_before_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = RuntimeClient {
        tool_binding: Default::default(), request_dispatched: Default::default(),
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
    let args = json!({"projectId":"project","taskId":"advisory","threadId":"root","expectedRevision":1,
        "entityId":"character-a","maxHops":1,"limit":2});
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"impact-call"});
    let mut foreign = args.clone();
    foreign["snapshotId"] = json!("foreign");
    assert!(
        client
            .call("analyze_story_impact", &foreign, &meta)
            .is_err()
    );
    let mut invalid = args.clone();
    invalid["limit"] = json!(21);
    assert!(
        client
            .call("analyze_story_impact", &invalid, &meta)
            .is_err()
    );
    let mut child = meta.clone();
    child["callerThreadId"] = json!("child");
    child["parentThreadId"] = json!("root");
    assert!(client.call("analyze_story_impact", &args, &child).is_err());
    client.operation = "generateChapter".into();
    assert!(client.call("analyze_story_impact", &args, &meta).is_err());
    client.operation = "advisory".into();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(false).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..3 {
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
            requests.push(headers);
        }
        requests
    });
    let expected = json!({"threadId":"root","expectedRevision":1,"entityId":"character-a","maxHops":1,"limit":2});
    assert_eq!(
        client.call("analyze_story_impact", &args, &meta).unwrap(),
        (200, json!({"received":expected}))
    );
    let action = json!({"projectId":"project","taskId":"advisory","threadId":"root","expectedRevision":1,
        "expectedIntentRevision":0,"operation":"modelKnowledge","target":{"chapterId":"chapter-a","chapterNumber":1,"sourceRevisionId":"source-a"},
        "instruction":"分析本章","idempotencyKey":"analysis-action"});
    let mut expected = action.clone();
    expected.as_object_mut().unwrap().remove("projectId");
    assert_eq!(
        client
            .call("request_candidate_task", &action, &meta)
            .unwrap(),
        (200, json!({"received":expected}))
    );
    let planning = json!({"projectId":"project","taskId":"advisory","threadId":"root","expectedRevision":1,
        "expectedIntentRevision":0,"artifactKind":"outline","revisionId":"outline-one","targetNodeId":null,
        "roles":["structure","character"],"focus":"检查人物动机","idempotencyKey":"review-once"});
    assert!(
        client
            .call("request_planning_review", &planning, &child)
            .is_err()
    );
    client.operation = "generateChapter".into();
    assert!(
        client
            .call("request_planning_review", &planning, &meta)
            .is_err()
    );
    client.operation = "advisory".into();
    let mut expected = planning.clone();
    expected.as_object_mut().unwrap().remove("projectId");
    expected.as_object_mut().unwrap().remove("taskId");
    assert_eq!(
        client
            .call("request_planning_review", &planning, &meta)
            .unwrap(),
        (200, json!({"received":expected}))
    );
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("POST /api/projects/project/tasks/advisory/impact-analysis "));
    assert!(requests[0].contains("X-Novel-Root-Thread: root\r\n"));
    assert!(requests[0].contains("X-Novel-Tool-Name: analyze_story_impact\r\n"));
    assert!(requests[1].starts_with("POST /api/projects/project/tasks/advisory/actions "));
    assert!(requests[2].starts_with("POST /api/projects/project/tasks/advisory/planning-reviews "));
    assert!(requests[2].contains("X-Novel-Tool-Name: request_planning_review\r\n"));
}
