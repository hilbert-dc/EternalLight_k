use std::env;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;

use serde_json::Value;
use serde_json::json;

const PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

struct RuntimeClient {
    host: String,
    port: u16,
    candidate_token: String,
    capability: String,
    operation: String,
}

impl RuntimeClient {
    fn from_environment() -> Result<Self, String> {
        let origin = env::var("NOVEL_CODEX_RUNTIME_URL")
            .map_err(|_| "NOVEL_CODEX_RUNTIME_URL is not configured".to_owned())?;
        let authority = origin
            .strip_prefix("http://")
            .ok_or_else(|| "Runtime URL must use loopback HTTP".to_owned())?;
        if authority.contains('/') || authority.contains('@') {
            return Err("Runtime URL must be a loopback HTTP origin".into());
        }
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| "Runtime URL must include a port".to_owned())?;
        if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
            return Err("Runtime URL must use a loopback host".into());
        }
        Ok(Self {
            host: host.trim_matches(['[', ']']).to_owned(),
            port: port.parse().map_err(|_| "Runtime port is invalid")?,
            candidate_token: required_environment("NOVEL_CODEX_CANDIDATE_TOKEN")?,
            capability: required_environment("NOVEL_CODEX_TASK_CAPABILITY")?,
            operation: required_environment("NOVEL_CODEX_OPERATION")?,
        })
    }

    fn exposed_tools(&self) -> Vec<Value> {
        tool_specs()
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| allowed_tools(&self.operation).contains(&name))
            })
            .collect()
    }

    fn call(&self, name: &str, arguments: &Value, meta: &Value) -> Result<(u16, Value), String> {
        if !allowed_tools(&self.operation).contains(&name) {
            return Err("tool is not allowed for this generation operation".into());
        }
        let arguments = arguments
            .as_object()
            .ok_or_else(|| "arguments must be an object".to_owned())?;
        let tool = tool_specs()
            .into_iter()
            .find(|tool| tool["name"] == name)
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        let schema = &tool["inputSchema"];
        let properties = schema["properties"]
            .as_object()
            .ok_or_else(|| "tool schema is invalid".to_owned())?;
        if let Some(field) = arguments
            .keys()
            .find(|field| !properties.contains_key(*field))
        {
            return Err(format!("unexpected argument: {field}"));
        }
        for field in schema["required"].as_array().into_iter().flatten() {
            let field = field.as_str().unwrap_or_default();
            if !arguments.contains_key(field) {
                return Err(format!("missing required argument: {field}"));
            }
        }
        let project_id = required_string(arguments.get("projectId"), "projectId")?;
        let task_id = required_string(arguments.get("taskId"), "taskId")?;
        let thread_id = required_string(arguments.get("threadId"), "threadId")?;
        let expected_revision = arguments
            .get("expectedRevision")
            .and_then(Value::as_i64)
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| "expectedRevision must be a non-negative integer".to_owned())?;
        validate_execution_identity(meta, thread_id, name)?;

        let (method, path, body) = match name {
            "read_context" => {
                let purpose = arguments
                    .get("purpose")
                    .and_then(Value::as_str)
                    .unwrap_or("planner");
                let mut query = vec![
                    ("taskId", task_id.to_owned()),
                    ("threadId", thread_id.to_owned()),
                    ("projectRevision", expected_revision.to_string()),
                    ("purpose", purpose.to_owned()),
                    (
                        "budget",
                        arguments
                            .get("budget")
                            .and_then(Value::as_u64)
                            .unwrap_or(24)
                            .to_string(),
                    ),
                    (
                        "byteBudget",
                        arguments
                            .get("byteBudget")
                            .and_then(Value::as_u64)
                            .unwrap_or(24_000)
                            .to_string(),
                    ),
                ];
                for key in [
                    "storyTime",
                    "readingChapter",
                    "sceneOrder",
                    "povCharacterId",
                ] {
                    if let Some(value) = arguments.get(key) {
                        query.push((
                            key,
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string()),
                        ));
                    }
                }
                let query = query
                    .into_iter()
                    .map(|(key, value)| format!("{key}={}", percent_encode(&value)))
                    .collect::<Vec<_>>()
                    .join("&");
                (
                    "GET",
                    format!(
                        "/api/projects/{}/context?{query}",
                        percent_encode(project_id)
                    ),
                    None,
                )
            }
            "save_brief" | "save_outline" | "save_chapter_plans" => {
                let kind = match name {
                    "save_brief" => "brief",
                    "save_outline" => "outline",
                    "save_chapter_plans" => "plan",
                    _ => unreachable!(),
                };
                (
                    "POST",
                    format!(
                        "/api/projects/{}/artifacts/{kind}",
                        percent_encode(project_id)
                    ),
                    Some(without(arguments, &["projectId"])),
                )
            }
            "save_draft_checkpoint" => (
                "POST",
                format!(
                    "/api/projects/{}/drafts/checkpoints",
                    percent_encode(project_id)
                ),
                Some(without(arguments, &["projectId"])),
            ),
            "submit_fact_proposals" => (
                "POST",
                format!("/api/projects/{}/reviews", percent_encode(project_id)),
                Some(json!({
                    "taskId": task_id, "threadId": thread_id, "expectedRevision": expected_revision,
                    "artifactKind": "draft", "revisionId": arguments.get("revisionId"),
                    "proposals": arguments.get("proposals"), "selections": {},
                    "idempotencyKey": arguments.get("idempotencyKey")
                })),
            ),
            "complete_task" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/complete",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(json!({
                    "threadId": thread_id, "expectedRevision": expected_revision,
                    "summary": arguments.get("summary").cloned().unwrap_or_else(|| json!(""))
                })),
            ),
            "submit_planning_review_report" => {
                let run_id = required_string(arguments.get("runId"), "runId")?;
                (
                    "POST",
                    format!(
                        "/api/projects/{}/planning-reviews/{}/reports",
                        percent_encode(project_id),
                        percent_encode(run_id)
                    ),
                    Some(without(arguments, &["projectId", "runId"])),
                )
            }
            _ => return Err(format!("unknown tool: {name}")),
        };
        self.request(method, &path, body.as_ref(), meta, name)
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        meta: &Value,
        tool_name: &str,
    ) -> Result<(u16, Value), String> {
        let body = body.map(Value::to_string).unwrap_or_default();
        let mut headers = vec![
            ("Authorization", format!("Bearer {}", self.candidate_token)),
            ("X-Novel-Task-Capability", self.capability.clone()),
            ("Content-Type", "application/json".into()),
            ("Accept", "application/json".into()),
            ("Connection", "close".into()),
            ("X-Novel-Tool-Name", tool_name.to_owned()),
        ];
        for (header, field) in [
            ("X-Novel-Root-Thread", "rootThreadId"),
            ("X-Novel-Caller-Thread", "callerThreadId"),
            ("X-Novel-Parent-Thread", "parentThreadId"),
            ("X-Novel-Turn", "turnId"),
            ("X-Novel-Tool-Call", "callId"),
        ] {
            if let Some(value) = meta
                .get(field)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                headers.push((header, value.to_owned()));
            }
        }
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|error| format!("Novel Runtime is unavailable: {error}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|error| error.to_string())?;
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\n",
            self.host, self.port
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
        stream
            .write_all(request.as_bytes())
            .map_err(|error| error.to_string())?;
        let mut response = Vec::new();
        stream
            .take((MAX_RESPONSE_BYTES + 64 * 1024) as u64)
            .read_to_end(&mut response)
            .map_err(|error| error.to_string())?;
        parse_http_response(&response)
    }
}

