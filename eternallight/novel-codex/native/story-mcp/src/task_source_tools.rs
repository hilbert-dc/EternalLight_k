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
        "read_source_content" => {
            let endpoint = if args["view"] == "raw" { "task-source" } else { "task-source-content" };
            let mut path = format!("{base}/{endpoint}?{binding}");
            match args.get("sourceId") {
                Some(Value::String(source)) if source.len() == 71 && source.starts_with("source-") => {
                    path.push_str(&format!("&sourceId={}", super::percent_encode(source)));
                }
                None if matches!(args["view"].as_str(), Some("workspace" | "passages")) => {},
                _ => return Err("sourceId is required except for view=workspace/passages; copy a current task source".into()),
            }
            if let Some(view) = args.get("view") {
                if !matches!(view.as_str(), Some("raw" | "content" | "index" | "workspace" | "locate" | "passages" | "scene"))
                {
                    return Err("view must be content, raw, index, workspace, passages, scene or locate".into());
                }
                if matches!(view.as_str(), Some("index" | "workspace" | "locate" | "passages" | "scene")) { path.push_str(&format!("&view={}", view.as_str().unwrap())); }
            }
            if args["view"] == "passages" {
                let edge = args["edge"].as_str().filter(|v| matches!(*v, "start" | "end")).ok_or("passages requires edge=start/end")?;
                let count = args["characters"].as_u64().filter(|n| (1..=512).contains(n)).ok_or("passages requires characters=1..512")?;
                if args.get("objectKind").is_some_and(|v| v != "draft") { return Err("passages reads only frozen draft sources".into()); }
                path.push_str(&format!("&edge={edge}&characters={count}"));
            } else if args.get("edge").is_some() || args.get("characters").is_some() {
                return Err("edge/characters require view=passages".into());
            }
            if args["view"] == "locate" && args.get("quote").is_none() { return Err("locate requires quote".into()); }
            if args["view"] == "scene" && args.get("groupId").is_none() { return Err("scene requires groupId".into()); }
            for key in ["quote", "groupId"] {
                if let Some(value) = args.get(key) {
                    if args["view"] != "locate" && !(key == "groupId" && args["view"] == "scene") { return Err("quote requires locate; groupId requires locate or scene".into()); }
                    let value = value.as_str().ok_or("locate fields must be strings")?;
                    if value.is_empty() || (key == "quote" && value.chars().count() > 120) || (key == "groupId" && value.len() > 160) { return Err("invalid locate field".into()); }
                    path.push_str(&format!("&{key}={}", super::percent_encode(value)));
                }
            }
            for key in ["chapterNumber", "objectKind", "query", "revisionId"] {
                if let Some(value) = args.get(key) {
                    if !matches!(args["view"].as_str(), Some("workspace" | "passages")) {
                        return Err("Workspace filters require view=workspace/passages".into());
                    }
                    let field = match key {
                        "chapterNumber" => value
                            .as_i64()
                            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
                            .map(|n| n.to_string()),
                        "revisionId" => value.as_str().filter(|id| !id.is_empty() && id.len() <= 160).map(str::to_owned),
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
                    "Omit cursor for the first page; otherwise copy the complete nextRead.arguments from this source"
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
        ("read_source_content","For chapter or revision lookup, use view=workspace without sourceId: Native binds the current task. Filter chapterNumber/objectKind/revisionId and read the returned source. For batched boundary text use view=passages without sourceId, edge=start/end and characters=1..512: exact partial prose from frozen visible chapters, with versioned coordinates, never complete-source credit. For other views copy this task's sourceId; never reuse a sourceId or cursor from another task. Read bounded parsed fields from an authorized taskSources sourceId. textRepresentation=sceneGroups omits only the exactly reconstructible aggregate body; read group bodies in order with original whitespace and metadata. Use view=index to locate records by actual chapter identity; use the returned read request and follow nextRead until null to read only the chosen record. Its boundary is preserved across pages. Index metadata is not reading. Runtime handles immutable source binding. The original input sourceIds remain frozen; view=workspace also discovers Native-saved checkpoints from this Task and admitted earlier producers in the same Work. Read their returned sourceId, never an arbitrary revision. savedCheckpoint is immutable reading/comparison evidence; isCurrentAtListing observes only the candidate head when listed, not adoption, independent acceptance or delivery. entries use JSON-pointer paths; long strings include exact characterRange. Prose entries carry editAnchor with immutable chapter/source/group identity and Unicode scalar start/end; use entry text/value as patch oldText. Anchors do not grant write authority; preserve locks, scope and candidate CAS. Omit cursor for the first page. For later pages invoke the complete returned nextRead request until null; preserve its filters and cursor. Read only sources needed for your question, all protected content before editing. Complete requires all pages from the start; a suffix is not complete reading. read_context.taskSources.entries.returnedCoverage durably records only actual parsed entry ranges returned, not semantic understanding. returnedChapterNumbers refers only to this page; chapterNumbers is catalog scope. Index access does not update reading progress. Raw pages retain their own byte coverage; do not mix raw and parsed cursors. Empty/missing results are uncertainty, never proof of absent facts. Prefer this to manual JSON-fragment reconstruction.",vec![]),
        ("read_work_status","Frozen chapter standards are returned as chapterAcceptanceSource; use read_source_content to read the full value before review or revision. verifyChapter ready actions include currentDraftSource.read: copy this exact request and follow nextRead to read the current saved draft without a directory search. Locators do not certify reading. Read task state before choosing an action. Ordinary advisory returns mode=conversation with same-conversation existing runs; bound Work decisions return exact required inputs and readyActions. For author-requested continuation after prerequisite edits, explicitly read view=conversationRuns to select the original run; omit cursor for this view. No operation, target or permission is selected by the caller. canFinish describes persisted dependencies, not semantic author-goal completion. Read small pages using pagination.nextCursor until null when more evidence is needed; cursor binds this decision, Work version and source digest. Restart with null if stale. Missing coverage is uncertainty, not absence.",vec![]),
        ("request_work_action","Success includes decisionStatus, the same current first page as read_work_status, in the action transaction. Use its version/sourceDigest, actions and deliveryReviewRequired directly; query again for concurrent changes, recovery or further pages. Request exactly one of actionKey from read_work_status or proposal {operation,target}, using the exact version/sourceDigest. A proposal locates existing authorized content: generateChapter needs chapterId and optional narrower scope; artifacts need sourceTaskId from this Work. A new planning artifact is allowed only for an existing approved planning target: copy its readyAction.actionKey or sourceTaskId; a prior same-operation Task is unnecessary. A Brief-only authorization never permits Outline or Plan. Native refreshes current inputs while preserving the exact approved target, pending delivery and same Work allowance. Knowledge, extraction and assessments need chapterId. Native derives current source revisions and all execution authority. Generation can revise only its current chapter; selectedScenes preserves every unselected group; add bodyRanges [{groupId,start,end}] plus the exact sourceRevisionId to narrow to text, using Unicode character offsets. Reading scope is separate; this cannot widen a frozen readScope. Never propose new chapters, deletion, publication or canon changes. Native derives operation and target from the original author authorization; instruction refines execution without new permission. Choose chapterAcceptance.verification={method:direct|independent,reason} from the author request; direct permits root verification without a separate critic, while independent is mandatory when explicitly required. After saving, a verifyChapter ready action accepts chapterReview={outcomes,uncoveredRequirements} using the same judgments/evidence/issue shape as submit_revision_outcome; Native binds writer and revision IDs. First call read_context for THIS task: never reuse a previous task sourceId/cursor. Read authorRequirementsSource and the complete current draft: this task’s relevantText qualifies only when it contains the exact full current revision; otherwise use the workspace directory and returned read.arguments. Draft receipt metadata alone is not a full draft read. read_context.authorRequirementsSource.rootReadComplete reports actual root transport coverage, not semantic understanding. For exact citations, read_source_content(view=locate) returns a ready evidence object; copy it without editing quotes or offsets. Evidence quotes must be verbatim sceneGroup prose, never metadata such as nativeSavedCheckpoint. Every supported/unmet criterion requires nonempty exact prose evidence. Freeze prose-verifiable criteria only; candidate saving and adoption/publication limits remain Native lifecycle constraints and must be reported from receipts, never converted into invented prose criteria or quotes. It saves a report in the current decision without starting another Task: use the returned decisionStatus to decide next, do not complete merely because verification was submitted. Negative findings are valid reports and must be repaired. For the first writer choice (writeNextChapter/generateChapter) of a chapter and author-intent generation, include chapterAcceptance {requirements:[{id,text}],length:{requirement,minCharacters,maxCharacters}} before drafting: use stable criterion identities and interpret original author intent, null bounds only for no restriction. For revisions in the same Work, chapter and author-intent generation, omit chapterAcceptance to reuse the Native frozen standards; do not copy them into every action request. Read the full frozen standards through read_work_status.chapterAcceptanceSource with read_source_content before checking or reusing chapterAcceptance; correcting an interpretation still needs actual original author evidence. finishChapter takes no chapterReview: Native binds the current saved assessment, all frozen criteria, actual length and unresolved problems transactionally. The model explicitly chooses to finish; no missing, stale or negative assessment can pass. Read read_work_issues to retain stable problem identity and actual observation references across revisions. When checks fail, use original author sources to decide whether prose needs revision or a model interpretation needs correction. A correction must carry interpretationCorrection evidence and does not certify the draft or relax explicit author requirements. Omit both fields for unrelated actions. Execution actions register work, then complete this decision so Host can execute it. For verifyChapter, use readyActions[].currentDraftSource.read for the exact saved draft; follow nextRead until complete. The locator itself grants no reading evidence. verifyChapter only records current evidence; use the returned decisionStatus and continue this decision. When readyAction.recovery.planRequired is true, including the first provider_output_limit failure, include recoveryPlan bound to readyAction.recovery.failedTaskId explaining cause, changed approach and verification. Do not repeat a failed correction with unchanged sources. Never call request_candidate_task in a Work decision or invent source identifiers, runtime fields or permissions. Reuse an idempotencyKey only for an identical request.",vec!["instruction","idempotencyKey","expectedWorkVersion","expectedSourceDigest"]),
        ("read_revision_outcome","Read the actual direct or independent assessment report for a writerTaskId from this Work. Returned assessment is a frozen taskSources reference. Read it with read_source_content from this tool's taskSources directory; follow parsed pages to obtain every criterion, judgment, reason and exact before/after prose evidence. Never treat the reference as report content. A report is candidate analysis, not proof of passing; check current state, all judgments and actual length from read_work_status. Use unmet/insufficientEvidence and quoted prose to choose a targeted repair under the frozen chapterAcceptance, then reassess the latest saved writer. Do not infer quality from report existence.",vec!["writerTaskId"]),
        ("pause_generation_run","Interpret the author intent in context. Pause an existing authorized same-conversation run using runId and expectedVersion copied from read_work_status(view=conversationRuns), plus current expectedIntentRevision. This commits actual pause in the existing state transaction, retaining scope, saved results, allowance and unknown write/usage fences. Do not resume or cancel via this tool. Report paused only after receipt.status=paused; never announce a pause from prose alone. For your own active generation coordinator, pausing revokes the current attempt; end after the receipt instead of registering more work.",vec!["runId","expectedVersion","expectedIntentRevision","instruction","idempotencyKey"]),
        ("request_generation_handoff","Propose genuinely new chapter scope (1-100 total) for author approval, or select an explicitly intended existing run. For existing scope, read conversationRuns: resumeSelection.status=candidate means only paused/blocked state eligibility; notNeeded means run and Work are both running, so no handoff is needed; unavailable requires inspecting workStatus, not retrying handoff. Copy its id/expectedVersion into resumeRun and preserve targetChapters and the original allowance. Identity, intent, versions, prerequisite outputs and settlement are still checked. A non-generation Work decision may select continuation after requested edits/checks settle. This records a handoff, not execution or saved prose. New scope requires author approval; an existing authorized scope needs no duplicate approval. After registration, read_work_status and complete the task with its required deliveryReview; if a continuation goal exists, record the proposalId there. Fulfilled covers verified prerequisites and registered transfer, not future chapters. Choose from author intent in context, never keywords. Never invent chapter IDs or scope. Reuse idempotencyKey only for identical proposals.",vec!["targetChapters","instruction","expectedIntentRevision","idempotencyKey"]),
    ].into_iter().map(|(name,description,fields)| {
        let mut properties=json!({"projectId":{"type":"string","minLength":1,"maxLength":160},"taskId":{"type":"string","minLength":1,"maxLength":160},"threadId":{"type":"string","minLength":1,"maxLength":160},"expectedRevision":{"type":"integer","minimum":0},"sourceId":{"type":"string","minLength":71,"maxLength":71},"cursor":{"type":["string","null"],"description":"First page: JSON null, without quotes. Later pages: copy the exact opaque nextCursor returned by this same source. Never use the strings null or 0, or calculate offsets.","minLength":1,"maxLength":100},"actionKey":{"type":"string","minLength":1,"maxLength":160},"instruction":{"type":"string","minLength":1,"maxLength":1000},"idempotencyKey":{"type":"string","minLength":1,"maxLength":160},"expectedWorkVersion":{"type":"integer","minimum":0},"expectedSourceDigest":{"type":"string","minLength":1,"maxLength":160}});
        properties["chapterAcceptance"]=super::chapter_acceptance_schema::acceptance();
        let full_report=super::revision_outcome_schema::payload();
        let direct=json!({"type":"object","additionalProperties":false,"required":["outcomes","uncoveredRequirements"],"properties":{"outcomes":full_report["properties"]["outcomes"],"uncoveredRequirements":full_report["properties"]["uncoveredRequirements"]}});
        properties["chapterReview"]=direct;
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
        properties.as_object_mut().expect("properties").retain(|key,_|required.contains(&key.as_str()) || (name == "read_source_content" && key == "sourceId") || (name == "request_generation_handoff" && key == "resumeRun") || (matches!(name, "read_work_status" | "read_source_content") && key == "cursor") || (name == "request_work_action" && ["actionKey", "proposal", "recoveryPlan", "chapterAcceptance", "chapterReview"].contains(&key.as_str())));
        if name == "read_work_issues" { properties["cursor"]=json!({"type":"integer","minimum":0,"maximum":9007199254740991u64,"description":"0 for first page. Later copy the actual integer nextCursor; Native uses a keyset boundary, not a caller-computed offset."}); }
        if matches!(name, "read_source_content") { properties["cursor"]=json!({"type":"string","minLength":1,"maxLength":100,"pattern":"^[a-f0-9]{64}:[0-9]+(:[0-9]+)?$","description":"Omit this field for the first page. For later pages copy the complete returned nextRead.arguments, including all filters and this cursor. Never send null, the string null, first, or a numeric offset."}); }
        if name == "read_source_content" {
            properties["view"]=json!({"type":"string","enum":["content","raw","index","workspace","locate","passages","scene"],"description":"Use passages without sourceId with edge=start/end and characters=1..512 to batch exact boundary excerpts from frozen visible chapter prose; partial excerpts do not certify complete reading. Default content returns parsed fields. raw returns exact JSON UTF-8 fragments: assemble contiguous byteRange, verify contentHash, then parse the complete JSON. Copy nextRead to continue in the same view. Use locate with quote (and optional groupId) for exact prose matches and small edit anchors; multiple matches are not auto-selected, and matching does not certify full-source reading. Use workspace without sourceId: the current task is bound by Native. Locate admitted revisions by chapterNumber, objectKind, revisionId or query; no directory traversal is needed for a known chapter. The directory includes frozen inputs and authorized Native-saved checkpoints with producer/version/completeness and isCurrentAtListing. Current-at-listing observes candidate head only, not delivery or adoption. New saves invalidate directory cursors; immutable body cursors remain bound to their exact saved source. Then omit view/cursor and read the chosen item's sourceId; actual contentHash comes from that body read. Use index for records already inside one source. Use view=scene with sourceId and groupId from a prose anchor to read that entire visible scene, following nextRead; its pagination never crosses into another scene. Directory/index metadata is not content reading; every source's coverage is local, and child pages do not certify root reading."});
            properties["edge"]=json!({"type":"string","enum":["start","end"],"description":"Required only for passages: first/last non-empty visible prose group. A scope-clipped boundary is not necessarily the full chapter boundary."});
            properties["characters"]=json!({"type":"integer","minimum":1,"maximum":512,"description":"Required only for passages: maximum Unicode scalar characters per chapter excerpt. Exact original text with version and coordinates, not a summary or complete-source reading."});
            properties["revisionId"]=json!({"type":"string","minLength":1,"maxLength":160,"description":"Exact immutable revision filter, only with view=workspace/passages. Intersects authorized sources; never grants access to an arbitrary revision."});
            properties["quote"]=json!({"type":"string","minLength":1,"maxLength":120,"description":"Exact prose to locate, required only for view=locate. Returns all occurrences through nextRead; no fuzzy matching or write grant."});
            properties["groupId"]=json!({"type":"string","minLength":1,"maxLength":160,"description":"Exact group identity: optional filter for locate; required for scene to expand the complete visible scene with pagination. Copy from an original-text anchor; no full-source reading credit."});
            properties["chapterNumber"]=json!({"type":"integer","minimum":1,"maximum":9007199254740991u64,"description":"Optional exact chapter locator, only with view=workspace; never widens the task's Native visibility."});
            properties["objectKind"]=json!({"type":"string","enum":["draft","brief","outline","plan","propertyRules","revisionOutcome","sceneAssessment","commitmentExtraction","critique","commitment"],"description":"Optional admitted object kind, only with view=workspace/passages. savedReport is actual model analysis, never established prose or acceptance. Native report-store and producer scope remain authoritative; writer visibility cannot be widened by choosing a report kind."});
            properties["query"]=json!({"type":"string","minLength":1,"maxLength":120,"description":"Optional literal source locator, only with view=workspace/passages. Search scans at most 64 admitted records per page; invoke the complete nextRead request until null, preserving its query and filters. Writer search is visible metadata only; missing matches do not prove absence. Read actual text and decide meaning yourself."});
        }
        if name == "read_work_status" { properties["view"]=json!({"type":"string","enum":["conversationRuns"],"description":"Read original same-conversation generation runs after prerequisite outputs are saved; omit cursor. Default omitted view reads current Work requirements."}); properties["cursor"]["description"] = json!("Optional: omit or null for first page, then copy pagination.nextCursor from this same decision. A stale cursor must be rejected; restart with null."); }
        let mut tool = json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties}});
        if name == "read_source_content" {
            tool["inputSchema"]["anyOf"] = json!([{ "required":["sourceId","view","groupId"],"properties":{"view":{"const":"scene"}}}, { "required":["sourceId"],"properties":{"view":{"enum":["content","raw","index","locate"]}} }, {"required":["view"],"properties":{"view":{"const":"workspace"}}}, {"required":["view","edge","characters"],"properties":{"view":{"const":"passages"}}}]);
        }
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
