use super::query_plan_schema_tests::{call, client};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

fn plan() -> Value {
    json!({"question":"信中藏着什么？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"source","operation":{"type":"text","query":"信","limit":1}}]})
}

#[test]
fn real_mcp_semantic_errors_are_safe_and_prevent_runtime_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let client = client(&listener);
    let mut cases = Vec::new();
    let mut missing_ranking = plan();
    missing_ranking.as_object_mut().unwrap().remove("ranking");
    cases.push((missing_ranking, "plan.ranking", "required field"));
    let mut missing_limit = plan();
    missing_limit["steps"][0]["operation"]
        .as_object_mut()
        .unwrap()
        .remove("limit");
    cases.push((
        missing_limit,
        "plan.steps[0].operation.limit",
        "required field",
    ));
    let mut limit = plan();
    limit["steps"][0]["operation"]["limit"] = json!(21);
    cases.push((limit, "plan.steps[0].operation.limit", "maximum 20"));
    let mut unknown = plan();
    unknown["steps"][0]["operation"]["secret-body-key"] = json!("private fixture text");
    cases.push((
        unknown,
        "plan.steps[0].operation",
        "only the declared fields; remove unknown fields",
    ));
    let mut bad_type = plan();
    bad_type["steps"][0]["operation"]["limit"] = json!("private fixture text");
    cases.push((
        bad_type,
        "plan.steps[0].operation.limit",
        "type \"integer\"",
    ));
    let mut rank = plan();
    rank["ranking"] = json!({"type":"modelSemantic","baseReportHash":"a".repeat(/*n*/ 64),
        "judgments":[{"resultId":"b".repeat(/*n*/ 64),"score":1001,"rationale":"证据"}]});
    cases.push((rank, "plan.ranking.judgments[0].score", "maximum 1000"));
    for (plan, path, expected) in cases {
        for tool in ["query_story", "read_context"] {
            let path = if tool == "read_context" {
                path.replacen("plan", "queryPlan", /*count*/ 1)
            } else {
                path.to_owned()
            };
            let response = call(&client, tool, plan.clone());
            let diagnostic = super::query_plan_schema_tests::parameter_error(&response, tool);
            assert_eq!(
                diagnostic["detail"],
                json!({"code":"invalid_query_plan","path":path,"expected":expected})
            );
            assert!(!response.to_string().contains("private fixture"));
            assert!(!response.to_string().contains("secret-body-key"));
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
}

#[test]
fn operation_specific_writer_author_perspective_remains_forbidden() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let mut client = client(&listener);
    client.operation = "generateChapter".into();
    let mut plan = plan();
    plan["steps"][0]["operation"] =
        json!({"type":"state","entityId":"entity","perspective":"author"});
    for tool in ["query_story", "read_context"] {
        let path = if tool == "query_story" {
            "plan"
        } else {
            "queryPlan"
        };
        let diagnostic = super::query_plan_schema_tests::parameter_error(
            &call(&client, tool, plan.clone()),
            tool,
        );
        assert_eq!(
            diagnostic["detail"],
            json!({"code":"invalid_query_plan","path":format!("{path}.steps[0].operation.perspective"),
            "expected":"allowed value [\"world\",\"character\",\"reader\"]"})
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn frozen_writer_baseline_plan_is_operation_scoped_and_forwards_without_source_overrides() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let mut client = client(&listener);
    let baseline = json!({"question":"读取冻结本章原稿","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"writer-source","operation":{"type":"writerBaselinePassages","cursor":null,"maxChars":400,"limit":2}}]});
    for operation in ["advisory", "assessSceneOutcome"] {
        client.operation = operation.into();
        for tool in ["query_story", "read_context"] {
            assert_eq!(
                call(&client, tool, baseline.clone())["result"]["isError"],
                json!(true)
            );
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
    client.operation = "generateChapter".into();
    for (field, value) in [
        ("sourceRevisionId", json!("private-source")),
        ("chapterId", json!("other-chapter")),
        ("cursor", json!("中".repeat(/*n*/ 257))),
        ("limit", json!(5)),
    ] {
        let mut invalid = baseline.clone();
        invalid["steps"][0]["operation"][field] = value;
        for tool in ["query_story", "read_context"] {
            let result = call(&client, tool, invalid.clone());
            assert_eq!(result["result"]["isError"], json!(true));
            assert!(!result.to_string().contains("private-source"));
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
    listener.set_nonblocking(/*nonblocking*/ false).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(
                /*dur*/ Some(std::time::Duration::from_secs(/*secs*/ 5)),
            )
            .unwrap();
        let mut request = Vec::new();
        let (end, length) = loop {
            let mut buffer = [0; 4096];
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
                break (end + 4, length);
            }
        };
        while request.len() < end + length {
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        let body: Value = serde_json::from_slice(&request[end..end + length]).unwrap();
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"readOnly\":true}").unwrap();
        body
    });
    let mut next_page = baseline;
    next_page["steps"][0]["operation"]["cursor"] = json!("runtime-frozen-cursor");
    assert_eq!(
        call(&client, "query_story", next_page.clone()),
        json!({"jsonrpc":"2.0","id":1,
        "result":{"content":[{"type":"text","text":"{\"readOnly\":true}"}]}})
    );
    assert_eq!(
        server.join().unwrap(),
        json!({"threadId":"root","expectedRevision":0,"plan":next_page})
    );
}

#[test]
fn transport_valid_bad_plan_can_be_corrected_through_actual_mcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let client = client(&listener);
    let tool = client
        .exposed_tools()
        .into_iter()
        .find(|tool| tool["name"] == "query_story")
        .unwrap();
    let validator = jsonschema::validator_for(&tool["inputSchema"]).unwrap();
    let mut bad = plan();
    bad["steps"][0]["operation"]
        .as_object_mut()
        .unwrap()
        .remove("limit");
    assert!(validator.is_valid(&json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"plan":bad})));
    assert_eq!(
        call(&client, "query_story", bad)["result"]["isError"],
        json!(true)
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(/*nonblocking*/ false).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(
                /*dur*/ Some(std::time::Duration::from_secs(/*secs*/ 5)),
            )
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        let (end, length) = loop {
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
                break (end + 4, length);
            }
        };
        while request.len() < end + length {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        let body: Value = serde_json::from_slice(&request[end..end + length]).unwrap();
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"readOnly\":true}").unwrap();
        body
    });
    assert_eq!(
        call(&client, "query_story", plan()),
        json!({"jsonrpc":"2.0","id":1,"result":{
        "content":[{"type":"text","text":"{\"readOnly\":true}"}]}})
    );
    assert_eq!(
        server.join().unwrap(),
        json!({"threadId":"root","expectedRevision":0,"plan":plan()})
    );
    assert!(!validator.is_valid(&json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,"plan":plan(),"unrecognized":true})));
}
