use std::env;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;

use serde_json::Value;
use serde_json::json;
mod knowledge_schema;
mod operation_digest;

mod query_plan_schema;
mod reading_checkpoint_tools;
use query_plan_schema::query_plan_schema;

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
                    .is_some_and(|name| allowed_tools(&self.operation).contains(&name)
                        || (self.operation != "assessRevisionOutcome" && reading_checkpoint_tools::allowed(&self.operation,name)))
            })
            .map(|mut tool| {
                if self.operation == "generateChapter" {
                    for field in ["plan", "queryPlan"] {
                        if let Some(operations) = tool.pointer_mut(&format!("/inputSchema/properties/{field}/properties/steps/items/properties/operation/oneOf")).and_then(Value::as_array_mut) {
                            for operation in operations {
                                if operation["properties"]["type"]["const"] == "state" {
                                    operation["properties"]["perspective"]["enum"] = json!(["world", "character", "reader"]);
                                }
                            }
                        }
                    }
                }
                tool
            })
            .collect()
    }

    fn call(&self, name: &str, arguments: &Value, meta: &Value) -> Result<(u16, Value), String> {
        if !allowed_tools(&self.operation).contains(&name)
            && (self.operation == "assessRevisionOutcome"
                || !reading_checkpoint_tools::allowed(&self.operation, name))
        {
            return Err("tool is not allowed for this generation operation".into());
        }
        let normalized_arguments = operation_digest::host_json(arguments)?;
        let arguments = normalized_arguments
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
            "read_property_rules" => {
                let mut path = format!(
                    "/api/projects/{}/tasks/{}/property-rule-input?threadId={}&expectedRevision={expected_revision}",
                    percent_encode(project_id),
                    percent_encode(task_id),
                    percent_encode(thread_id)
                );
                if let Some(cursor) = arguments.get("cursor").filter(|value| !value.is_null()) {
                    let cursor = required_string(Some(cursor), "cursor")?;
                    if cursor.len() > 100 {
                        return Err("Rule cursor exceeds its bound".into());
                    }
                    path.push_str(&format!("&cursor={}", percent_encode(cursor)));
                }
                ("GET", path, None)
            }
            "read_commitment" | "submit_commitment_candidate" => {
                let id = required_string(arguments.get("commitmentId"), "commitmentId")?;
                if id.len() > 120 {
                    return Err("commitmentId exceeds 120 bytes".into());
                }
                let path = format!(
                    "/api/projects/{}/commitments/{}",
                    percent_encode(project_id),
                    percent_encode(id)
                );
                if name == "read_commitment" {
                    (
                        "GET",
                        format!(
                            "{path}/task-input?taskId={}&threadId={}&expectedRevision={expected_revision}",
                            percent_encode(task_id),
                            percent_encode(thread_id)
                        ),
                        None,
                    )
                } else {
                    (
                        "POST",
                        format!("{path}/task-candidate"),
                        Some(without(arguments, &["projectId", "commitmentId"])),
                    )
                }
            }
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
                    "queryPlan",
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
            "read_reading_progress" => (
                "GET",
                format!(
                    "/api/projects/{}/tasks/{}/reading-progress?threadId={}&expectedRevision={expected_revision}",
                    percent_encode(project_id),
                    percent_encode(task_id),
                    percent_encode(thread_id)
                ),
                None,
            ),
            "checkpoint_reading" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/reading-progress",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(
                    json!({"threadId":thread_id,"expectedRevision":expected_revision,"plan":arguments.get("plan").ok_or("plan is required")?,
                    "reportHash":arguments.get("reportHash").ok_or("reportHash is required")?,"expectedVersion":arguments.get("expectedVersion").ok_or("expectedVersion is required")?,
                    "summary":arguments.get("summary").ok_or("summary is required")?}),
                ),
            ),
            "query_story" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/query",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(
                    json!({"threadId":thread_id,"expectedRevision":expected_revision,"plan":arguments.get("plan").ok_or("plan is required")?}),
                ),
            ),
            "search_story" => {
                let query = required_string(arguments.get("query"), "query")?;
                if query.chars().count() > 240 {
                    return Err("query exceeds 240 characters".into());
                }
                let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(10);
                if !(1..=20).contains(&limit) {
                    return Err("limit must be between 1 and 20".into());
                }
                let mut query_parts = vec![
                    ("threadId", thread_id.to_owned()),
                    ("expectedRevision", expected_revision.to_string()),
                    ("q", query.to_owned()),
                    ("limit", limit.to_string()),
                ];
                for key in ["kind", "cursor", "entityName", "entityId"] {
                    if let Some(value) = arguments.get(key) {
                        query_parts.push((key, required_string(Some(value), key)?.to_owned()));
                    }
                }
                let query_string = query_parts
                    .into_iter()
                    .map(|(key, value)| format!("{key}={}", percent_encode(&value)))
                    .collect::<Vec<_>>()
                    .join("&");
                (
                    "GET",
                    format!(
                        "/api/projects/{}/tasks/{}/search?{query_string}",
                        percent_encode(project_id),
                        percent_encode(task_id)
                    ),
                    None,
                )
            }
            "search_relations" => {
                let entity_id = required_string(arguments.get("entityId"), "entityId")?;
                if entity_id.len() > 160 {
                    return Err("entityId exceeds 160 bytes".into());
                }
                let limit = match arguments.get("limit") {
                    Some(value) => value.as_u64().ok_or("limit must be an integer")?,
                    None => 20,
                };
                let max_hops = match arguments.get("maxHops") {
                    Some(value) => value.as_u64().ok_or("maxHops must be an integer")?,
                    None => 1,
                };
                if !(1..=20).contains(&limit) || !(1..=3).contains(&max_hops) {
                    return Err("relation search budget is invalid".into());
                }
                let mut query_parts = vec![
                    ("threadId", thread_id.to_owned()),
                    ("expectedRevision", expected_revision.to_string()),
                    ("entityId", entity_id.to_owned()),
                    ("limit", limit.to_string()),
                    ("maxHops", max_hops.to_string()),
                ];
                for key in ["relationType", "direction"] {
                    if let Some(value) = arguments.get(key) {
                        query_parts.push((key, required_string(Some(value), key)?.to_owned()));
                    }
                }
                if let Some(value) = arguments.get("worldTime") {
                    let time = value.as_i64().ok_or("worldTime must be an integer")?;
                    query_parts.push(("worldTime", time.to_string()));
                }
                let query_string = query_parts
                    .into_iter()
                    .map(|(key, value)| format!("{key}={}", percent_encode(&value)))
                    .collect::<Vec<_>>()
                    .join("&");
                (
                    "GET",
                    format!(
                        "/api/projects/{}/tasks/{}/relations?{query_string}",
                        percent_encode(project_id),
                        percent_encode(task_id)
                    ),
                    None,
                )
            }
            "request_candidate_task" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/actions",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(without(arguments, &["projectId"])),
            ),
            "save_brief" | "save_outline" | "save_chapter_plans" | "save_property_rules" => {
                let kind = match name {
                    "save_brief" => "brief",
                    "save_outline" => "outline",
                    "save_chapter_plans" => "plan",
                    "save_property_rules" => "propertyRules",
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
            "submit_revision_outcome" => {
                let writer_task_id =
                    required_string(arguments.get("writerTaskId"), "writerTaskId")?;
                (
                    "POST",
                    format!(
                        "/api/projects/{}/revision-outcomes/{}",
                        percent_encode(project_id),
                        percent_encode(writer_task_id)
                    ),
                    Some(without(arguments, &["projectId", "writerTaskId"])),
                )
            }
            "submit_scene_assessment" | "submit_commitment_extraction" => {
                let chapter_id = required_string(arguments.get("chapterId"), "chapterId")?;
                let resource = if name == "submit_scene_assessment" {
                    "scene-assessments"
                } else {
                    "commitment-extractions"
                };
                (
                    "POST",
                    format!(
                        "/api/projects/{}/chapters/{}/{resource}",
                        percent_encode(project_id),
                        percent_encode(chapter_id)
                    ),
                    Some(without(arguments, &["projectId", "chapterId"])),
                )
            }
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
            "publish_knowledge_batch" => {
                let batch_id = required_string(arguments.get("batchId"), "batchId")?;
                (
                    "POST",
                    format!(
                        "/api/projects/{}/modeling-batches/{}/publish",
                        percent_encode(project_id),
                        percent_encode(batch_id)
                    ),
                    Some(without(arguments, &["projectId", "batchId"])),
                )
            }
            _ => return Err(format!("unknown tool: {name}")),
        };
        let hash = operation_digest::operation_digest(name, &Value::Object(arguments.clone()))?;
        self.request(method, &path, body.as_ref(), meta, name, &hash)
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        meta: &Value,
        tool_name: &str,
        request_hash: &str,
    ) -> Result<(u16, Value), String> {
        let body = body.map(Value::to_string).unwrap_or_default();
        let mut headers = vec![
            ("Authorization", format!("Bearer {}", self.candidate_token)),
            ("X-Novel-Task-Capability", self.capability.clone()),
            ("Content-Type", "application/json".into()),
            ("Accept", "application/json".into()),
            ("Connection", "close".into()),
            ("X-Novel-Tool-Name", tool_name.to_owned()),
            ("X-Novel-Request-Hash", request_hash.to_owned()),
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
                if value.len() > 160 || value.chars().any(char::is_control) {
                    return Err("execution metadata contains an invalid header value".into());
                }
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
        "advisory" => &[
            "read_context",
            "read_commitment",
            "search_story",
            "query_story",
            "request_candidate_task",
        ],
        "reviewPlanningSeat" => &[
            "read_context",
            "search_story",
            "query_story",
            "submit_planning_review_report",
            "complete_task",
        ],
        "generateBrief" => &[
            "read_context",
            "search_story",
            "query_story",
            "save_brief",
            "complete_task",
        ],
        "generatePropertyRules" => &[
            "read_context",
            "read_property_rules",
            "search_story",
            "query_story",
            "save_property_rules",
            "complete_task",
        ],
        "generateOutline" => &[
            "read_context",
            "search_story",
            "query_story",
            "save_outline",
            "complete_task",
        ],
        "generatePlan" => &[
            "read_context",
            "search_story",
            "query_story",
            "save_chapter_plans",
            "complete_task",
        ],
        "generateChapter" => &[
            "read_context",
            "search_story",
            "search_relations",
            "query_story",
            "save_draft_checkpoint",
            "complete_task",
        ],
        "modelKnowledge" => &[
            "read_context",
            "search_story",
            "query_story",
            "publish_knowledge_batch",
            "complete_task",
        ],
        "assessSceneOutcome" => &[
            "read_context",
            "search_story",
            "query_story",
            "submit_scene_assessment",
            "complete_task",
        ],
        "assessRevisionOutcome" => &[
            "read_context",
            "search_story",
            "search_relations",
            "query_story",
            "submit_revision_outcome",
            "complete_task",
        ],
        "extractCommitments" => &[
            "read_context",
            "search_story",
            "query_story",
            "submit_commitment_extraction",
            "complete_task",
        ],
        "reviseCommitment" => &[
            "read_context",
            "read_commitment",
            "search_story",
            "query_story",
            "submit_commitment_candidate",
            "complete_task",
        ],
        _ => &[],
    }
}

