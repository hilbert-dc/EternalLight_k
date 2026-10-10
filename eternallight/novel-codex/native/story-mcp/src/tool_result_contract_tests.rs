use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{RuntimeClient, handle_message, operation_digest};

fn receive(stream: &mut TcpStream) -> (String, Value) {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length = header.lines().find_map(|line| line.strip_prefix("Content-Length: "))
                .unwrap().parse::<usize>().unwrap();
            if bytes.len() >= end + 4 + length {
                let body = if length == 0 { Value::Null } else {
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                };
                return (header.to_owned(), body);
            }
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, payload: &Value) {
    let body = payload.to_string();
    write!(stream, "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

#[test]
fn actual_rpc_keeps_saved_completion_and_native_rejection_beside_exact_recovery_proof() {
    let checkpoint = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,
        "expectedCandidateRevisionId":null,"idempotencyKey":"save","payload":{"chapterId":"chapter","chapterNumber":1,
        "title":"信","body":"信到了。","completeness":"chapterComplete"}});
    let completion = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"summary":"完成当前职责"});
    let rejection = json!({"error":{"code":"invalid_request","message":"作者原要求尚未完整读取"}});
    for (tool, status, arguments, payload, ledger_state) in [
        ("save_draft_checkpoint", 201, checkpoint, json!({"chapterId":"chapter","revisionId":"draft-saved","checkpointId":"checkpoint-saved","version":1}), "committed"),
        ("complete_task", 200, completion.clone(), json!({"id":"task","projectId":"project","threadId":"root","status":"completed",
            "deliveryStatus":"ready_for_review","resultSummary":"已核对实际保存结果"}), "committed"),
        ("complete_task", 400, completion.clone(), rejection.clone(), "rejected"),
        ("complete_task", 503, completion, json!({"error":{"code":"unavailable","message":"结果尚未确认"}}), "accepted"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hash = operation_digest::operation_digest(tool, &arguments).unwrap();
        let server_hash = hash.clone();
        let server_payload = payload.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = receive(&mut stream);
            let path = if tool == "save_draft_checkpoint" { "/api/projects/project/drafts/checkpoints" } else { "/api/projects/project/tasks/task/complete" };
            assert!(headers.starts_with(&format!("POST {path} HTTP/1.1")));
            assert!(headers.contains(&format!("X-Novel-Request-Hash: {server_hash}\r\n")));
            assert!(headers.contains("X-Novel-Tool-Call: exact-call\r\n"));
            assert_eq!(body["threadId"], "root");
            respond(&mut stream, status, &server_payload);
            drop(stream);
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = receive(&mut stream);
            assert_eq!(body, Value::Null);
            assert!(headers.starts_with("GET /api/projects/project/tasks/recovery-operations?"));
            for exact in ["taskId=task", "threadId=root", "turnId=exact-turn", "callId=exact-call", &format!("requestHash={server_hash}")] {
                assert!(headers.contains(exact));
            }
            let receipt = if ledger_state == "committed" { json!("runtime:candidate:actual-receipt") } else { Value::Null };
            respond(&mut stream, 200, &json!({"operation":{"run_id":"task","thread_id":"root","turn_id":"exact-turn",
                "tool_call_id":"exact-call","tool_name":tool,"request_hash":server_hash,"operation_id":"operation-actual",
                "state":ledger_state,"receipt_ref":receipt}}));
        });
        let client = RuntimeClient {
            request_dispatched: Default::default(), completion_receipt: Default::default(),
            publication_gate: Default::default(), read_progress: Default::default(), argument_validation: Default::default(),
            host: "127.0.0.1".into(), port, candidate_token: "synthetic-candidate".into(),
            capability: "synthetic-capability".into(), operation: "generateChapter".into(),
        };
        let response = handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":tool,"arguments":arguments,"_meta":{"sessionId":"root","rootThreadId":"root","callerThreadId":"root",
            "parentThreadId":null,"turnId":"exact-turn","callId":"exact-call"}}}), &Ok(client)).unwrap();
        assert!(response.get("error").is_none());
        let result = &response["result"];
        assert_eq!(result["isError"], if status < 300 { Value::Null } else { json!(true) });
        let mut visible: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(result["structuredContent"], visible, "Core's preferred structured response must contain the same actual business result as text");
        let proof = visible.as_object_mut().unwrap().remove("recoveryOperation").unwrap();
        assert_eq!(proof, json!({"authority":"novelWorkspaceMcp","projectId":"project","runId":"task","rootThreadId":"root",
            "callerThreadId":"root","parentThreadId":null,"turnId":"exact-turn","callId":"exact-call","toolName":tool,
            "requestHash":hash,"phase":"runtimeLedger","operationId":"operation-actual",
            "outcome":if ledger_state == "accepted" {"unknown"} else {ledger_state},
            "receiptRef":match ledger_state {"committed" => json!("runtime:candidate:actual-receipt"),
                "rejected" => json!("runtime:operation:operation-actual:rejected"), _ => Value::Null}}));
        if tool == "save_draft_checkpoint" {
            assert_eq!(visible["saved"], true); assert_eq!(visible["revisionId"], "draft-saved");
            assert_eq!(visible["checkpointId"], "checkpoint-saved");
            assert_eq!(visible["nextAction"], "complete_task"); assert!(visible.get("body").is_none());
        } else if status == 200 {
            assert_eq!(visible["status"], "completed"); assert_eq!(visible["taskId"], "task");
            assert_eq!(visible["summary"], "已核对实际保存结果");
            assert_eq!(visible["authority"], "acknowledgedRuntimeCompletion");
        } else {
            assert_eq!(visible, payload, "Native rejected or uncertain completion must remain its real error");
            assert!(visible.get("saved").is_none()); assert!(visible.get("status").is_none());
        }
        server.join().unwrap();
    }
}

