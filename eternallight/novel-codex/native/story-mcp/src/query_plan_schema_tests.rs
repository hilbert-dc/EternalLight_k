use super::{RuntimeClient, handle_message};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

fn boundary(maximum: usize) -> String {
    format!("{}{}", "中🌙".repeat(maximum / 7), "x".repeat(maximum % 7))
}

fn plans() -> Vec<(&'static str, usize, Value)> {
    let base = json!({"question":"线索","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"source","operation":{"type":"text","query":"信","limit":1}}]});
    let mut cases = Vec::new();
    for (pointer, maximum, operation) in [
        (
            "/question",
            1000,
            json!({"type":"text","query":"信","limit":1}),
        ),
        (
            "/steps/0/id",
            32,
            json!({"type":"text","query":"信","limit":1}),
        ),
        (
            "/steps/0/operation/cursor",
            768,
            json!({"type":"passages","cursor":null,"maxChars":160,"limit":2}),
        ),
        (
            "/steps/0/operation/entityId",
            160,
            json!({"type":"state","entityId":"entity","perspective":"author"}),
        ),
        (
            "/steps/0/operation/entityId",
            160,
            json!({"type":"relations","entityId":"entity","direction":"both","maxHops":1,"limit":1}),
        ),
        (
            "/ranking/judgments/0/rationale",
            256,
            json!({"type":"text","query":"信","limit":1}),
        ),
    ] {
        let mut plan = base.clone();
        plan["steps"][0]["operation"] = operation;
        if pointer.starts_with("/ranking") {
            plan["ranking"] = json!({"type":"modelSemantic","baseReportHash":"a".repeat(64),
                "judgments":[{"resultId":"b".repeat(64),"score":500,"rationale":"依据"}]});
        }
        *plan.pointer_mut(pointer).unwrap() = json!(boundary(maximum));
        cases.push((pointer, maximum, plan));
    }
    cases
}

pub(super) fn client(listener: &TcpListener) -> RuntimeClient {
    RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "fixture-candidate".into(),
        capability: "fixture-capability".into(),
        operation: "advisory".into(),
    }
}

pub(super) fn call(client: &RuntimeClient, tool: &str, plan: Value) -> Value {
    let mut arguments =
        json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0});
    if tool == "read_context" {
        arguments["budget"] = json!(24);
        arguments["byteBudget"] = json!(60000);
        arguments["queryPlan"] = plan;
    } else {
        arguments["plan"] = plan;
    }
    handle_message(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":tool,"arguments":arguments,
        "_meta":{"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"query"}}}),
        &Ok(RuntimeClient { publication_gate: super::publication_gate::PublicationGate::default(),
            read_progress: Default::default(),
            argument_validation: Default::default(),
            host: client.host.clone(), port: client.port, candidate_token: client.candidate_token.clone(),
            capability: client.capability.clone(), operation: client.operation.clone() })).unwrap()
}

// Each schema case uses a fresh executor; task-local retry limits are tested
// separately against one persistent client.
pub(super) fn parameter_error(response: &Value, tool: &str) -> Value {
    assert_eq!(response["result"]["isError"], true);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let diagnostic: Value = serde_json::from_str(text).unwrap();
    assert_eq!(response["result"]["structuredContent"], diagnostic);
    assert_eq!(diagnostic["code"], "invalid_tool_arguments");
    assert_eq!(diagnostic["tool"], tool);
    assert_eq!(diagnostic["attempt"], 1);
    assert_eq!(diagnostic["limit"], 3);
    assert!(text.len() <= 1600);
    diagnostic
}

#[test]
fn query_utf8_overflow_is_safe_and_never_dispatches_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(/*nonblocking*/ true).unwrap();
    let client = client(&listener);
    for (pointer, maximum, mut plan) in plans() {
        *plan.pointer_mut(pointer).unwrap() = json!(format!("{}x", boundary(maximum)));
        let path = match pointer {
            "/question" => "plan.question",
            "/steps/0/id" => "plan.steps[0].id",
            "/steps/0/operation/cursor" => "plan.steps[0].operation.cursor",
            "/steps/0/operation/entityId" => "plan.steps[0].operation.entityId",
            "/ranking/judgments/0/rationale" => "plan.ranking.judgments[0].rationale",
            _ => unreachable!(),
        };
        for tool in ["query_story", "read_context"] {
            let path = if tool == "read_context" {
                path.replacen("plan.", "queryPlan.", 1)
            } else {
                path.to_owned()
            };
            let detail = json!({"code":"invalid_query_plan","path":path,"expected":format!("at most {maximum} UTF-8 bytes")});
            let diagnostic = parameter_error(&call(&client, tool, plan.clone()), tool);
            assert_eq!(diagnostic["detail"], detail);
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
}

#[test]
fn query_utf8_boundaries_and_unicode_character_fields_forward_unchanged() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = client(&listener);
    let mut plans = plans()
        .into_iter()
        .map(|(_, _, plan)| plan)
        .collect::<Vec<_>>();
    for operation in [
        json!({"type":"text","query":"中".repeat(240),"limit":1}),
        json!({"type":"resolveEntity","name":"中".repeat(160)}),
    ] {
        plans.push(
            json!({"question":"查原文","byteBudget":9000,"ranking":{"type":"literal"},
            "steps":[{"id":"source","operation":operation}]}),
        );
    }
    let count = plans.len() * 2;
    let server = std::thread::spawn(move || {
        let mut actual = Vec::new();
        for _ in 0..count {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(
                    /*dur*/ Some(std::time::Duration::from_secs(/*secs*/ 5)),
                )
                .unwrap();
            let mut request = Vec::new();
            let (end, length) = loop {
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
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
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
            }
            actual.push((
                std::str::from_utf8(&request[..end])
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
                std::str::from_utf8(&request[end..end + length])
                    .unwrap()
                    .to_owned(),
            ));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"readOnly\":true}").unwrap();
        }
        actual
    });
    for plan in &plans {
        for tool in ["query_story", "read_context"] {
            assert_eq!(
                call(&client, tool, plan.clone()),
                json!({"jsonrpc":"2.0","id":1,"result":{
            "content":[{"type":"text","text":"{\"readOnly\":true}"}]}})
            );
        }
    }
    assert_eq!(
        server.join().unwrap(),
        plans
            .into_iter()
            .flat_map(|plan| [
                ("POST /api/projects/project/tasks/task/query HTTP/1.1".to_owned(), json!({"threadId":"root","expectedRevision":0,"plan":plan}).to_string()),
                (format!("GET /api/projects/project/context?taskId=task&threadId=root&projectRevision=0&purpose=planner&budget=24&byteBudget=60000&queryPlan={} HTTP/1.1", super::percent_encode(&plan.to_string())), String::new()),
            ])
            .collect::<Vec<_>>()
    );
}