fn tool_specs() -> Vec<Value> {
    [
        ("read_context", "Read a bounded task context snapshot. All supported tasks may include a typed queryPlan with required ranking, executed by Runtime and included as queryReport under the same byteBudget. First use ranking type literal; modelSemantic must bind the literal contentHash and judge every returned resultId. queryScope describes the derived policy: rolling planning excludes unwritten chapters, modeling and scene assessment bind source chapters, Writer retains chapter/scene/POV/time restrictions. Never upload a self-made report or widen the bound scope."),
        ("search_story", "Search source-bound story text by one contiguous substring, not a space-separated AND query. An empty result for '林夕 密函' does not imply that separate occurrences of 林夕 and 密函 are absent; query terms separately. Writer tasks may add an exact entityName and optional entityId to retrieve prior-chapter knowledge with evidence and coverage."),
        ("read_commitment", "Read one existing narrative commitment without author approval controls. Advisory reads within its captured project; revision tasks read only their frozen target base, even after their own checkpoints. Omitted Context Pack entries do not mean no commitment exists."),
        ("read_property_rules", "Read a root-owned frozen rule input page. Start with cursor null, then use nextCursor. Concatenate exact payloadPart strings in contiguous byteRange order and parse the resulting JSON payload; check contentHash and total bytes. endOfBaselineFromCursor only describes this suffix, not prior reading or semantic completeness. Never switch to a current candidate head."),
        ("query_story", "Execute a finite read-only evidence plan: text, passages, entity resolution, state and relations. text matches one contiguous substring; split multi-term questions into separate text steps. passages reads source text without keyword matching: cursor null first, maxChars 80-400, limit 1-4; continue with nextCursor under the same question and step ID. If pageComplete is false, reread the original cursor with smaller limit/maxChars instead of skipping omitted text. endOfScopeFromCursor never certifies prior pages or semantic completeness. ranking is required: literal first; modelSemantic keeps question/steps/byteBudget and binds literal contentHash with judgments for every returned resultId, score 0-1000, rationale at most 256 UTF-8 bytes. Whole plan at most 9000 bytes. Judgment is advice, not fact confirmation. Runtime derives task policy: project planning/review/advisory may read project evidence; rolling plans exclude future chapters; modeling and assessment bind source chapters. Writer retains chapter/scene/POV/time restrictions; unverified POV text remains hidden. No author state perspective or later identity/confirmation in bounded scope. Resolve evidenceRef in the shared dictionary; incomplete coverage is uncertainty, not absence."),
        ("search_relations", "Read relation claims within the bound task's source policy. Writer uses prior chapters and POV restrictions; revision assessment uses its frozen chapter scope. Returns source-bound evidence, never permission to change Canon."),
        ("request_candidate_task", "Request a candidate task only after an explicit author instruction. For generateChapter local edits, read the current draft first: target includes chapterId, chapterNumber and scope={type:selectedScenes,sourceRevisionId,groupIds:[stable sceneGroups id]}, at most 16 groups. Only selected bodies may change; preserve all other groups, order and metadata. Ambiguous targets require clarification, not whole-chapter rewriting. For reviseCommitment first read_commitment, then target requires commitmentId and its current revisionId; never guess an ambiguous identity. For extractCommitments target requires chapterId, chapterNumber and current sourceRevisionId. This records work, never confirms content."),
        ("save_brief", "Save a reviewable Brief candidate."),
        ("save_property_rules", "Save a finite propertyRules candidate for the bound root task. Structural approval belongs only to Author Review; this never confirms values or executes descriptions/exceptions."),
        ("save_outline", "Save a reviewable Outline candidate."),
        ("save_chapter_plans", "Save reviewable ChapterPlan candidates."),
        ("save_draft_checkpoint", "Save an immutable DraftRevision checkpoint."),
        ("submit_scene_assessment", "Save a source-bound, reviewable scene outcome assessment; this does not change Canon."),
        ("submit_revision_outcome", "Save the revisionOutcome candidate for exactly the frozen writerTaskId. Compare approved desiredOutcomes with bound original and changed new scenes; evidence uses exact new scene body quotes and Unicode offsets. Every criterion exactly once; supported/unmet require evidence. This does not change Canon."),
        ("submit_commitment_extraction", "Save up to sixteen open narrative commitment proposals bound to the task source and Unicode examined ranges. Proposals are not established commitments or facts; empty candidates do not certify absence."),
        ("submit_commitment_candidate", "Save a source-validated candidate revision of exactly the bound existing commitment. Keep payload.id unchanged; origin is derived by Runtime. Does not create a commitment, confirm it or change Canon. Fulfillment requires exact current body evidence."),
        ("submit_planning_review_report", "Submit an evidence-based planning review report."),
        ("publish_knowledge_batch", "Publish source-anchored candidate knowledge from a modeling task. Each relation requires a same-batch propositionId; character epistemic claims must reference that proposition explicitly."),
        ("complete_task", "Complete the bound task after persisted output validation."),
    ]
    .into_iter()
    .map(|(name, description)| {
        let operation_required: &[&str] = match name {
            "read_context" => &["purpose", "budget", "byteBudget"],
            "search_story" => &["query"],
            "read_commitment" => &["commitmentId"],
            "read_property_rules" => &["cursor"],
            "submit_commitment_candidate" => &["commitmentId", "payload", "expectedCandidateRevisionId", "idempotencyKey"],
            "query_story" => &["plan"],
            "search_relations" => &["entityId"],
            "request_candidate_task" => &["operation", "target", "instruction", "idempotencyKey", "expectedIntentRevision"],
            "save_brief" | "save_outline" | "save_chapter_plans" | "save_draft_checkpoint" | "save_property_rules" => {
                &["payload", "expectedCandidateRevisionId", "idempotencyKey"]
            }
            "submit_scene_assessment" | "submit_commitment_extraction" => {
                &["chapterId", "payload", "expectedCandidateRevisionId", "idempotencyKey"]
            }
            "submit_revision_outcome" => {
                &["writerTaskId", "payload", "expectedCandidateRevisionId", "idempotencyKey"]
            }
            "submit_planning_review_report" => &["runId", "report"],
            "publish_knowledge_batch" => &["batchId", "entities", "assertions"],
            "complete_task" => &["summary"],
            _ => &[],
        };
        let required = ["taskId", "projectId", "threadId", "expectedRevision"]
            .into_iter()
            .chain(operation_required.iter().copied())
            .collect::<Vec<_>>();
        let mut properties = json!({
                    "taskId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "projectId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "threadId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "expectedRevision": {"type": "integer", "minimum": 0},
                    "expectedIntentRevision": {"type": "integer", "minimum": 0},
                    "purpose": {"type": "string", "enum": ["planner", "writer", "critic"]},
                    "query": {"type": "string", "minLength": 1, "maxLength": 240},
                    "entityName": {"type": "string", "minLength": 1, "maxLength": 160},
                    "entityId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "kind": {"type": "string", "enum": ["brief", "outline", "plan", "draft", "commitment"]},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20},
                    "cursor": {"type": "string", "minLength": 1, "maxLength": 256},
                    "operation": {"type": "string", "enum": ["generateBrief", "generateOutline", "generatePlan", "generateChapter", "extractCommitments", "reviseCommitment", "generatePropertyRules"]},
                    "target": {"type": "object"},
                    "instruction": {"type": "string", "minLength": 1, "maxLength": 1000},
                    "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 160},
                    "budget": {"type": "integer", "minimum": 1, "maximum": 100000},
                    "byteBudget": {"type": "integer", "minimum": 4000, "maximum": 60000},
                    "storyTime": {"type": "integer"}, "readingChapter": {"type": "integer", "minimum": 0},
                    "sceneOrder": {"type": "integer", "minimum": 0}, "povCharacterId": {"type": "string"},
                    "payload": {"type": "object"},
                    "chapterId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "writerTaskId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "commitmentId": {"type": "string", "minLength": 1, "maxLength": 120},
                    "expectedCandidateRevisionId": {"type": ["string", "null"]},
                    "summary": {"type": "string", "maxLength": 4000},
                    "runId": {"type": "string"}, "report": {"type": "object"},
                    "batchId": {"type": "string"}, "entities": {"type": "array", "maxItems": 100},
                    "assertions": {"type": "array", "maxItems": 100}
        });
        if name == "search_relations" {
            properties.as_object_mut().expect("tool properties").extend(
                json!({
                    "relationType": {"type": "string", "minLength": 1, "maxLength": 160},
                    "direction": {"type": "string", "enum": ["outgoing", "incoming", "both"]},
                    "worldTime": {"type": "integer"},
                    "maxHops": {"type": "integer", "minimum": 1, "maximum": 3}
                }).as_object().expect("relation properties").clone()
            );
        }
        if name == "query_story" {
            properties["plan"] = query_plan_schema();
        }
        if name == "publish_knowledge_batch" {
            properties["entities"] = knowledge_schema::entities();
            properties["assertions"] = knowledge_schema::assertions();
        }
        if name == "read_property_rules" {
            properties["cursor"] = json!({"type":["string","null"],"minLength":1,"maxLength":100});
        }
        if name == "read_context" {
            properties["queryPlan"] = query_plan_schema();
        }
        // Publish only parameters consumed by this handler. Shared definitions
        // are not a shared contract: unrelated enums can reject valid task kinds.
        let optional: &[&str] = match name {
            "read_context" => &["storyTime", "readingChapter", "sceneOrder", "povCharacterId", "queryPlan"],
            "search_story" => &["kind", "cursor", "entityName", "entityId", "limit"],
            "search_relations" => &["relationType", "direction", "worldTime", "maxHops", "limit"],
            _ => &[],
        };
        properties.as_object_mut().expect("tool properties").retain(|key, _|
            required.contains(&key.as_str()) || optional.contains(&key.as_str()));
        let mut input_schema = json!({
            "type": "object", "properties": properties,
            "required": required, "additionalProperties": false
        });
        if name == "publish_knowledge_batch" {
            input_schema["$defs"] = knowledge_schema::definitions();
        }
        json!({"name":name,"description":description,"inputSchema":input_schema})
    })
    .chain(reading_checkpoint_tools::specs())
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
    if (caller != root || !parent.is_empty()) && tool_name == "read_property_rules" {
        return Err("Frozen rule inputs belong to the root task".into());
    }
    if (caller != root || !parent.is_empty())
        && matches!(tool_name, "read_reading_progress" | "checkpoint_reading")
    {
        return Err("Reading checkpoints belong to the root WorkStep".into());
    }
    if (caller != root || !parent.is_empty())
        && !matches!(
            tool_name,
            "read_context"
                | "search_story"
                | "search_relations"
                | "query_story"
                | "read_commitment"
        )
    {
        return Err("Subagents may only use read-only tools".into());
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

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
