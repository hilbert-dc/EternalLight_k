use serde_json::{Value, json};

pub(crate) fn purpose(operation: &str) -> Result<&'static str, String> {
    match operation {
        "advisory"
        | "generateBrief"
        | "generateOutline"
        | "generatePlan"
        | "generatePropertyRules"
        | "reviewPlanningSeat" => Ok("planner"),
        "generateChapter" => Ok("writer"),
        "modelKnowledge"
        | "extractCommitments"
        | "assessSceneOutcome"
        | "assessRevisionOutcome"
        | "assessChapterStructure"
        | "reviseCommitment" => Ok("critic"),
        _ => Err("Unknown bound operation".into()),
    }
}

pub(crate) fn request(
    name: &str,
    args: &Value,
) -> Result<(&'static str, String, Option<Value>), String> {
    let base = format!(
        "/api/projects/{}/tasks/{}",
        super::percent_encode(args["projectId"].as_str().unwrap_or_default()),
        super::percent_encode(args["taskId"].as_str().unwrap_or_default())
    );
    let binding = format!(
        "threadId={}&expectedRevision={}",
        super::percent_encode(args["threadId"].as_str().unwrap_or_default()),
        args["expectedRevision"]
    );
    match name {
        "read_task_source" => {
            let source = args["sourceId"]
                .as_str()
                .filter(|id| id.len() == 71 && id.starts_with("source-"))
                .ok_or("sourceId must be copied from taskSources")?;
            let mut path = format!(
                "{base}/task-source?{binding}&sourceId={}",
                super::percent_encode(source)
            );
            match &args["cursor"] {
                Value::Null => {}
                Value::String(cursor) if !cursor.is_empty() && cursor.len() <= 100 => {
                    path.push_str(&format!("&cursor={}", super::percent_encode(cursor)))
                }
                _ => return Err("cursor must be null or a bounded nextCursor".into()),
            }
            Ok(("GET", path, None))
        }
        "read_work_status" => {
            let mut path = format!("{base}/work-decision?{binding}");
            match &args["cursor"] {
                Value::Null => {}
                Value::String(cursor) if !cursor.is_empty() && cursor.len() <= 100 => {
                    path.push_str(&format!("&cursor={}", super::percent_encode(cursor)))
                }
                _ => return Err("cursor must be null or the exact bounded nextCursor".into()),
            }
            Ok(("GET", path, None))
        }
        "request_work_action" | "request_generation_handoff" => {
            for field in ["instruction", "idempotencyKey"] {
                let limit = if field == "instruction" { 4_000 } else { 160 };
                if args[field].as_str().is_none_or(|value| {
                    value.trim().is_empty()
                        || value.len() > limit
                        || (field == "instruction" && value.chars().count() > 1000)
                }) {
                    return Err(format!("Invalid {field}"));
                }
            }
            let suffix = if name == "request_work_action" {
                for field in ["expectedSourceDigest"] {
                    if args[field]
                        .as_str()
                        .is_none_or(|value| value.trim().is_empty() || value.len() > 160)
                    {
                        return Err(format!("Invalid {field}"));
                    }
                }
                let action = args.get("actionKey").filter(|v| !v.is_null());
                let proposal = args.get("proposal").filter(|v| !v.is_null());
                if action.is_some() == proposal.is_some() {
                    return Err("Provide exactly one of actionKey or proposal".into());
                }
                if action.is_some_and(|v| v.as_str().is_none_or(|s| s.trim().is_empty() || s.len() > 160)) {
                    return Err("Invalid actionKey".into());
                }
                if let Some(proposal) = proposal {
                    validate_proposal(proposal)?;
                }
                if args["expectedWorkVersion"]
                    .as_i64()
                    .is_none_or(|version| version < 0)
                {
                    return Err("Invalid expectedWorkVersion".into());
                }
                if let Some(plan) = args.get("recoveryPlan").filter(|v| !v.is_null()) {
                    let object = plan.as_object().ok_or("Invalid recoveryPlan")?;
                    if object.len() != 4 || !["failedTaskId", "cause", "change", "verification"].iter().all(|key|
                        object.get(*key).and_then(Value::as_str).is_some_and(|text| !text.trim().is_empty()
                            && text.chars().count() <= if *key == "failedTaskId" { 160 } else { 600 })) {
                        return Err("Invalid recoveryPlan".into());
                    }
                }
                "work-decision/actions"
            } else {
                if args["targetChapters"]
                    .as_i64()
                    .is_none_or(|count| !(1..=100).contains(&count))
                    || args["expectedIntentRevision"]
                        .as_i64()
                        .is_none_or(|revision| revision < 0)
                {
                    return Err("Invalid generation proposal scope".into());
                }
                "generation-handoff"
            };
            let mut body = args.clone();
            body.as_object_mut()
                .ok_or("Arguments must be an object")?
                .remove("projectId");
            Ok(("POST", format!("{base}/{suffix}"), Some(body)))
        }
        _ => Err("Unknown task source tool".into()),
    }
}

