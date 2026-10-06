mod delivery_review_schema;
use std::env;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;

use serde_json::Value;
use serde_json::json;
mod argument_validation;
mod creative_strategy;
mod quality_tools;
mod brief_schema;
mod chapter_plan_schema;
mod draft_schema;
mod evidence_tool;
mod extraction_schema;
mod hypothesis_tool;
mod impact_tool;
mod knowledge_schema;
mod operation_digest;
mod planning_schema;
mod publication_gate;
mod read_progress;
mod task_source_tools;

mod chapter_structure_tool;
mod query_plan_schema;
mod query_plan_validation;
mod reading_checkpoint_tools;

const PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

struct RuntimeClient {
    argument_validation: argument_validation::ArgumentValidation,
    publication_gate: publication_gate::PublicationGate,
    read_progress: read_progress::ReadProgress,
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
            publication_gate: publication_gate::PublicationGate::default(),
            read_progress: Default::default(),
            argument_validation: Default::default(),
            host: host.trim_matches(['[', ']']).to_owned(),
            port: port.parse().map_err(|_| "Runtime port is invalid")?,
            candidate_token: required_environment("NOVEL_CODEX_CANDIDATE_TOKEN")?,
            capability: required_environment("NOVEL_CODEX_TASK_CAPABILITY")?,
            operation: required_environment("NOVEL_CODEX_OPERATION")?,
        })
    }

    fn exposed_tools(&self) -> Vec<Value> {
        self.operation_tools()
            .into_iter()
            .filter(|tool| {
                tool["name"]
                    .as_str()
                    .is_some_and(|name| self.publication_gate.allows_tool(name))
            })
            .collect()
    }

    fn operation_tools(&self) -> Vec<Value> {
        tool_specs()
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        (name == "read_creative_strategy" && creative_strategy::allowed(&self.operation))
                            || (name == "read_task_source"
                            && task_source_tools::purpose(&self.operation).is_ok())
                            || (self.operation == "advisory"
                                && matches!(
                                    name,
                                    "read_work_status"
                                        | "request_work_action"
                                        | "request_generation_handoff"
                                ))
                            || allowed_tools(&self.operation).contains(&name)
                            || (!matches!(
                                self.operation.as_str(),
                                "assessRevisionOutcome" | "assessChapterStructure"
                            ) && reading_checkpoint_tools::allowed(&self.operation, name))
                    })
            })
            .map(|mut tool| {
                if matches!(tool["name"].as_str(), Some("query_story" | "read_context")) {
                    for field in ["plan", "queryPlan"] {
                        if let Some(slot) =
                            tool.pointer_mut(&format!("/inputSchema/properties/{field}"))
                        {
                            *slot = query_plan_schema::transport_schema(&self.operation);
                        }
                    }
                }
                tool
            })
            .collect()
    }

    fn call(&self, name: &str, arguments: &Value, meta: &Value) -> Result<(u16, Value), String> {
        self.argument_validation.ensure_active(name)?;
        let tool = self
            .operation_tools()
            .into_iter()
            .find(|tool| tool["name"] == name)
            .ok_or_else(|| "tool is not allowed for this generation operation".to_owned())?;
        // Authenticate execution metadata before consuming any correction attempt.
        // Missing/mistyped threadId is a schema error; an actual different identity
        // remains an authorization failure, not a retryable argument correction.
        let claimed_thread = arguments
            .get("threadId")
            .and_then(Value::as_str)
            .unwrap_or_else(|| meta["rootThreadId"].as_str().unwrap_or_default());
        validate_execution_identity(meta, claimed_thread, name)?;
        if !self.publication_gate.allows_tool(name) {
            return Err(
                "Knowledge publication committed; call complete_task now. Other tools are closed."
                    .into(),
            );
        }
        let normalized_arguments = self.argument_validation.local(name,
            operation_digest::host_json(arguments), "JSON numbers must be finite and exact integers within the JavaScript safe range; use strings for larger identifiers.")?;
        self.argument_validation
            .validate(name, &normalized_arguments, &tool["inputSchema"])?;
        if name == "read_creative_strategy" {
            // Only public static guidance, never task data or a permission receipt.
            let result = creative_strategy::read(normalized_arguments["strategyId"].as_str().unwrap())?;
            self.argument_validation.complete_preflight(name);
            return Ok((200, result));
        }
        let arguments = normalized_arguments
            .as_object()
            .ok_or("validated arguments must be an object")?;
        let project_id = self.argument_validation.local(
            name,
            required_string(arguments.get("projectId"), "projectId"),
            "projectId must be a non-empty string.",
        )?;
        let task_id = self.argument_validation.local(
            name,
            required_string(arguments.get("taskId"), "taskId"),
            "taskId must be a non-empty string.",
        )?;
        let thread_id = self.argument_validation.local(
            name,
            required_string(arguments.get("threadId"), "threadId"),
            "threadId must be a non-empty string.",
        )?;
        let expected_revision = arguments
            .get("expectedRevision")
            .and_then(Value::as_i64)
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| "expectedRevision must be a non-negative integer".to_owned())?;
        validate_execution_identity(meta, thread_id, name)?;
        self.publication_gate
            .check(name, project_id, task_id, thread_id)?;
        if name == "request_candidate_task" && arguments["operation"] == "generateChapter" {
            self.argument_validation.local(
                name,
                required_string(arguments["target"].get("chapterId"), "target.chapterId"),
                "generateChapter requires target.chapterId of an existing chapter. For new chapters or continuous creation, call request_generation_handoff with the actual targetChapters (1-100) and author goal; never invent IDs or create placeholders.",
            )?;
        }
        let draft_payload = if name == "save_draft_checkpoint" {
            Some(self.argument_validation.local(name, draft_schema::runtime_payload(&arguments["payload"]), "Use the full DraftPayload contract: chapterId, chapterNumber, title, completeness, and exactly one of body or sceneGroups; scene group IDs must be unique.")?)
        } else {
            None
        };
        if name == "save_outline" {
            self.argument_validation.local(
                name,
                planning_schema::validate_outline(&arguments["payload"]),
                "Match the declared outline payload and its node fields.",
            )?;
        }
        if name == "save_chapter_plans" {
            self.argument_validation.local(name, chapter_plan_schema::validate(&arguments["payload"]), "Match the declared chapter plan payload; preserve required chapter and scene fields.")?;
        }
        if name == "save_brief" {
            self.argument_validation.local(name, brief_schema::validate(&arguments["payload"]), "Use a string premise and optional string promise, stakes and endingDirection; no other fields.")?;
        }
        if name == "submit_commitment_extraction" {
            self.argument_validation.local(name, extraction_schema::validate(&arguments["payload"]), "Provide at most 16 candidates; plotThreadId/characterArcId must be null or non-empty IDs up to 160 characters. Evidence fields are objectId, revisionId, quote, start and end only.")?;
        }
        if name == "analyze_story_impact" {
            self.argument_validation.local(
                name,
                impact_tool::validate(&normalized_arguments),
                "Use a non-empty entityId within 160 bytes and the declared maxHops/limit bounds.",
            )?;
        }
        if name == "query_story" {
            self.argument_validation.local(
                name,
                query_plan_validation::validate(&arguments["plan"], &self.operation, "plan"),
                "Use the declared task-specific query plan contract and byte budget.",
            )?;
        }
        if name == "read_context"
            && let Some(plan) = arguments.get("queryPlan")
        {
            self.argument_validation.local(
                name,
                query_plan_validation::validate(plan, &self.operation, "queryPlan"),
                "Use the declared task-specific queryPlan contract and byte budget.",
            )?;
        }
        if name == "simulate_story_hypothesis" {
            self.argument_validation.local(name, hypothesis_tool::validate(&normalized_arguments), "Use fields for the selected action only, at most 25 exact baseline assumptions, input within 8KiB and limit 1-16.")?;
        }
        if name == "read_chapter_structure_source" {
            self.argument_validation.local(
                name,
                chapter_structure_tool::validate(&normalized_arguments),
                "Use the declared chapter structure source cursor and page limits.",
            )?;
        }
        if name == "resolve_story_evidence" {
            self.argument_validation.local(
                name,
                evidence_tool::validate(&normalized_arguments),
                "Use non-empty sourceObjectId/sourceRevisionId within 160 UTF-8 bytes and an exact source quote of 1-2000 Unicode characters.",
            )?;
        }

        let (method, path, body) = match name {
            "read_task_source"
            | "read_work_status"
            | "request_work_action"
            | "request_generation_handoff" => {
                self.argument_validation.local(name, task_source_tools::request(name, &normalized_arguments), "Copy sourceId and nextCursor from taskSources; preserve the declared Work action fields, version, digest and instruction bounds.")?
            }
            "read_commitment" | "submit_commitment_candidate" => {
                let id = self.argument_validation.local(name, required_string(arguments.get("commitmentId"), "commitmentId"), "commitmentId must be a non-empty string.")?;
                if id.len() > 120 {
                    return self.argument_validation.local(name, Err("commitmentId exceeds 120 bytes".into()), "commitmentId must be at most 120 UTF-8 bytes.");
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
            "read_quality_notebook" | "read_quality_review" | "record_quality_note" | "read_quality_packet" | "submit_quality_reading" => {
                let (action,key)=match name {"read_quality_notebook"=>("read","before"),"read_quality_review"=>("review","packetId"),"record_quality_note"=>("change","mutation"),"read_quality_packet"=>("page","page"),_=>("report","report")};
                let mut payload=json!({"threadId":thread_id,"expectedRevision":expected_revision,"action":action});
                payload[key]=arguments[key].clone();
                ("POST",format!("/api/projects/{}/tasks/{}/quality",percent_encode(project_id),percent_encode(task_id)),Some(payload))
            }
            "read_context" => {
                let purpose = task_source_tools::purpose(&self.operation)?;
                let mut query = vec![
                    ("taskId", task_id.to_owned()),
                    ("threadId", thread_id.to_owned()),
                    ("projectRevision", expected_revision.to_string()),
                    ("purpose", purpose.to_owned()),
                    (
                        "budget",
                        task_source_tools::context_budget(&normalized_arguments, "budget")?
                            .to_string(),
                    ),
                    (
                        "byteBudget",
                        task_source_tools::context_budget(&normalized_arguments, "byteBudget")?
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
            "request_planning_review" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/planning-reviews",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(without(arguments, &["projectId", "taskId"])),
            ),
            "simulate_story_hypothesis" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/hypothesis",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(without(arguments, &["projectId", "taskId"])),
            ),
            "read_chapter_structure_source" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/chapter-structure-source",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(without(arguments, &["projectId", "taskId"])),
            ),
            "resolve_story_evidence" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/source-evidence",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(without(arguments, &["projectId", "taskId"])),
            ),
            "analyze_story_impact" => (
                "POST",
                format!(
                    "/api/projects/{}/tasks/{}/impact-analysis",
                    percent_encode(project_id),
                    percent_encode(task_id)
                ),
                Some(without(arguments, &["projectId", "taskId"])),
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
                let query = self.argument_validation.local(name, required_string(arguments.get("query"), "query"), "query must be a non-empty string.")?;
                if query.chars().count() > 240 {
                    return self.argument_validation.local(name, Err("query exceeds 240 characters".into()), "query must be at most 240 Unicode characters.");
                }
                let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(10);
                if !(1..=20).contains(&limit) {
                    return self.argument_validation.local(name, Err("limit must be between 1 and 20".into()), "limit must be between 1 and 20.");
                }
                let mut query_parts = vec![
                    ("threadId", thread_id.to_owned()),
                    ("expectedRevision", expected_revision.to_string()),
                    ("q", query.to_owned()),
                    ("limit", limit.to_string()),
                ];
                for key in ["kind", "cursor", "entityName", "entityId"] {
                    if let Some(value) = arguments.get(key) {
                        query_parts.push((key, self.argument_validation.local(name, required_string(Some(value), key), "Optional search filters must be non-empty strings.")?.to_owned()));
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
                let entity_id = self.argument_validation.local(name, required_string(arguments.get("entityId"), "entityId"), "entityId must be a non-empty string.")?;
                if entity_id.len() > 160 {
                    return self.argument_validation.local(name, Err("entityId exceeds 160 bytes".into()), "entityId must be at most 160 UTF-8 bytes.");
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
                    return self.argument_validation.local(name, Err("relation search budget is invalid".into()), "Use limit 1-20 and maxHops 1-3.");
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
                        query_parts.push((key, self.argument_validation.local(name, required_string(Some(value), key), "Optional search filters must be non-empty strings.")?.to_owned()));
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
            "save_draft_checkpoint" => {
                let mut body = without(arguments, &["projectId"]);
                body["payload"] =
                    draft_payload.ok_or_else(|| "Draft payload was not validated".to_owned())?;
                (
                    "POST",
                    format!(
                        "/api/projects/{}/drafts/checkpoints",
                        percent_encode(project_id)
                    ),
                    Some(body),
                )
            }
            "submit_revision_outcome" => {
                let writer_task_id =
                    self.argument_validation.local(name, required_string(arguments.get("writerTaskId"), "writerTaskId"), "writerTaskId must be a non-empty string.")?;
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
                let chapter_id = self.argument_validation.local(name, required_string(arguments.get("chapterId"), "chapterId"), "chapterId must be a non-empty string.")?;
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
                    "summary": arguments.get("summary").cloned().unwrap_or_else(|| json!("")),
                    "deliveryReview": arguments.get("deliveryReview").cloned().unwrap_or(Value::Null)
                })),
            ),
            "submit_planning_review_report" => {
                let run_id = self.argument_validation.local(name, required_string(arguments.get("runId"), "runId"), "runId must be a non-empty string.")?;
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
                let batch_id = self.argument_validation.local(name, required_string(arguments.get("batchId"), "batchId"), "batchId must be a non-empty string.")?;
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
        self.argument_validation.complete_preflight(name);
        let mut response = self.request(method, &path, body.as_ref(), meta, name, &hash)?;
        if name == "request_generation_handoff" && (200..300).contains(&response.0) {
            let proposal = &response.1["proposal"];
            if proposal["sourceTaskId"] != task_id
                || proposal["threadId"] != thread_id
                || proposal["projectId"] != project_id
                || proposal["targetChapters"] != arguments["targetChapters"]
                || proposal["proposalId"]
                    .as_str()
                    .is_none_or(|id| id.is_empty() || id.len() > 160)
            {
                return Err("Generation proposal receipt binding is invalid".into());
            }
            response.1 = json!({"proposalId":proposal["proposalId"],"targetChapters":proposal["targetChapters"],"state":"proposed","approvalRequired":true});
        }
        self.read_progress.observe(name, &hash, &mut response);
        task_source_tools::add_read_recovery(name, &normalized_arguments, &mut response);
        self.publication_gate
            .observe(name, &normalized_arguments, &response);
        Ok(response)
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
                Err(error) => {
                    let mut result =
                        json!({"content": [{"type": "text", "text": error}], "isError": true});
                    if let Ok(feedback) = serde_json::from_str::<Value>(&error) {
                        if matches!(
                            feedback["code"].as_str(),
                            Some("invalid_tool_arguments" | "tool_argument_correction_limit")
                        ) {
                            result["structuredContent"] = feedback;
                        }
                    }
                    Some(rpc_result(id, result))
                }
            }
        }
        _ => Some(rpc_error(id, -32601, "Method not found")),
    }
}