fn main() {
    let client = RuntimeClient::from_environment();
    for line in BufReader::new(std::io::stdin().lock()).lines() {
        let response = match line {
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(message) => handle_message(&message, &client),
                Err(_) => Some(rpc_error(Value::Null, -32700, "Parse error")),
            },
            Err(_) => break,
        };
        if let Some(response) = response {
            println!("{response}");
        }
    }
}

fn handle_message(message: &Value, client: &Result<RuntimeClient, String>) -> Option<Value> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    if message.get("jsonrpc") != Some(&json!("2.0")) {
        return Some(rpc_error(id, -32600, "Invalid Request"));
    }
    message.get("id")?;
    match message.get("method").and_then(Value::as_str) {
        Some("initialize") => Some(rpc_result(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "novel-codex-workspace", "version": "0.1.0"},
                "instructions": "Candidate-only tools bound to a task, project, thread, revision, and capability."
            }),
        )),
        Some("ping") => Some(rpc_result(id, json!({}))),
        Some("tools/list") => match client.as_ref() {
            Ok(client) => Some(rpc_result(id, json!({"tools": client.exposed_tools()}))),
            Err(error) => Some(rpc_error(id, -32603, error)),
        },
        Some("tools/call") => {
            let Some(params) = message.get("params").and_then(Value::as_object) else {
                return Some(rpc_error(id, -32602, "Invalid params"));
            };
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "Invalid params"));
            };
            let arguments = params.get("arguments").unwrap_or(&Value::Null);
            let meta = params.get("_meta").unwrap_or(&Value::Null);
            let result = client
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|client| client.call(name, arguments, meta));
            match result {
                Ok((status, payload)) => {
                    let mut result = json!({
                        "content": [{"type": "text", "text": payload.to_string()}]
                    });
                    if !(200..300).contains(&status) {
                        result["isError"] = json!(true);
                    }
                    Some(rpc_result(id, result))
                }
                Err(error) => Some(rpc_result(
                    id,
                    json!({
                        "content": [{"type": "text", "text": error}], "isError": true
                    }),
                )),
            }
        }
        _ => Some(rpc_error(id, -32601, "Method not found")),
    }
}

