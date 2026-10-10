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
        "read_work_issues" => {
            let limit = args["limit"]
                .as_u64()
                .filter(|n| (1..=16).contains(n))
                .ok_or("limit must be an integer from 1 through 16")?;
            let cursor = args["cursor"]
                .as_u64()
                .filter(|n| *n <= 9_007_199_254_740_991)
                .ok_or("cursor must be zero initially or the actual nextCursor integer")?;
            let mut path = format!("{base}/work-issues?{binding}&cursor={cursor}&limit={limit}");
            match args.get("issueId") {
                Some(Value::Null) => {}
                Some(Value::String(value)) if !value.is_empty() && value.len() <= 160 => {
                    path.push_str(&format!("&issueId={}", super::percent_encode(value)));
                }
                _ => {
                    return Err(
                        "issueId must be explicit null or a copied bounded identifier".into(),
                    );
                }
            }
            Ok(("GET", path, None))
        }
        "read_task_source" | "read_source_content" => {
            let source = args["sourceId"]
                .as_str()
                .filter(|id| id.len() == 71 && id.starts_with("source-"))
                .ok_or("sourceId must be copied from taskSources")?;
            let endpoint = if name == "read_source_content" {
                "task-source-content"
            } else {
                "task-source"
            };
            let mut path = format!(
                "{base}/{endpoint}?{binding}&sourceId={}",
                super::percent_encode(source)
            );
            if let Some(view) = args.get("view") {
                if name != "read_source_content"
                    || !matches!(view.as_str(), Some("index" | "workspace"))
                {
                    return Err("Only read_source_content accepts view=index or workspace".into());
                }
                path.push_str(&format!("&view={}", view.as_str().unwrap()));
            }
            for key in ["chapterNumber", "objectKind", "query"] {
                if let Some(value) = args.get(key) {
                    if args["view"] != "workspace" {
                        return Err("Workspace filters require view=workspace".into());
                    }
                    let field = match key {
                        "chapterNumber" => value
                            .as_i64()
                            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
                            .map(|n| n.to_string()),
                        "objectKind" => value
                            .as_str()
                            .filter(|k| {
                                matches!(
                                    *k,
                                    "draft"
                                        | "brief"
                                        | "outline"
                                        | "plan"
                                        | "propertyRules"
                                        | "revisionOutcome"
                                        | "sceneAssessment"
                                        | "commitmentExtraction"
                                        | "critique"
                                        | "commitment"
                                )
                            })
                            .map(str::to_owned),
                        _ => value
                            .as_str()
                            .filter(|q| !q.trim().is_empty() && q.chars().count() <= 120)
                            .map(str::to_owned),
                    }
                    .ok_or_else(|| format!("Invalid workspace {key}"))?;
                    path.push_str(&format!("&{key}={}", super::percent_encode(&field)));
                }
            }
            match args.get("cursor") {
                None => {}
                Some(Value::String(cursor)) if !cursor.is_empty() && cursor.len() <= 100 => {
                    path.push_str(&format!("&cursor={}", super::percent_encode(cursor)))
                }
                _ => return Err(
                    "Omit cursor for the first page; otherwise copy nextCursor from this source"
                        .into(),
                ),
            }
            Ok(("GET", path, None))
        }
        "read_work_status" => {
            let mut path = format!("{base}/work-decision?{binding}");
            if let Some(view) = args.get("view") {
                if view != "conversationRuns" || !args["cursor"].is_null() {
                    return Err("Use conversationRuns without a Work cursor".into());
                }
                path.push_str("&view=conversationRuns");
            }
            match &args["cursor"] {
                Value::Null => {}
                Value::String(cursor) if !cursor.is_empty() && cursor.len() <= 100 => {
                    path.push_str(&format!("&cursor={}", super::percent_encode(cursor)))
                }
                _ => return Err("cursor must be null or the exact bounded nextCursor".into()),
            }
            Ok(("GET", path, None))
        }
        "read_revision_outcome" => {
            let writer = args["writerTaskId"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 160)
                .ok_or("Invalid writerTaskId")?;
            Ok((
                "GET",
                format!(
                    "{base}/revision-outcome?{binding}&writerTaskId={}",
                    super::percent_encode(writer)
                ),
                None,
            ))
        }
        "pause_generation_run" => {
            for field in ["runId", "instruction", "idempotencyKey"] {
                if args[field].as_str().is_none_or(|s| {
                    s.trim().is_empty()
                        || s.chars().count() > if field == "instruction" { 1000 } else { 160 }
                }) {
                    return Err(format!("Invalid {field}"));
                }
            }
            for field in ["expectedVersion", "expectedIntentRevision"] {
                if args[field].as_i64().is_none_or(|n| n < 0) {
                    return Err(format!("Invalid {field}"));
                }
            }
            Ok((
                "POST",
                format!("{base}/generation-control"),
                Some(args.clone()),
            ))
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
                if action.is_some_and(|v| {
                    v.as_str()
                        .is_none_or(|s| s.trim().is_empty() || s.len() > 160)
                }) {
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
                    if object.len() != 4
                        || !["failedTaskId", "cause", "change", "verification"]
                            .iter()
                            .all(|key| {
                                object
                                    .get(*key)
                                    .and_then(Value::as_str)
                                    .is_some_and(|text| {
                                        !text.trim().is_empty()
                                            && text.chars().count()
                                                <= if *key == "failedTaskId" { 160 } else { 600 }
                                    })
                            })
                    {
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
    if object.len() != 2
        || !matches!(
            proposal["operation"].as_str(),
            Some(
                "generateChapter"
                    | "generateBrief"
                    | "generateOutline"
                    | "generatePlan"
                    | "generatePropertyRules"
                    | "reviseCommitment"
                    | "modelKnowledge"
                    | "extractCommitments"
                    | "assessSceneOutcome"
                    | "assessRevisionOutcome"
            )
        )
    {
        return Err("Invalid proposal operation".into());
    }
    let target = proposal["target"]
        .as_object()
        .ok_or("Invalid proposal target")?;
    if target
        .keys()
        .any(|key| !["chapterId", "sourceTaskId", "scope"].contains(&key.as_str()))
    {
        return Err("Proposal target must only locate an existing authorized source".into());
    }
    for field in ["chapterId", "sourceTaskId"] {
        if target.get(field).filter(|v| !v.is_null()).is_some_and(|v| {
            v.as_str()
                .is_none_or(|s| s.trim().is_empty() || s.len() > 160)
        }) {
            return Err(format!("Invalid proposal target {field}"));
        }
    }
    if let Some(scope) = target.get("scope").filter(|v| !v.is_null()) {
        let scope_object = scope.as_object().ok_or("Invalid proposal scope")?;
        let allowed: &[&str] = match scope["type"].as_str() {
            Some("wholeChapter") => &["type"],
            Some("throughScene") if scope["endSceneOrder"].as_u64().is_some_and(|n| n > 0) => {
                &["type", "endSceneOrder"]
            }
            Some("selectedScenes")
                if scope["groupIds"].as_array().is_some_and(|ids| {
                    !ids.is_empty()
                        && ids.len() <= 16
                        && ids.iter().all(|id| {
                            id.as_str()
                                .is_some_and(|id| !id.trim().is_empty() && id.len() <= 160)
                        })
                }) =>
            {
                &["type", "groupIds", "sourceRevisionId", "bodyRanges"]
            }
            _ => return Err("Invalid proposal scope".into()),
        };
        if scope_object
            .keys()
            .any(|key| !allowed.contains(&key.as_str()))
            || scope.get("sourceRevisionId").is_some_and(|v| {
                v.as_str()
                    .is_none_or(|s| s.trim().is_empty() || s.len() > 160)
            })
        {
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
        ("read_work_issues","Read only this bound task's Native Work problems and their actual saved observations. Pass issueId:null and cursor:0 to list; copy a returned issueId to page its full observation history. limit is 1–16; copy the actual integer nextCursor, never calculate offsets. Stable identity is model-chosen, not text matching. Read original negative observations before linking a new current independent report's resolutionOf; stale or disputed findings are not resolved by deleting a method plan or inventing a new issue. Directory summaries are not prose or a completeness certificate. Native derives Work ownership and retains task/source visibility. Write selection does not restrict reading. Explicit readScope prefix, POV or time fences may prohibit whole-report summaries; use admitted source evidence when such a reading fence applies. Root coordinators, independent critics and whole-chapter writers still require the actual bound Native grant. A discovered schema or trusted delegated caller does not widen that grant. This does not write, adopt or complete anything.",vec!["issueId","cursor","limit"]),
        ("read_source_content","First obtain this task's taskSources directory from read_context, or from read_revision_outcome for its frozen report; never reuse sourceId or cursor from another task. Read bounded parsed fields from an authorized taskSources sourceId. Use view=index to locate records by actual chapter identity; omit view and copy readCursor to read the chosen record through entryRange.end. Index metadata is not reading. Runtime handles immutable source binding. The original input sourceIds remain frozen; view=workspace also discovers Native-saved checkpoints from this Task and admitted earlier producers in the same Work. Read their returned sourceId, never an arbitrary revision. savedCheckpoint is immutable reading/comparison evidence; isCurrentAtListing observes only the candidate head when listed, not adoption, independent acceptance or delivery. entries use JSON-pointer paths; long strings include exact characterRange. Omit cursor for the first page. Follow nextCursor verbatim for later pages, reading only sources needed for your question, all protected content before editing. Complete requires all pages from the start; a suffix is not complete reading. read_context.taskSources.entries.returnedCoverage durably records only actual parsed entry ranges returned, not semantic understanding. returnedChapterNumbers refers only to this page; chapterNumbers is catalog scope. Index/raw access does not update parsed reading progress. Empty/missing results are uncertainty, never proof of absent facts. Prefer this to manual JSON-fragment reconstruction.",vec!["sourceId"]),
        ("read_task_source","First obtain this task's taskSources directory from read_context or read_revision_outcome, then read only a sourceId returned there. Never reuse a prior task's sourceId or cursor. Each page delivers at most 2000 UTF-8 payload bytes and 6500 total bytes under the unchanged model-item cap. For the first page omit cursor entirely; then copy nextCursor exactly. Reassemble exact payloadPart using contiguous byteRange, check contentHash, and parse JSON after all needed pages. Original input sourceIds retain this task's original source choices and visibility. The same authorized workspace may separately list actual savedCheckpoint versions; their immutable content is readable here only via its returned sourceId, with no arbitrary path, revision, chapter or POV override. Historical output is comparison/accounting evidence, never latest-target acceptance. Read original material and all protected/unselected scenes before changing it; otherwise read only what the author's question needs. A suffix ending is not proof of prior reading or semantic completeness; unread material is unknown.",vec!["sourceId"]),
        ("read_work_status","Read task state before choosing an action. Ordinary advisory returns mode=conversation with same-conversation existing runs; bound Work decisions return exact required inputs and readyActions. For author-requested continuation after prerequisite edits, explicitly read view=conversationRuns to select the original run; omit cursor for this view. No operation, target or permission is selected by the caller. canFinish describes persisted dependencies, not semantic author-goal completion. Read small pages using pagination.nextCursor until null when more evidence is needed; cursor binds this decision, Work version and source digest. Restart with null if stale. Missing coverage is uncertainty, not absence.",vec![]),
        ("request_work_action","Request exactly one of actionKey from read_work_status or proposal {operation,target}, using the exact version/sourceDigest. A proposal locates existing authorized content: generateChapter needs chapterId and optional narrower scope; artifacts need sourceTaskId from this Work. A new planning artifact is allowed only for an existing approved planning target: copy its readyAction.actionKey or sourceTaskId; a prior same-operation Task is unnecessary. A Brief-only authorization never permits Outline or Plan. Native refreshes current inputs while preserving the exact approved target, pending delivery and same Work allowance. Knowledge, extraction and assessments need chapterId. Native derives current source revisions and all execution authority. Generation can revise only its current chapter; selectedScenes preserves every unselected group; add bodyRanges [{groupId,start,end}] plus the exact sourceRevisionId to narrow to text, using Unicode character offsets. Reading scope is separate; this cannot widen a frozen readScope. Never propose new chapters, deletion, publication or canon changes. Native derives operation and target from the original author authorization; instruction refines execution without new permission. Choose chapterAcceptance.verification={method:direct|independent,reason} from the author request; direct permits root verification without a separate critic, while independent is mandatory when explicitly required. After saving, a verifyChapter ready action accepts chapterReview={outcomes,uncoveredRequirements} using the same judgments/evidence/issue shape as submit_revision_outcome; Native binds writer and revision IDs. First read authorRequirementsSource and the complete current draft through this root task workspace. It saves a report in the current decision without starting another Task: re-read Work status and decide next, do not complete merely because verification was submitted. Negative findings are valid reports and must be repaired. For all writer choices (writeNextChapter/generateChapter), include chapterAcceptance {requirements:[{id,text}],length:{requirement,minCharacters,maxCharacters}} before drafting: use stable criterion identities and interpret original author intent, null bounds only for no restriction. Revisions reuse read_work_status.chapterAcceptance; correcting an interpretation still needs actual original author evidence. finishChapter requires chapterReview {sourceRevisionId,assessmentRevisionId,checks:[{criterionId,issueId}],length}: copy every frozen criterion ID and length, reference the actual current assessment report and its issueId or null. Do not resubmit a duplicate self-assessment, status or sourceChecks. The saved direct or independent report owns judgments and exact source evidence; Native rejects stale reports, missing or unsupported criteria and unresolved problems. Read read_work_issues to retain stable problem identity and actual observation references across revisions. When checks fail, use original author sources to decide whether prose needs revision or a model interpretation needs correction. A correction must carry interpretationCorrection evidence and does not certify the draft or relax explicit author requirements. Omit both fields for unrelated actions. Execution actions register work, then complete this decision so Host can execute it. verifyChapter only records current evidence; re-read status and continue this decision. When readyAction.recovery.planRequired is true, including the first provider_output_limit failure, include recoveryPlan bound to readyAction.recovery.failedTaskId explaining cause, changed approach and verification. Do not repeat a failed correction with unchanged sources. Never call request_candidate_task in a Work decision or invent source identifiers, runtime fields or permissions. Reuse an idempotencyKey only for an identical request.",vec!["instruction","idempotencyKey","expectedWorkVersion","expectedSourceDigest"]),
        ("read_revision_outcome","Read the actual direct or independent assessment report for a writerTaskId from this Work. Returned assessment is a frozen taskSources reference. Read it with read_source_content from this tool's taskSources directory; follow parsed pages to obtain every criterion, judgment, reason and exact before/after prose evidence. Never treat the reference as report content. A report is candidate analysis, not proof of passing; check current state, all judgments and actual length from read_work_status. Use unmet/insufficientEvidence and quoted prose to choose a targeted repair under the frozen chapterAcceptance, then reassess the latest saved writer. Do not infer quality from report existence.",vec!["writerTaskId"]),
        ("pause_generation_run","Interpret the author intent in context. Pause an existing authorized same-conversation run using runId and expectedVersion copied from read_work_status(view=conversationRuns), plus current expectedIntentRevision. This commits actual pause in the existing state transaction, retaining scope, saved results, allowance and unknown write/usage fences. Do not resume or cancel via this tool. Report paused only after receipt.status=paused; never announce a pause from prose alone. For your own active generation coordinator, pausing revokes the current attempt; end after the receipt instead of registering more work.",vec!["runId","expectedVersion","expectedIntentRevision","instruction","idempotencyKey"]),
        ("request_generation_handoff","Choose from author intent and conversation context, never from keywords. For resuming an existing authorized run, provide resumeRun {id,expectedVersion} copied from conversationRuns and targetChapters equal to its original total; this resumes the original allowance without requesting a new range. Otherwise propose genuinely new chapter creation, targetChapters 1-100. The author approves the total scope; decide planning and analysis incrementally, do not ask again every three chapters. This records a proposal only: no chapter, task or budget is created. For new scope only, Host asks the author to approve the exact scope and budget before starting a GenerationRun. A resumeRun selection retains its existing author requirements, scope and allowance; use separate admitted actions for requested edits or prerequisite checks before resuming. After those outputs settle, a non-generation Work decision may select the original run from read_work_status(view=conversationRuns). Keep continuation goal active with prerequisites satisfied, then complete this decision with an honest deliveryReview so Host can resume; do not claim the original novel is finished. Never invent chapter IDs or claim the proposal saved prose. Reuse idempotencyKey only for an identical proposal.",vec!["targetChapters","instruction","expectedIntentRevision","idempotencyKey"]),
    ].into_iter().map(|(name,description,fields)| {
        let mut properties=json!({"projectId":{"type":"string","minLength":1,"maxLength":160},"taskId":{"type":"string","minLength":1,"maxLength":160},"threadId":{"type":"string","minLength":1,"maxLength":160},"expectedRevision":{"type":"integer","minimum":0},"sourceId":{"type":"string","minLength":71,"maxLength":71},"cursor":{"type":["string","null"],"description":"First page: JSON null, without quotes. Later pages: copy the exact opaque nextCursor returned by this same source. Never use the strings null or 0, or calculate offsets.","minLength":1,"maxLength":100},"actionKey":{"type":"string","minLength":1,"maxLength":160},"instruction":{"type":"string","minLength":1,"maxLength":1000},"idempotencyKey":{"type":"string","minLength":1,"maxLength":160},"expectedWorkVersion":{"type":"integer","minimum":0},"expectedSourceDigest":{"type":"string","minLength":1,"maxLength":160}});
        properties["chapterAcceptance"]=super::chapter_acceptance_schema::acceptance();
        let full_report=super::revision_outcome_schema::payload();
        let direct=json!({"type":"object","additionalProperties":false,"required":["outcomes","uncoveredRequirements"],"properties":{"outcomes":full_report["properties"]["outcomes"],"uncoveredRequirements":full_report["properties"]["uncoveredRequirements"]}});
        properties["chapterReview"]=json!({"anyOf":[super::chapter_acceptance_schema::review(),direct]});
        properties["issueId"]=super::work_issue_schema::nullable_issue_id();
        properties["limit"]=json!({"type":"integer","minimum":1,"maximum":16});
        properties["resumeRun"]=json!({"type":"object","additionalProperties":false,"required":["id","expectedVersion"],"properties":{"id":{"type":"string","minLength":1,"maxLength":160},"expectedVersion":{"type":"integer","minimum":0}}});
        properties["writerTaskId"]=json!({"type":"string","minLength":1,"maxLength":160});
        properties["runId"]=json!({"type":"string","minLength":1,"maxLength":160});
        properties["expectedVersion"]=json!({"type":"integer","minimum":0});
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
                    "bodyRanges":{"type":"array","minItems":1,"maxItems":16,"description":"Optional precise write authority: exactly one Unicode [start,end) range per selected group, bound to sourceRevisionId. Text outside these ranges must remain byte-for-byte unchanged.","items":{"type":"object","additionalProperties":false,"required":["groupId","start","end"],"properties":{"groupId":{"type":"string","minLength":1,"maxLength":160},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0}}}},
                    "endSceneOrder":{"type":"integer","minimum":1}}}}}}});
        let required:Vec<_>=["projectId","taskId","threadId","expectedRevision"].into_iter().chain(fields).collect();
        properties.as_object_mut().expect("properties").retain(|key,_|required.contains(&key.as_str()) || (name == "request_generation_handoff" && key == "resumeRun") || (matches!(name, "read_work_status" | "read_source_content" | "read_task_source") && key == "cursor") || (name == "request_work_action" && ["actionKey", "proposal", "recoveryPlan", "chapterAcceptance", "chapterReview"].contains(&key.as_str())));
        if name == "read_work_issues" { properties["cursor"]=json!({"type":"integer","minimum":0,"maximum":9007199254740991u64,"description":"0 for first page. Later copy the actual integer nextCursor; Native uses a keyset boundary, not a caller-computed offset."}); }
        if matches!(name, "read_source_content" | "read_task_source") { properties["cursor"]=json!({"type":"string","minLength":1,"maxLength":100,"pattern":"^[a-f0-9]{64}:[0-9]+$","description":"Omit this field for the first page. For later pages copy the exact nextCursor from this same source and tool. Never send null, the string null, first, or a numeric offset."}); }
        if name == "read_source_content" {
            properties["view"]=json!({"type":"string","enum":["index","workspace"],"description":"Use workspace with read_context.taskWorkspace.sourceId to page/search admitted revision references by question, chapterNumber or objectKind. The directory includes frozen inputs and authorized Native-saved checkpoints with producer/version/completeness and isCurrentAtListing. Current-at-listing observes candidate head only, not delivery or adoption. New saves invalidate directory cursors; immutable body cursors remain bound to their exact saved source. Then omit view/cursor and read the chosen item's sourceId; actual contentHash comes from that body read. Use index for records already inside one source. Directory/index metadata is not content reading; every source's coverage is local, and child pages do not certify root reading."});
            properties["chapterNumber"]=json!({"type":"integer","minimum":1,"maximum":9007199254740991u64,"description":"Optional exact chapter locator, only with view=workspace; never widens the task's Native visibility."});
            properties["objectKind"]=json!({"type":"string","enum":["draft","brief","outline","plan","propertyRules","revisionOutcome","sceneAssessment","commitmentExtraction","critique","commitment"],"description":"Optional admitted object kind, only with view=workspace. savedReport is actual model analysis, never established prose or acceptance. Native report-store and producer scope remain authoritative; writer visibility cannot be widened by choosing a report kind."});
            properties["query"]=json!({"type":"string","minLength":1,"maxLength":120,"description":"Optional literal source locator, only with view=workspace. Search scans at most 64 admitted records per page; follow nextCursor if needed. Writer search is visible metadata only; missing matches do not prove absence. Read actual text and decide meaning yourself."});
        }
        if name == "read_work_status" { properties["view"]=json!({"type":"string","enum":["conversationRuns"],"description":"Read original same-conversation generation runs after prerequisite outputs are saved; omit cursor. Default omitted view reads current Work requirements."}); properties["cursor"]["description"] = json!("Optional: omit or null for first page, then copy pagination.nextCursor from this same decision. A stale cursor must be rejected; restart with null."); }
        let mut tool = json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties}});
        if name == "request_work_action" {
            tool["inputSchema"]["oneOf"] = json!([
                {"required":["actionKey"]},
                {"required":["proposal"]}
            ]);
            tool["inputSchema"]["description"] = json!("Provide exactly one of actionKey or proposal and omit the other. Use actionKey from an existing readyAction to preserve its frozen target. Use proposal only for a new action within the existing approved author scope.");
        }
        tool
    }).collect()
}