#[test]
fn non_object_json_keeps_its_original_value_without_losing_exact_proof() {
    let proof = json!({"outcome":"unknown","requestHash":"a".repeat(64),"callId":"exact"});
    for payload in [json!(["a", 2]), json!(42), Value::Null] {
        let result = super::tool_http_result(200, payload.clone(), Some(proof.clone()));
        let visible: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(visible, json!({"result":payload,"recoveryOperation":proof}));
        assert_eq!(result["structuredContent"], visible);
    }
}

#[test]
fn malformed_runtime_response_keeps_diagnosis_and_exact_ledger_state_without_claiming_success() {
    for state in ["accepted", "committed"] {
        let arguments = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"summary":"读取真实交付结果"});
        let hash = operation_digest::operation_digest("complete_task", &arguments).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_hash = hash.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = receive(&mut stream);
            assert!(headers.starts_with("POST /api/projects/project/tasks/task/complete HTTP/1.1"));
            assert_eq!(body["threadId"], "root");
            assert!(headers.contains(&format!("X-Novel-Request-Hash: {server_hash}\r\n")));
            stream.write_all(b"HTTP/1.1 200 Result\r\nContent-Length: 8\r\nConnection: close\r\n\r\nnot-json").unwrap();
            drop(stream);
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = receive(&mut stream);
            assert_eq!(body, Value::Null);
            assert!(headers.starts_with("GET /api/projects/project/tasks/recovery-operations?"));
            for exact in ["taskId=task", "threadId=root", "turnId=exact-turn", "callId=exact-call", "toolName=complete_task", &format!("requestHash={server_hash}")] {
                assert!(headers.contains(exact));
            }
            respond(&mut stream, 200, &json!({"operation":{"run_id":"task","thread_id":"root","turn_id":"exact-turn",
                "tool_call_id":"exact-call","tool_name":"complete_task","request_hash":server_hash,
                "operation_id":"operation-actual","state":state,
                "receipt_ref":if state == "committed" { json!("runtime:task:actual-completion") } else { Value::Null }}}));
        });
        let client = RuntimeClient {
            request_dispatched: Default::default(), completion_receipt: Default::default(),
            publication_gate: Default::default(), read_progress: Default::default(), argument_validation: Default::default(),
            host: "127.0.0.1".into(), port, candidate_token: "synthetic-candidate".into(),
            capability: "synthetic-capability".into(), operation: "generateChapter".into(),
        };
        let response = handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"complete_task","arguments":arguments,"_meta":{"sessionId":"root","rootThreadId":"root","callerThreadId":"root",
            "parentThreadId":null,"turnId":"exact-turn","callId":"exact-call"}}}), &Ok(client)).unwrap();
        let result = &response["result"];
        assert_eq!(result["isError"], true);
        let visible: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(result["structuredContent"], visible);
        assert_eq!(visible["error"]["message"], "Novel Runtime returned invalid JSON");
        assert!(visible.get("status").is_none()); assert!(visible.get("saved").is_none());
        assert_eq!(visible["recoveryOperation"], json!({"authority":"novelWorkspaceMcp","projectId":"project","runId":"task",
            "rootThreadId":"root","callerThreadId":"root","parentThreadId":null,"turnId":"exact-turn","callId":"exact-call",
            "toolName":"complete_task","requestHash":hash,"phase":"runtimeLedger","operationId":"operation-actual",
            "outcome":if state == "committed" {"committed"} else {"unknown"},
            "receiptRef":if state == "committed" { json!("runtime:task:actual-completion") } else { Value::Null }}));
        server.join().unwrap();
    }
}