fn allowed_tools(operation: &str) -> &'static [&'static str] {
    match operation {
        "advisory" => &["read_context"],
        "reviewPlanningSeat" => &[
            "read_context",
            "submit_planning_review_report",
            "complete_task",
        ],
        "generateBrief" => &["read_context", "save_brief", "complete_task"],
        "generateOutline" => &["read_context", "save_outline", "complete_task"],
        "generatePlan" => &["read_context", "save_chapter_plans", "complete_task"],
        "generateChapter" => &[
            "read_context",
            "save_draft_checkpoint",
            "submit_fact_proposals",
            "complete_task",
        ],
        _ => &[],
    }
}

fn tool_specs() -> Vec<Value> {
    [
        ("read_context", "Read a bounded project context snapshot."),
        ("save_brief", "Save a reviewable Brief candidate."),
        ("save_outline", "Save a reviewable Outline candidate."),
        ("save_chapter_plans", "Save reviewable ChapterPlan candidates."),
        ("save_draft_checkpoint", "Save an immutable DraftRevision checkpoint."),
        ("submit_fact_proposals", "Submit candidate story facts for author review."),
        ("submit_planning_review_report", "Submit an evidence-based planning review report."),
        ("complete_task", "Complete the bound task after persisted output validation."),
    ]
    .into_iter()
    .map(|(name, description)| {
        let operation_required: &[&str] = match name {
            "read_context" => &["purpose", "budget", "byteBudget"],
            "save_brief" | "save_outline" | "save_chapter_plans" | "save_draft_checkpoint" => {
                &["payload", "expectedCandidateRevisionId", "idempotencyKey"]
            }
            "submit_fact_proposals" => &["revisionId", "proposals", "idempotencyKey"],
            "submit_planning_review_report" => &["runId", "report"],
            "complete_task" => &["summary"],
            _ => &[],
        };
        let required = ["taskId", "projectId", "threadId", "expectedRevision"]
            .into_iter()
            .chain(operation_required.iter().copied())
            .collect::<Vec<_>>();
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "taskId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "projectId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "threadId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "expectedRevision": {"type": "integer", "minimum": 0},
                    "purpose": {"type": "string", "enum": ["planner", "writer", "critic"]},
                    "budget": {"type": "integer", "minimum": 1, "maximum": 100000},
                    "byteBudget": {"type": "integer", "minimum": 4000, "maximum": 60000},
                    "storyTime": {"type": "integer"}, "readingChapter": {"type": "integer", "minimum": 0},
                    "sceneOrder": {"type": "integer", "minimum": 0}, "povCharacterId": {"type": "string"},
                    "payload": {"type": "object"},
                    "expectedCandidateRevisionId": {"type": ["string", "null"]},
                    "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
                    "revisionId": {"type": "string"}, "proposals": {"type": "array", "maxItems": 200},
                    "summary": {"type": "string", "maxLength": 4000},
                    "runId": {"type": "string"}, "report": {"type": "object"}
                },
                "required": required,
                "additionalProperties": false
            }
        })
    })
    .collect()
}