fn validate_proposal(proposal: &Value) -> Result<(), String> {
    let object = proposal.as_object().ok_or("Invalid proposal")?;
    if object.len() != 2 || !matches!(proposal["operation"].as_str(),
        Some("generateChapter" | "generateBrief" | "generateOutline" | "generatePlan" |
            "generatePropertyRules" | "reviseCommitment" | "modelKnowledge" |
            "extractCommitments" | "assessSceneOutcome" | "assessRevisionOutcome")) {
        return Err("Invalid proposal operation".into());
    }
    let target = proposal["target"].as_object().ok_or("Invalid proposal target")?;
    if target.keys().any(|key| !["chapterId", "sourceTaskId", "scope"].contains(&key.as_str())) {
        return Err("Proposal target must only locate an existing authorized source".into());
    }
    for field in ["chapterId", "sourceTaskId"] {
        if target.get(field).filter(|v| !v.is_null()).is_some_and(|v|
            v.as_str().is_none_or(|s| s.trim().is_empty() || s.len() > 160)) {
            return Err(format!("Invalid proposal target {field}"));
        }
    }
    if let Some(scope) = target.get("scope").filter(|v| !v.is_null()) {
        let scope_object = scope.as_object().ok_or("Invalid proposal scope")?;
        let allowed: &[&str] = match scope["type"].as_str() {
            Some("wholeChapter") => &["type"],
            Some("throughScene") if scope["endSceneOrder"].as_u64().is_some_and(|n| n > 0) => &["type", "endSceneOrder"],
            Some("selectedScenes") if scope["groupIds"].as_array().is_some_and(|ids| !ids.is_empty() && ids.len() <= 16
                && ids.iter().all(|id| id.as_str().is_some_and(|id| !id.trim().is_empty() && id.len() <= 160))) => &["type", "groupIds", "sourceRevisionId"],
            _ => return Err("Invalid proposal scope".into()),
        };
        if scope_object.keys().any(|key| !allowed.contains(&key.as_str()))
            || scope.get("sourceRevisionId").is_some_and(|v| v.as_str().is_none_or(|s| s.trim().is_empty() || s.len() > 160)) {
            return Err("Invalid proposal scope fields".into());
        }
    }
    Ok(())
}

pub(crate) fn context_budget(args: &Value, field: &str) -> Result<u64, String> {
    let (default, minimum, maximum) = match field {
        "budget" => (24, 1, 100_000),
        "byteBudget" => (24_000, 4_000, 60_000),
        _ => return Err("Unknown context budget".into()),
    };
    match args.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .filter(|value| (minimum..=maximum).contains(value))
            .ok_or_else(|| format!("Invalid {field}")),
    }
}

