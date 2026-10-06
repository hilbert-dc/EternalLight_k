use super::RuntimeClient;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
#[test]
fn assessment_exposes_only_reads_and_forwards_exact_source_role_without_caller_scope() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "assessChapterStructure".into(),
    };
    let tools = client
        .exposed_tools()
        .into_iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    for forbidden in [
        "save_brief",
        "save_draft_checkpoint",
        "request_candidate_task",
        "request_planning_review",
        "simulate_story_hypothesis",
        "checkpoint_reading",
        "publish_modeling_batch",
    ] {
        assert!(!tools.iter().any(|name| name == forbidden));
    }
    let arguments = json!({"projectId":"project","taskId":"assessment","threadId":"root","expectedRevision":2,"sourceRole":"saved","cursor":null,"limit":5});
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call"});
    let mut foreign = arguments.clone();
    foreign["snapshotId"] = json!("foreign");
    assert!(
        client
            .call("read_chapter_structure_source", &foreign, &meta)
            .is_err()
    );
    let mut large = arguments.clone();
    large["limit"] = json!(1001);
    assert!(
        client
            .call("read_chapter_structure_source", &large, &meta)
            .is_err()
    );
    let mut child = meta.clone();
    child["callerThreadId"] = json!("child");
    child["parentThreadId"] = json!("root");
    assert!(
        client
            .call("read_chapter_structure_source", &arguments, &child)
            .is_err()
    );
    client.operation = "advisory".into();
    assert!(
        client
            .call("read_chapter_structure_source", &arguments, &meta)
            .is_err()
    );
    client.operation = "assessChapterStructure".into();
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
        let (header, payload) = loop {
            let mut part = [0; 4096];
            let count = stream.read(&mut part).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&part[..count]);
            if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                let header = std::str::from_utf8(&bytes[..end]).unwrap();
                let size: usize = header
                    .lines()
                    .find_map(|l| l.strip_prefix("Content-Length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    break (
                        header.to_owned(),
                        serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + size]).unwrap(),
                    );
                }
            }
        };
        let result = json!({"received":payload}).to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{result}",
            result.len()
        )
        .unwrap();
        header
    });
    assert_eq!(
        client
            .call("read_chapter_structure_source", &arguments, &meta)
            .unwrap(),
        (
            200,
            json!({"received":{"threadId":"root","expectedRevision":2,"sourceRole":"saved","cursor":null,"limit":5}})
        )
    );
    let header = server.join().unwrap();
    assert!(
        header.starts_with("POST /api/projects/project/tasks/assessment/chapter-structure-source ")
    );
    assert!(header.contains("X-Novel-Tool-Name: read_chapter_structure_source\r\n"));
}