fn validate_execution_identity(
    meta: &Value,
    thread_id: &str,
    tool_name: &str,
) -> Result<(), String> {
    let root = meta
        .get("rootThreadId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let caller = meta
        .get("callerThreadId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session = meta
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parent = meta
        .get("parentThreadId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if (caller != root || !parent.is_empty()) && tool_name != "read_context" {
        return Err("Subagents are restricted to read_context".into());
    }
    if root.is_empty() || caller.is_empty() || session != root || thread_id != root {
        return Err("trusted execution identity does not match threadId".into());
    }
    Ok(())
}

fn required_environment(name: &str) -> Result<String, String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is not configured"))
}

fn required_string<'a>(value: Option<&'a Value>, name: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} must be a non-empty string"))
}

fn without(source: &serde_json::Map<String, Value>, excluded: &[&str]) -> Value {
    Value::Object(
        source
            .iter()
            .filter(|(key, _)| !excluded.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn parse_http_response(response: &[u8]) -> Result<(u16, Value), String> {
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "Novel Runtime returned malformed HTTP".to_owned())?;
    let headers = std::str::from_utf8(&response[..separator]).map_err(|error| error.to_string())?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| "Novel Runtime returned an invalid status".to_owned())?;
    let raw_body = &response[separator + 4..];
    let decoded;
    let body = if headers.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    }) {
        decoded = decode_chunked(raw_body)?;
        decoded.as_slice()
    } else {
        raw_body
    };
    if body.len() > MAX_RESPONSE_BYTES {
        return Err("Novel Runtime response exceeded the size limit".into());
    }
    let payload = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(body)
            .map_err(|_| "Novel Runtime returned invalid JSON".to_owned())?
    };
    Ok((status, payload))
}

fn decode_chunked(mut input: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    loop {
        let line_end = input
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| "Novel Runtime returned malformed chunked HTTP".to_owned())?;
        let size = std::str::from_utf8(&input[..line_end])
            .ok()
            .and_then(|value| value.split(';').next())
            .and_then(|value| usize::from_str_radix(value, 16).ok())
            .ok_or_else(|| "Novel Runtime returned an invalid HTTP chunk".to_owned())?;
        input = &input[line_end + 2..];
        if size == 0 {
            break;
        }
        if input.len() < size + 2 || &input[size..size + 2] != b"\r\n" {
            return Err("Novel Runtime returned a truncated HTTP chunk".into());
        }
        output.extend_from_slice(&input[..size]);
        if output.len() > MAX_RESPONSE_BYTES {
            return Err("Novel Runtime response exceeded the size limit".into());
        }
        input = &input[size + 2..];
    }
    Ok(output)
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