fn allowed_tools(operation: &str) -> &'static [&'static str] {
    match operation {
        "advisory" => &[
            "read_quality_notebook",
            "read_quality_review",
            "record_quality_note",
            "complete_task",
            "simulate_story_hypothesis",
            "resolve_story_evidence",
            "read_context",
            "analyze_story_impact",
            "read_commitment",
            "search_story",
            "query_story",
            "request_candidate_task",
            "request_planning_review",
        ],
        "reviewQuality" => &["read_quality_packet", "submit_quality_reading", "complete_task"],
        "reviewPlanningSeat" => &[
            "read_context",
            "search_story",
            "query_story",
            "submit_planning_review_report",
            "complete_task",
        ],
        "generateBrief" => &[
            "read_quality_notebook",
            "read_context",
            "search_story",
            "query_story",
            "save_brief",
            "complete_task",
        ],
        "generatePropertyRules" => &[
            "read_context",
            "search_story",
            "query_story",
            "save_property_rules",
            "complete_task",
        ],
        "generateOutline" => &[
            "read_quality_notebook",
            "read_context",
            "search_story",
            "query_story",
            "save_outline",
            "complete_task",
        ],
        "generatePlan" => &[
            "read_quality_notebook",
            "read_context",
            "search_story",
            "query_story",
            "save_chapter_plans",
            "complete_task",
        ],
        "generateChapter" => &[
            "read_quality_notebook",
            "read_context",
            "search_story",
            "search_relations",
            "query_story",
            "save_draft_checkpoint",
            "complete_task",
        ],
        "modelKnowledge" => &[
            "resolve_story_evidence",
            "read_context",
            "search_story",
            "query_story",
            "publish_knowledge_batch",
            "complete_task",
        ],
        "assessSceneOutcome" => &[
            "resolve_story_evidence",
            "read_context",
            "search_story",
            "query_story",
            "submit_scene_assessment",
            "complete_task",
        ],
        "assessChapterStructure" => &[
            "read_context",
            "query_story",
            "search_story",
            "resolve_story_evidence",
            "analyze_story_impact",
            "read_chapter_structure_source",
            "complete_task",
        ],
        "assessRevisionOutcome" => &[
            "resolve_story_evidence",
            "read_context",
            "search_story",
            "search_relations",
            "query_story",
            "submit_revision_outcome",
            "complete_task",
        ],
        "extractCommitments" => &[
            "resolve_story_evidence",
            "read_context",
            "search_story",
            "query_story",
            "submit_commitment_extraction",
            "complete_task",
        ],
        "reviseCommitment" => &[
            "resolve_story_evidence",
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
        ("analyze_story_impact", impact_tool::DESCRIPTION),
        ("read_context", "Read the bound task's context and taskSources directory. Purpose is derived from the trusted operation; budget defaults to 24 entries and byteBudget to 24000, both optional and bounded. Writer position defaults to its exact target and cannot be widened. Directory metadata has not delivered content: select needed sourceId with read_task_source, preserving frozen source/hash and original task visibility. Original/selected/locked material required for a modification must be fully read; ordinary questions do not scan all sources by default. Coverage gaps, empty retrieval and a suffix-ending page never prove absence or semantic completeness. Optional queryPlan executes the same scope: literal ranking first, not a caller-made report. delegationTargets only identify exact available targets, never authorize changes."),
        ("search_story", "Search source-bound story text by one contiguous substring, not a space-separated AND query. An empty result for '林夕 密函' does not imply that separate occurrences of 林夕 and 密函 are absent; query terms separately. Writer tasks may add an exact entityName and optional entityId to retrieve prior-chapter knowledge with evidence and coverage."),
        ("read_commitment", "Read one existing narrative commitment without author approval controls. Advisory reads within its captured project; revision tasks read only their frozen target base, even after their own checkpoints. Omitted Context Pack entries do not mean no commitment exists."),
        ("query_story", "Execute a finite read-only evidence plan: text, passages, entity resolution, state and relations. text matches one contiguous substring; split multi-term questions into separate text steps. passages reads source text without keyword matching: cursor null first, maxChars 80-400, limit 1-4; continue with nextCursor under the same question and step ID. If pageComplete is false, reread the original cursor with smaller limit/maxChars instead of skipping omitted text. endOfScopeFromCursor never certifies prior pages or semantic completeness. ranking is required: literal first; modelSemantic keeps question/steps/byteBudget and binds literal contentHash with judgments for every returned resultId, score 0-1000, rationale at most 256 UTF-8 bytes. Whole plan at most 9000 bytes. Judgment is advice, not fact confirmation. Runtime derives task policy: project planning/review/advisory may read project evidence; rolling plans exclude future chapters; modeling and assessment bind source chapters. Writer retains chapter/scene/POV/time restrictions; unverified POV text remains hidden. No author state perspective or later identity/confirmation in bounded scope. Resolve evidenceRef in the shared dictionary; incomplete coverage is uncertainty, not absence."),
        ("search_relations", "Read relation claims within the bound task's source policy. Writer uses prior chapters and POV restrictions; revision assessment uses its frozen chapter scope. Returns source-bound evidence, never permission to change Canon."),
        ("request_planning_review", "Use a separate planning review when useful for the author goal; ordinary questions may be answered directly. Read the frozen current outline/plan first. Register its exact artifactKind/revisionId, targetNodeId (null means whole artifact), 1-3 distinct roles structure/character/experience. Choose only necessary views; preserve every view the author explicitly requested, and request additional views later only when findings justify them. Supply bounded focus (at most 1000 Unicode characters and 4000 UTF-8 bytes). Keep the original author intent; focus is only the review question, never new authority. Reuse idempotencyKey only for the identical request, with the current expectedIntentRevision. Host executes independent seats within this conversation Work remaining allowance. Registration is not saved reports or adoption; ordinary discussion does not require this tool."),
        ("request_candidate_task", "Request a candidate task only after an explicit author instruction. generateChapter modifies an EXISTING chapter only. For new chapters, continuation or a batch such as the first 100 chapters, use request_generation_handoff(targetChapters=the actual requested count, instruction=the author goal); never invent a chapter ID or ask the author to create placeholder chapters. Planning artifacts do not satisfy a chapter creation goal. For an explicitly requested whole-chapter rewrite, verify the existing chapter and use target.scope={type:wholeChapter}; the worker reads its frozen original through writerBaselinePassages. Never ask the author to supply technical IDs. For generateChapter local edits, read the current draft first: target includes chapterId, chapterNumber and scope={type:selectedScenes,sourceRevisionId,groupIds:[stable sceneGroups id]}, at most 16 groups. Only selected bodies may change; preserve all other groups, order and metadata. Ambiguous targets require clarification, not whole-chapter rewriting. For reviseCommitment first read_commitment, then target requires commitmentId and its current revisionId; never guess an ambiguous identity. For extractCommitments target requires chapterId, chapterNumber and current sourceRevisionId. This records work, never confirms content."),
        ("save_brief", "Save only a Brief candidate: required string premise, optional string promise/stakes/endingDirection. No chapter prose or other fields. This does not save chapters or formally adopt the brief."),
        ("save_property_rules", "Save a finite propertyRules candidate for the bound root task. Structural approval belongs only to Author Review; this never confirms values or executes descriptions/exceptions."),
        ("save_outline", "Save a reviewable Outline candidate."),
        ("save_chapter_plans", "Save reviewable ChapterPlan candidates."),
        ("save_draft_checkpoint", "Save an immutable DraftRevision checkpoint. Supply chapterId and chapterNumber from the frozen target, title and completeness. Send exactly one full-text representation: body-only OR sceneGroups-only, never both. With sceneGroups omit body; MCP derives the canonical body without changing the groups. A bound planned whole-chapter chapterComplete requires explicit sceneGroups with at least one nonempty group mapped to an exact bound sceneId; all non-null sceneIds must be legal. Body-only is allowed for other checkpoints and unbound chapters. Selected-scene revisions still submit all original and edited groups, not a partial patch. Supplied groups require stable id, body and locked, with protected metadata preserved. Mapping does not mean planned effects were realized. Copy expectedCandidateRevisionId exactly from the manifest: absent head is JSON null, never the string \"null\". A rejected call has not saved the chapter; correct its arguments without changing target or CAS."),
        ("submit_scene_assessment", "Save a source-bound candidate assessment, not Canon. Every assessments item requires sceneId, verdict, rationale, recommendation and effectAssessments; every planned effect and bound commitment must be assessed exactly once. Copy actualEventRefs only as {modelingBatchId,assertionIndex} from this scene's scenePlanDraftLinks.actualEventRefs, never infer scene boundaries or borrow another scene's event. If actualSceneGroupIds or usable events are empty, use insufficientEvidence and empty event refs for effects and commitments, preserving bound reference/state; set the scene verdict to insufficientEvidence and recommend repairDraft for missing mapping. The scene verdict must match the effects and commitments; repairDraft is advice, not permission to change prose."),
        ("submit_revision_outcome", "Save the revisionOutcome candidate for exactly the frozen writerTaskId. Compare approved desiredOutcomes with bound original and changed new scenes. Evidence needs groupId and an exact new scene body quote; Runtime anchors unique quotes. Optional Unicode start/end must both be correct; repeated quotes require explicit anchors or a longer unique quote. Every criterion exactly once; supported/unmet require evidence. This does not change Canon."),
        ("submit_commitment_extraction", "Atomically replace the complete narrative commitment proposal report bound to the task source and Unicode examined ranges. Submit the whole candidates array once, at most sixteen proposals and 40 KB; this does not append individual proposals. A later correction must include every proposal to retain, using the returned revisionId for CAS. The receipt candidateCount counts only that saved revision; report the last successful receipt count, never the sum of previous saves. Proposals are not established commitments or facts; empty candidates do not certify absence."),
        ("submit_commitment_candidate", "Save a source-validated candidate revision of exactly the bound existing commitment. Keep payload.id unchanged; origin is derived by Runtime. Does not create a commitment, confirm it or change Canon. Fulfillment requires exact current body evidence."),
        ("submit_planning_review_report", "Submit an evidence-based planning review report."),
        ("publish_knowledge_batch", "Atomically publish source-anchored candidate knowledge once. Each relation requires a same-batch propositionId; character epistemic claims must reference that proposition explicitly. A committed receipt means publication is durable and immutable: immediately call complete_task, never revise or republish the batch."),
        ("complete_task", "Complete the bound task after persisted output validation. Final Work decision with no selected next action MUST submit deliveryReview bound to current read_work_status version/sourceDigest and all current saved revision outputs. This is agent self-assessment, never literary certification. Workers and decisions dispatching another action omit deliveryReview."),
    ]
    .into_iter()
    .map(|(name, description)| {
        let operation_required: &[&str] = match name {
            "read_context" => &[],
            "search_story" => &["query"],
            "read_commitment" => &["commitmentId"],
            "submit_commitment_candidate" => &["commitmentId", "payload", "expectedCandidateRevisionId", "idempotencyKey"],
            "query_story" => &["plan"],
            "analyze_story_impact" => &["entityId"],
            "search_relations" => &["entityId"],
            "request_planning_review" => &["artifactKind", "revisionId", "targetNodeId", "roles", "focus", "idempotencyKey", "expectedIntentRevision"],
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
                    "artifactKind": {"type":"string","enum":["outline","plan"]},
                    "revisionId": {"type":"string","minLength":1,"maxLength":160},
                    "targetNodeId": {"type":["string","null"],"minLength":1,"maxLength":160},
                    "roles": {"type":"array","minItems":1,"maxItems":3,"uniqueItems":true,"items":{"type":"string","enum":["structure","character","experience"]}},
                    "focus": {"type":"string","maxLength":1000},
                    "expectedIntentRevision": {"type": "integer", "minimum": 0, "description":"Copy authorIntentState.expectedIntentRevision from read_context after reading the current author instructions; initial value is 0, independent of project revision. Request actions only when authorIntentState.canRequestActions is true."},
                    "purpose": {"type": "string", "enum": ["planner", "writer", "critic"]},
                    "query": {"type": "string", "minLength": 1, "maxLength": 240},
                    "entityName": {"type": "string", "minLength": 1, "maxLength": 160},
                    "entityId": {"type": "string", "minLength": 1, "maxLength": 160},
                    "kind": {"type": "string", "enum": ["brief", "outline", "plan", "draft", "commitment"]},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20},
                    "cursor": {"type": "string", "minLength": 1, "maxLength": 256},
                    "operation": {"type": "string", "enum": ["generateBrief", "generateOutline", "generatePlan", "generateChapter", "modelKnowledge", "extractCommitments", "reviseCommitment", "generatePropertyRules"]},
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
                    "expectedCandidateRevisionId": {"type": ["string", "null"],
                        "description":"Copy the frozen manifest CAS value exactly, or the last successful save receipt revisionId. A missing candidate is JSON null, never the string \"null\". Do not invent or convert IDs."},
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
            properties["plan"] = query_plan_schema::transport_schema("advisory");
        }
        if name == "analyze_story_impact" {
            properties.as_object_mut().expect("tool properties").extend(impact_tool::properties().as_object().expect("impact properties").clone());
        }
        if name == "complete_task" {
            properties["deliveryReview"] = delivery_review_schema::schema();
        }
        if name == "save_draft_checkpoint" {
            properties["payload"] = draft_schema::transport_payload();
            properties["expectedCandidateRevisionId"]["description"] = json!(
                "Copy the manifest CAS value exactly. A missing candidate is JSON null, not the string \"null\"; never invent or convert a revision ID."
            );
        }
        if name == "save_brief" {
            properties["payload"] = brief_schema::payload();
        }
        if name == "save_outline" {
            properties["payload"] = planning_schema::outline_payload();
        }
        if name == "save_chapter_plans" {
            properties["payload"] = chapter_plan_schema::payload();
        }
        if name == "submit_commitment_extraction" {
            properties["payload"] = extraction_schema::payload();
        }
        if name == "publish_knowledge_batch" {
            properties["entities"] = knowledge_schema::entities();
            properties["assertions"] = knowledge_schema::assertions();
        }
        if name == "read_context" {
            properties["queryPlan"] = query_plan_schema::transport_schema("advisory");
        }
        // Publish only parameters consumed by this handler. Shared definitions
        // are not a shared contract: unrelated enums can reject valid task kinds.
        let optional: &[&str] = match name {
            "complete_task" => &["deliveryReview"],
            "read_context" => &["budget", "byteBudget", "storyTime", "readingChapter", "sceneOrder", "povCharacterId", "queryPlan"],
            "search_story" => &["kind", "cursor", "entityName", "entityId", "limit"],
            "search_relations" => &["relationType", "direction", "worldTime", "maxHops", "limit"],
            "analyze_story_impact" => &["maxHops", "limit"],
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
    .chain(std::iter::once(chapter_structure_tool::spec()))
    .chain(std::iter::once(creative_strategy::spec()))
    .chain(quality_tools::specs())
    .chain(task_source_tools::specs())
    .chain(std::iter::once(evidence_tool::spec()))
    .chain(std::iter::once(hypothesis_tool::spec()))
    .chain(reading_checkpoint_tools::specs())
    .collect()
}

fn validate_execution_identity(
    meta: &Value,
    thread_id: &str,
    tool_name: &str,
) -> Result<(), String> {
    for field in [
        "rootThreadId",
        "callerThreadId",
        "parentThreadId",
        "turnId",
        "callId",
    ] {
        if let Some(value) = meta.get(field).and_then(Value::as_str) {
            if value.len() > 160 || value.chars().any(char::is_control) {
                return Err("execution metadata contains an invalid header value".into());
            }
        }
    }
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
    if (caller != root || !parent.is_empty())
        && matches!(
            tool_name,
            "read_task_source"
                | "read_work_status"
                | "request_work_action"
                | "request_generation_handoff"
        )
    {
        return Err("Task sources and Work decisions belong to the bound root task".into());
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
                | "read_creative_strategy"
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

#[cfg(test)]
#[path = "publication_gate_tests.rs"]
mod publication_gate_tests;

#[cfg(test)]
#[path = "draft_checkpoint_tests.rs"]
mod draft_checkpoint_tests;

#[cfg(test)]
#[path = "conversation_analysis_tests.rs"]
mod conversation_analysis_tests;

#[cfg(test)]
#[path = "extraction_schema_tests.rs"]
mod extraction_schema_tests;

#[cfg(test)]
#[path = "evidence_tool_tests.rs"]
mod evidence_tool_tests;

#[cfg(test)]
#[path = "hypothesis_tool_tests.rs"]
mod hypothesis_tool_tests;

#[cfg(test)]
#[path = "query_plan_schema_tests.rs"]
mod query_plan_schema_tests;

#[cfg(test)]
#[path = "query_plan_validation_tests.rs"]
mod query_plan_validation_tests;

#[cfg(test)]
#[path = "planning_schema_tests.rs"]
mod planning_schema_tests;

#[cfg(test)]
#[path = "brief_schema_tests.rs"]
mod brief_schema_tests;

#[cfg(test)]
#[path = "chapter_plan_schema_tests.rs"]
mod chapter_plan_schema_tests;

#[cfg(test)]
#[path = "chapter_structure_tool_tests.rs"]
mod chapter_structure_tool_tests;

#[cfg(test)]
#[path = "task_source_tools_tests.rs"]
mod task_source_tools_tests;