pub(crate) fn specs() -> Vec<Value> {
    [
        ("read_task_source","Read only a sourceId returned by this task's taskSources directory. Each page delivers at most 2000 UTF-8 payload bytes and 6500 total bytes under the unchanged model-item cap. For the first page use JSON null (not the string null or a numeric offset); then copy nextCursor exactly. Reassemble exact payloadPart using contiguous byteRange, check contentHash, and parse JSON after all needed pages. The cached projection retains this task's original source choices and visibility: no arbitrary path, revision, chapter or POV override. Read original material and all protected/unselected scenes before changing it; otherwise read only what the author's question needs. A suffix ending is not proof of prior reading or semantic completeness; unread material is unknown.",vec!["sourceId","cursor"]),
        ("read_work_status","Read this task's authoritative Work decision, exact required inputs and readyActions. No operation, target or permission is selected by the caller. canFinish describes persisted dependencies, not semantic author-goal completion. Read small pages using pagination.nextCursor until null when more evidence is needed; cursor binds this decision, Work version and source digest. Restart with null if stale. Missing coverage is uncertainty, not absence.",vec![]),
        ("request_work_action","Request exactly one of actionKey from read_work_status or proposal {operation,target}, using the exact version/sourceDigest. A proposal locates existing authorized content: generateChapter needs chapterId and optional narrower scope; artifact revisions need sourceTaskId from this Work; knowledge, extraction and assessments need chapterId. Native derives current source revisions and all execution authority. Generation can revise only its current chapter; selectedScenes preserves every unselected group. Never propose new chapters, deletion, publication or canon changes. Native derives operation and target from the original author authorization; instruction refines execution without new permission. Register only, then complete this decision so Host can execute it. For repeated failure, include recoveryPlan bound to readyAction.recovery.failedTaskId explaining cause, changed approach and verification. Do not repeat a failed correction with unchanged sources. Never call request_candidate_task in a Work decision or invent source identifiers, runtime fields or permissions. Reuse an idempotencyKey only for an identical request.",vec!["instruction","idempotencyKey","expectedWorkVersion","expectedSourceDigest"]),
        ("request_generation_handoff","Propose new chapter creation from the author's actual conversation goal, targetChapters 1-100. The author approves the total scope; decide planning and analysis incrementally, do not ask again every three chapters. This records a proposal only: no chapter, task or budget is created. Host asks the author to approve this exact scope and budget before a GenerationRun starts. Never invent chapter IDs or claim the proposal saved prose. Reuse idempotencyKey only for an identical proposal.",vec!["targetChapters","instruction","expectedIntentRevision","idempotencyKey"]),
    ].into_iter().map(|(name,description,fields)| {
        let mut properties=json!({"projectId":{"type":"string","minLength":1,"maxLength":160},"taskId":{"type":"string","minLength":1,"maxLength":160},"threadId":{"type":"string","minLength":1,"maxLength":160},"expectedRevision":{"type":"integer","minimum":0},"sourceId":{"type":"string","minLength":71,"maxLength":71},"cursor":{"type":["string","null"],"description":"First page: JSON null, without quotes. Later pages: copy the exact opaque nextCursor returned by this same source. Never use the strings null or 0, or calculate offsets.","minLength":1,"maxLength":100},"actionKey":{"type":"string","minLength":1,"maxLength":160},"instruction":{"type":"string","minLength":1,"maxLength":1000},"idempotencyKey":{"type":"string","minLength":1,"maxLength":160},"expectedWorkVersion":{"type":"integer","minimum":0},"expectedSourceDigest":{"type":"string","minLength":1,"maxLength":160}});
        properties["targetChapters"]=json!({"type":"integer","minimum":1,"maximum":100});
        properties["expectedIntentRevision"]=json!({"type":"integer","minimum":0});
        properties["recoveryPlan"]=json!({"type":["object","null"],"additionalProperties":false,"required":["failedTaskId","cause","change","verification"],"properties":{"failedTaskId":{"type":"string","minLength":1,"maxLength":160},"cause":{"type":"string","minLength":1,"maxLength":600},"change":{"type":"string","minLength":1,"maxLength":600},"verification":{"type":"string","minLength":1,"maxLength":600}}});
        properties["proposal"]=json!({"type":"object","additionalProperties":false,"required":["operation","target"],"properties":{
            "operation":{"type":"string","enum":["generateChapter","generateBrief","generateOutline","generatePlan","generatePropertyRules","reviseCommitment","modelKnowledge","extractCommitments","assessSceneOutcome","assessRevisionOutcome"]},
            "target":{"type":"object","additionalProperties":false,"properties":{
                "chapterId":{"type":"string","minLength":1,"maxLength":160},"sourceTaskId":{"type":"string","minLength":1,"maxLength":160},
                "scope":{"type":"object","additionalProperties":false,"required":["type"],"properties":{
                    "type":{"type":"string","enum":["wholeChapter","selectedScenes","throughScene"]},
                    "sourceRevisionId":{"type":"string","minLength":1,"maxLength":160},
                    "groupIds":{"type":"array","minItems":1,"maxItems":16,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":160}},
                    "endSceneOrder":{"type":"integer","minimum":1}}}}}}});
        let required:Vec<_>=["projectId","taskId","threadId","expectedRevision"].into_iter().chain(fields).collect();
        properties.as_object_mut().expect("properties").retain(|key,_|required.contains(&key.as_str()) || (name == "read_work_status" && key == "cursor") || (name == "request_work_action" && ["actionKey", "proposal", "recoveryPlan"].contains(&key.as_str())));
        if name == "read_work_status" { properties["cursor"]["description"] = json!("Optional: omit or null for first page, then copy pagination.nextCursor from this same decision. A stale cursor must be rejected; restart with null."); }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties}})
    }).collect()
}

/// Preserve the Runtime rejection and give the agent an exact, source-bound
/// first-page request. Never coerce a bad cursor or execute a retry in the host.
pub(crate) fn add_read_recovery(name: &str, args: &Value, response: &mut (u16, Value)) {
    if name != "read_task_source"
        || response.0 != 400
        || !response.1["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message
                    .contains("Task source cursor does not match the frozen task and UTF-8 range")
            })
    {
        return;
    }
    let mut corrected = args.clone();
    corrected["cursor"] = Value::Null;
    response.1["error"]["details"]["field"] = json!("cursor");
    response.1["error"]["recovery"] = json!({
        "nextAction": "restart_source_page", "retryUnchanged": false,
        "instruction": "Use the corrected arguments below: cursor must be JSON null for the first page, not the string null or 0. Continue only with the exact nextCursor returned for this source; never guess or increment offsets.",
        "tool": "read_task_source", "arguments": corrected
    });
}
