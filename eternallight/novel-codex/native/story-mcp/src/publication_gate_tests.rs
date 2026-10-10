use std::io::{Read, Write};
use std::net::TcpListener;

use serde_json::{Value, json};

use super::{RuntimeClient, publication_gate::PublicationGate};

#[test]
fn acknowledged_publication_closes_tools_but_preserves_root_completion_and_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = RuntimeClient {
        request_dispatched: Default::default(),
        publication_gate: PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "modelKnowledge".into(),
    };
    let server = std::thread::spawn(move || {
        let mut routes = Vec::new();
        for (status, response) in [
            (422, json!({"error":"invalid publication"})),
            (
                200,
                json!({"batchId":"batch","publicationState":"committed","receiptRef":"runtime:knowledge:batch","nextAction":"complete_task"}),
            ),
            (503, json!({"error":"temporarily unavailable"})),
            (200, json!({"id":"model","projectId":"project","threadId":"root","status":"completed","resultSummary":"已发布"})),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&request[..end]).unwrap();
                    let length: usize = header
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        routes.push(header.lines().next().unwrap().to_owned());
                        break;
                    }
                }
            }
            let body = response.to_string();
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        routes
    });
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"publication"});
    let arguments = json!({"projectId":"project","taskId":"model","threadId":"root","expectedRevision":0,"batchId":"batch","entities":[],"assertions":[]});
    assert_eq!(
        client
            .call("publish_knowledge_batch", &arguments, &root)
            .unwrap()
            .0,
        422
    );
    assert!(
        client
            .exposed_tools()
            .iter()
            .any(|tool| tool["name"] == "publish_knowledge_batch")
    );
    assert_eq!(
        client
            .call("publish_knowledge_batch", &arguments, &root)
            .unwrap()
            .0,
        200
    );
    assert_eq!(
        client
            .exposed_tools()
            .iter()
            .map(|tool| tool["name"].clone())
            .collect::<Vec<_>>(),
        vec![json!("complete_task")]
    );
    assert_eq!(
        client
            .call("publish_knowledge_batch", &arguments, &root)
            .unwrap_err(),
        "Knowledge publication committed; call complete_task now. Other tools are closed."
    );
    let read =
        json!({"projectId":"project","taskId":"model","threadId":"root","expectedRevision":0});
    let mut context = read.clone();
    context["budget"] = json!(12);
    context["byteBudget"] = json!(6000);
    assert_eq!(
        client.call("read_context", &context, &root).unwrap_err(),
        "Knowledge publication committed; call complete_task now. Other tools are closed."
    );
    let mut completion = read;
    completion["summary"] = json!("已发布");
    let mut other = completion.clone();
    other["taskId"] = json!("other");
    assert_eq!(
        client.call("complete_task", &other, &root).unwrap_err(),
        "Publication completion belongs to the bound root task"
    );
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    assert_eq!(
        client
            .call("complete_task", &completion, &child)
            .unwrap_err(),
        "Subagents may read sources or save explicitly authorized isolated candidates; parent owns formal writes and decisions"
    );
    assert_eq!(
        client.call("complete_task", &completion, &root).unwrap().0,
        503
    );
    let completed = client.call("complete_task", &completion, &root).unwrap();
    assert_eq!(completed.1["status"], "completed");
    assert_eq!(completed.1["summary"], "已发布");
    assert_eq!(client.call("complete_task", &completion, &root).unwrap(), completed);
    assert_eq!(client.call("read_context", &context, &root).unwrap().1["code"], "task_already_completed");
    assert_eq!(
        server.join().unwrap(),
        vec![
            "POST /api/projects/project/modeling-batches/batch/publish HTTP/1.1",
            "POST /api/projects/project/modeling-batches/batch/publish HTTP/1.1",
            "POST /api/projects/project/tasks/model/complete HTTP/1.1",
            "POST /api/projects/project/tasks/model/complete HTTP/1.1",
        ]
    );
}

#[test]
fn incomplete_or_mismatched_receipts_cannot_close_the_session() {
    let arguments =
        json!({"projectId":"project","taskId":"model","threadId":"root","batchId":"batch"});
    for (status, receipt) in [
        (
            200,
            json!({"publicationState":"committed","nextAction":"complete_task","batchId":"other","receiptRef":"runtime:knowledge:other"}),
        ),
        (
            200,
            json!({"publicationState":"committed","nextAction":"complete_task","batchId":"batch"}),
        ),
        (
            503,
            json!({"publicationState":"committed","nextAction":"complete_task","batchId":"batch","receiptRef":"runtime:knowledge:batch"}),
        ),
        (200, Value::Null),
    ] {
        let gate = PublicationGate::default();
        gate.observe("publish_knowledge_batch", &arguments, &(status, receipt));
        assert!(gate.allows_tool("publish_knowledge_batch"));
    }
}