#[test]
fn actual_schema_feedback_stays_intact_beside_before_dispatch_recovery_proof() {
    let client = RuntimeClient {
        request_dispatched: Default::default(), completion_receipt: Default::default(),
        publication_gate: Default::default(), read_progress: Default::default(), argument_validation: Default::default(),
        host: "127.0.0.1".into(), port: 1, candidate_token: "synthetic-candidate".into(),
        capability: "synthetic-capability".into(), operation: "generateChapter".into(),
    };
    let arguments = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"summary":42});
    let hash = operation_digest::operation_digest("complete_task", &arguments).unwrap();
    let response = handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"complete_task","arguments":arguments,"_meta":{"sessionId":"root","rootThreadId":"root","callerThreadId":"root",
        "parentThreadId":null,"turnId":"exact-turn","callId":"exact-call"}}}), &Ok(client)).unwrap();
    let result = &response["result"];
    assert_eq!(result["isError"], true);
    let visible: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["structuredContent"], visible);
    assert_eq!(visible["code"], "invalid_tool_arguments");
    assert_eq!(visible["tool"], "complete_task");
    assert_eq!(visible["attempt"], 1);
    assert!(visible.get("error").is_none(), "original schema feedback must retain its own structure");
    assert_eq!(visible["recoveryOperation"], json!({"authority":"novelWorkspaceMcp","projectId":"project","runId":"task",
        "rootThreadId":"root","callerThreadId":"root","parentThreadId":null,"turnId":"exact-turn","callId":"exact-call",
        "toolName":"complete_task","requestHash":hash,"phase":"beforeDispatch","operationId":null,
        "outcome":"rejected","receiptRef":format!("mcp:preflight:exact-call:{hash}")}));
}

#[test]
fn completed_task_with_forgotten_revision_returns_terminal_fact_without_dispatch() {
    let client = RuntimeClient {
        request_dispatched: Default::default(), completion_receipt: Default::default(),
        publication_gate: Default::default(), read_progress: Default::default(), argument_validation: Default::default(),
        host: "127.0.0.1".into(), port: 1, candidate_token: "synthetic-candidate".into(),
        capability: "synthetic-capability".into(), operation: "advisory".into(),
    };
    let args = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":2});
    let mut receipt = (200,json!({"projectId":"project","id":"task","threadId":"root","status":"completed","resultSummary":"已登记独立检查"}));
    client.completion_receipt.observe(&args,&mut receipt).unwrap();
    let mut stale = args.clone(); stale["expectedRevision"] = json!(0);
    let response = handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"read_work_status","arguments":stale,"_meta":{"sessionId":"root","rootThreadId":"root","callerThreadId":"root","parentThreadId":null,"turnId":"turn","callId":"call"}
    }}), &Ok(client)).unwrap();
    let body: Value = serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["code"],"task_already_completed");
    assert_eq!(body["requestedOperationExecuted"],false);
    assert_eq!(body["completion"]["summary"],"已登记独立检查");
}
