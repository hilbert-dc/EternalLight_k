//! Derives requirements from exact saved outputs; optional checks are choices, not a pipeline.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};


pub(crate) struct ReadyAction {
    pub key: String,
    pub operation: String,
    pub target: Value,
    pub source_task: Option<String>,
    pub source_revision: Option<String>,
    pub required: bool,
    pub recovery: Option<Value>,
    pub author_action_id: Option<String>,
}

pub(crate) fn action(
    operation: &str,
    target: Value,
    source: Option<String>,
    revision: Option<String>,
    required: bool,
) -> ReadyAction {
    let key = format!(
        "action-{:x}",
        Sha256::digest(json!([operation, target, revision]).to_string())
    );
    ReadyAction {
        key,
        operation: operation.into(),
        target,
        source_task: source,
        source_revision: revision,
        required,
        recovery: None,
        author_action_id: None,
    }
}

pub(crate) fn retryable(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    let record = crate::tasks::load_task(db, task)?;
    Ok(crate::conversation_receipts::settled_failure_without_writes(db, task)?
        && crate::conversation_receipts::failure_code(record.error.as_deref()).is_some())
}

pub(crate) fn actions(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<(Vec<Value>, Vec<ReadyAction>), RepositoryError> {
    let mut statement=db.prepare("SELECT t.task_id,t.target_json,o.revision_id,c.chapter_id,c.number FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.attempt_id=s.current_attempt_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id AND t.status='completed' AND t.operation='generateChapter' JOIN story_task_outputs_v2 o ON o.task_id=t.task_id AND o.output_kind='draft' AND o.completeness='chapterComplete' JOIN story_chapters_v2 c ON c.project_id=t.project_id AND c.chapter_id=json_extract(t.target_json,'$.chapterId') WHERE s.work_task_id=?1 AND t.project_id=?2 AND o.ordinal=(SELECT MAX(ordinal) FROM story_task_outputs_v2 WHERE task_id=t.task_id AND output_kind='draft') AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 newer JOIN story_tasks_v2 nt ON nt.task_id=newer.current_attempt_id WHERE newer.work_task_id=s.work_task_id AND newer.ordinal>s.ordinal AND nt.status='completed' AND nt.operation='generateChapter' AND json_extract(nt.target_json,'$.chapterId')=c.chapter_id) ORDER BY s.ordinal").map_err(storage_error)?;
    let rows = statement
        .query_map(params![work, project.as_str()], |r| {
            Ok((
                r.get::<_, String>(/*idx*/ 0)?,
                r.get::<_, String>(/*idx*/ 1)?,
                r.get::<_, String>(/*idx*/ 2)?,
                r.get::<_, String>(/*idx*/ 3)?,
                r.get::<_, i64>(/*idx*/ 4)?,
            ))
        })
        .map_err(storage_error)?;
    let mut requirements = Vec::new();
    // Explicit requested candidates remain part of the goal even when no
    // child attempt was created (e.g. a later approval failed).
    let mut pending = db.prepare(
        "SELECT a.action_id,a.operation FROM story_task_action_requests_v1 a
         JOIN story_work_attempts_v1 p ON p.attempt_id=a.source_task_id
         JOIN story_work_steps_v1 s ON s.id=p.step_id
         WHERE s.work_task_id=?1 AND a.project_id=?2 AND a.status IN ('proposed','queued')
           AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 child JOIN story_tasks_v2 t ON t.task_id=child.current_attempt_id
               WHERE child.id=a.action_id AND child.work_task_id=s.work_task_id AND t.project_id=a.project_id
                 AND t.status IN ('completed','failed','cancelled'))
         ORDER BY a.action_id",
    ).map_err(storage_error)?;
    for row in pending
        .query_map(params![work, project.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?
    {
        let (id, operation) = row.map_err(storage_error)?;
        requirements.push(json!({"kind":"requestedOutput","actionId":id,"operation":operation,"status":"pending"}));
    }
    let mut available = Vec::new();
    for approved in crate::task_actions::pending_planning_actions(db, project, work)? {
        let mut candidate = action(
            &approved.operation,
            approved.target,
            Some(approved.source_task_id),
            None,
            true,
        );
        candidate.key = format!("planning-{}", approved.id);
        candidate.author_action_id = Some(approved.id);
        available.push(candidate);
    }
    let replanned = crate::work_decision_generation::replanned_current_chapter(db, project, work)?;
    for row in rows {
        let (writer, encoded, revision, chapter, number) = row.map_err(storage_error)?;
        let target: Value =
            serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
        if let Some(run) = target["runId"]
            .as_str()
            .filter(|run| work == format!("work-{run}"))
        {
            let delivered:Option<i64>=db.query_row("SELECT json_extract(state_json,'$.completedChapters') FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",params![project.as_str(),run],|row|row.get(0)).optional().map_err(storage_error)?;
            if delivered.is_some_and(|through| number <= through) {
                continue;
            }
        }
        if replanned.as_ref().is_some_and(|(current, generation)| {
            current == &chapter
                && target["runId"]
                    .as_str()
                    .is_some_and(|run| work == format!("work-{run}"))
                && target["intentGeneration"]
                    .as_i64()
                    .is_some_and(|old| old < *generation)
        }) {
            continue;
        }
        let selected:Option<String>=db.query_row("SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_object_heads_v2 WHERE project_id=?1 AND object_id='draft-'||?2",params![project.as_str(),chapter],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?.flatten();
        if selected.as_deref() != Some(revision.as_str()) {
            return Err(RepositoryError::TaskInputStale);
        }
        let batch:Option<String>=db.query_row("SELECT b.batch_id FROM story_knowledge_heads_v2 h JOIN story_modeling_batches_v2 b ON b.project_id=h.project_id AND b.batch_id=h.batch_id AND b.object_id=h.object_id WHERE h.project_id=?1 AND h.object_id='draft-'||?2 AND b.source_revision_id=?3 AND b.status='ready'",params![project.as_str(),chapter,revision],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?;
        let needs_outcome = target["chapterAcceptance"].is_object();
        // Semantic extraction is an agent-selected aid. Author-requested outcome
        // checks can compare exact before/after prose without a knowledge pipeline.
        if batch.is_none() {
            available.push(action("modelKnowledge",json!({"type":"knowledge","chapterId":chapter,"chapterNumber":number,"sourceRevisionId":revision}),Some(writer.clone()),Some(revision.clone()),false));
        }
        let extraction:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.revision_id=h.candidate_revision_id AND r.project_id=h.project_id JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.status='completed' WHERE h.project_id=?1 AND h.object_id='commitment-extraction-'||?2 AND json_extract(r.content_json,'$.payload.sourceRevisionId')=?3)",params![project.as_str(),chapter,revision],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        if !extraction {
            available.push(action("extractCommitments",json!({"type":"commitmentExtraction","chapterId":chapter,"chapterNumber":number,"sourceRevisionId":revision}),Some(writer.clone()),Some(revision.clone()),false));
        }
        let writer_task = crate::tasks::load_task(db, &writer)?;
        let original = crate::revision_outcome::original_revision(db, &writer_task, &target)?;
        if needs_outcome {
            let assessment =
                crate::revision_outcome::assessment_state(db, project, &writer, &revision)?;
            let passed = assessment["verdict"] == "passed";
            requirements.push(json!({"kind":"revisionOutcome","chapterId":chapter,"sourceRevisionId":revision,"status":if passed{"satisfied"}else{"pending"},"assessment":assessment}));
            if !passed {
                available.push(action("assessRevisionOutcome",json!({"writerTaskId":writer,"chapterId":chapter,"chapterNumber":number,"originalRevisionId":original,"draftRevisionId":revision,"knowledgeBatchId":batch}),Some(writer.clone()),Some(revision.clone()),true));
                if crate::direct_verification::enabled(&target) {
                    available.push(action("verifyChapter",json!({"writerTaskId":writer,"chapterId":chapter,"chapterNumber":number,"originalRevisionId":original,"draftRevisionId":revision,"knowledgeBatchId":batch,"chapterAcceptance":target["chapterAcceptance"]}),Some(writer.clone()),Some(revision.clone()),false));
                    if let Some(independent)=available.iter_mut().rev().find(|a|a.operation=="assessRevisionOutcome") { independent.required=false; }
                }
            }
        }
        if let Some(batch) = batch {
            let plan:Option<String>=db.query_row("SELECT plan_revision_id FROM story_chapter_plan_bindings_v2 WHERE project_id=?1 AND chapter_id=?2",params![project.as_str(),chapter],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?;
            if let Some(plan) = plan {
                let current:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.revision_id=h.candidate_revision_id AND r.project_id=h.project_id JOIN story_task_outputs_v2 o ON o.revision_id=r.revision_id JOIN story_tasks_v2 t ON t.task_id=o.task_id AND t.status='completed' WHERE h.project_id=?1 AND h.object_id='scene-assessment-'||?2 AND json_extract(r.content_json,'$.draftRevisionId')=?3 AND json_extract(r.content_json,'$.planRevisionId')=?4 AND json_extract(r.content_json,'$.knowledgeBatchId')=?5)",params![project.as_str(),chapter,revision,plan,batch],|r|r.get(/*idx*/0)).map_err(storage_error)?;
                let current = current
                    && crate::work_decision_delivery::scene_feedback(db, project, work, &chapter)?
                        .is_none_or(|feedback| feedback.current);
                if !current {
                    let snapshot:String=db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project.as_str()],|r|r.get(/*idx*/0)).map_err(storage_error)?;
                    available.push(action("assessSceneOutcome",json!({"type":"sceneAssessment","chapterId":chapter,"chapterNumber":number,"planRevisionId":plan,"draftRevisionId":revision,"knowledgeBatchId":batch,"inputSnapshotId":snapshot,"intentGeneration":0}),Some(writer),Some(revision),false));
                }
            }
        }
    }
    drop(statement);
    crate::quality_notebook::reader_actions(db, project, work, &mut available)?;
    crate::work_decision_generation::available(
        db,
        project,
        work,
        &mut requirements,
        &mut available,
    )?;
    // Selected or already successful actions are not silently replayed. Known zero-output
    // failures can be selected under a successor; unknown outcomes remain recovery gates.
    crate::work_decision_recovery::failed_actions(db, work, &mut requirements, &mut available)?;
    let mut ready = Vec::new();
    for candidate in available {
        let selected:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.work_task_id=?1 AND a.action_key=?2 AND s.status IN ('queued','running','completed'))",params![work,candidate.key],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        let active:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.work_task_id=?1 AND a.action_key=?2 AND s.status IN ('queued','running'))",params![work,candidate.key],|r|r.get(0)).map_err(storage_error)?;
        if !selected
            || candidate.operation == "finishChapter"
            || (!active && candidate.operation == "assessRevisionOutcome")
        {
            ready.push(candidate)
        }
    }
    ready.sort_by(|a, b| (!a.required, &a.key).cmp(&(!b.required, &b.key)));
    Ok((requirements, ready))
}

pub(crate) fn status(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<Value, RepositoryError> {
    status_page(db, project, work, "", None, json!({}))
}

pub(crate) fn status_page(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    decision_task: &str,
    cursor: Option<&str>,
    envelope: Value,
) -> Result<Value, RepositoryError> {
    let (version, thread, kind): (i64, String, String) = db
        .query_row(
            "SELECT version,thread_id,kind FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2",
            params![work, project.as_str()],
            |r| Ok((r.get(/*idx*/ 0)?, r.get(/*idx*/ 1)?, r.get(/*idx*/ 2)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskNotFound)?;
    let source_digest = crate::task_input::project_source_digest(db, project)?;
    let page_digest = format!("{:x}", Sha256::digest(
        json!([receipt_page_digest(db, work, &source_digest)?, &envelope]).to_string()
    ));
    let page = read_page(
        cursor,
        project.as_str(),
        work,
        decision_task,
        version,
        &page_digest,
    )?;
    let (requirements, mut actions) = actions(db, project, work)?;
    // Match request_work_action admission: once this decision selected completion,
    // it must submit its final review, not select the same action after compaction.
    let completion_registration: Option<Value> = db.query_row(
        "SELECT action_key,source_revision_id FROM story_work_decision_actions_v1 WHERE work_task_id=?1 AND decision_task_id=?2 AND operation='finishChapter'",
        params![work, decision_task],
        |row| Ok(json!({"operation":"finishChapter","actionKey":row.get::<_,String>(0)?,"sourceRevisionId":row.get::<_,Option<String>>(1)?,"status":"registered","nextRequiredOperation":"complete_task"})),
    ).optional().map_err(storage_error)?;
    if completion_registration.is_some() {
        actions.clear();
    }
    let pending = requirements
        .iter()
        .filter(|r| r["status"] == "pending")
        .count();
    let work_plan = crate::model_work_plan::summary(db, work)?;
    let planned_pending = work_plan["pendingGoals"].as_u64().unwrap_or(0) > 0;
    let issue_pending = crate::work_issues::has_unresolved(db, project, work, None)?;
    let can_report_partial = ((kind != "generation" && planned_pending && pending == 0)
        || kind != "generation"
            && pending > 0
            && requirements
                .iter()
                .any(|r| r["status"] == "pending" && r["kind"] == "requestedOutput")
            && requirements
                .iter()
                .filter(|r| r["status"] == "pending")
                .all(|r| r["kind"] == "requestedOutput" || r["kind"] == "revisionOutcome"))
        && work_plan["canReportPartialGoals"] == true
        || (kind != "generation" && issue_pending);
    let total_requirements = requirements.len();
    let mut requirements = requirements;
    requirements.sort_by_key(|r| (r["status"] != "pending", r.to_string()));
    let chapter_acceptance = actions
        .iter()
        .find_map(|a| a.target.get("chapterAcceptance"))
        .cloned()
        .or_else(|| {
            requirements
                .iter()
                .find_map(|r| r["assessment"].get("chapterAcceptance").cloned())
        });
    // Status is a bounded decision summary. Standards occur once; the formal
    // report source delivers reasons and quotations through parsed pages.
    let requirements: Vec<_> = requirements
        .into_iter()
        .map(|mut requirement| {
            if let Some(assessment) = requirement.get_mut("assessment") {
                if let Some(fields) = assessment.as_object_mut() {
                    fields.remove("chapterAcceptance");
                    // Reasons remain in the paginated report. The decision page
                    // needs the omission count and verdict, not a second prose copy.
                    fields.remove("uncoveredRequirements");
                    if fields.get("uncoveredRequirementCount").and_then(Value::as_u64) == Some(0) {
                        fields.remove("uncoveredRequirementCount");
                    }
                }
            if let Some(checks) = assessment["checks"].as_array() {
                let counts = json!({"total":checks.len(),"supported":checks.iter().filter(|c|c["status"]=="supported").count(),"unmet":checks.iter().filter(|c|c["status"]=="unmet").count(),"insufficientEvidence":checks.iter().filter(|c|c["status"]=="insufficientEvidence").count()});
                if let Some(fields) = assessment.as_object_mut() { fields.remove("checks"); }
                assessment["checkCounts"] = counts;
            }
            if let Some(length) = assessment["length"].as_object_mut() {
                length.remove("requirement");
                length.remove("countUnit");
                length.remove("minCharacters");
                length.remove("maxCharacters");
                }
            }
            requirement
        })
        .collect();
    let total_actions = actions.len();
    let ready: Vec<Value> = actions
        .iter()
        .map(|a| {
            let mut result =
                json!({"actionKey":a.key,"operation":a.operation,"required":a.required});
            for field in ["chapterId", "chapterNumber", "qualityPacketId"] {
                if let Some(value) = a.target.get(field).filter(|v| !v.is_null()) {
                    result[field] = value.clone();
                }
            }
            if let Some(revision) = &a.source_revision {
                result["sourceRevisionId"] = json!(revision);
            }
            if a.author_action_id.is_some() {
                result["sourceTaskId"] = json!(a.source_task);
                result["authorization"] = json!("existingAuthorPlanningTarget");
            }
            if a.operation == "verifyChapter" {
                result["writerTaskId"] = a.target["writerTaskId"].clone();
                result["verificationMethod"] = json!("direct");
                result["chapterReviewRequired"] = json!(true);
            }
            if let Some(recovery) = &a.recovery {
                result["recovery"] = recovery.clone();
            }
            result
        })
        .collect();
    let mut result = json!({"issues":crate::work_issues::summary(db,project,work)?,"workTaskId":work,"threadId":thread,"version":version,"sourceDigest":source_digest,"requirements":requirements,"readyActions":ready,"chapterAcceptance":chapter_acceptance,"authorRequirementsSource":{"tool":"read_context","sectionPath":"/authorRequirementsSource"},"finalDeliveryReviewRequired":crate::work_decision_generation::final_delivery_pending(db,project,work)?,"canReportPartial":can_report_partial,"canFinish":!issue_pending && pending==0 && (kind!="generation" || crate::work_decision_generation::goal_complete(db,project,work)?),"coverage":{"pendingRequirements":pending,"totalRequirements":total_requirements,"availableActions":total_actions},"receipts":receipts(db,project,work)?});
    result["completionRegistration"] = completion_registration.unwrap_or(Value::Null);
    for (key, value) in envelope.as_object().ok_or(RepositoryError::CorruptState)? {
        result[key] = value.clone();
    }
    // Internal readiness consumers use the full state; wire consumers receive
    // standards once by immutable source, not repeated on every status page.
    if decision_task.is_empty() { return Ok(result); }
    let acceptance = result.as_object_mut().ok_or(RepositoryError::CorruptState)?.remove("chapterAcceptance");
    result["chapterAcceptanceSource"] = match acceptance.filter(Value::is_object) {
        Some(value) => crate::task_sources::work_acceptance_source(db, decision_task, work, &value)?,
        None => Value::Null,
    };
    let task = crate::tasks::load_task(db, decision_task)?;
    if let Some(ready) = result["readyActions"].as_array_mut() {
        for action in ready.iter_mut().filter(|a| a["operation"] == "verifyChapter") {
            let writer = action["writerTaskId"].as_str().ok_or(RepositoryError::CorruptState)?;
            let revision = action["sourceRevisionId"].as_str().ok_or(RepositoryError::CorruptState)?;
            action["currentDraftSource"] = crate::task_workspace::verification_source(db, &task, writer, revision)?;
        }
    }
    crate::work_decision_page::pack(result, page, |next| {
        page_cursor(project.as_str(), work, decision_task, version, &page_digest, next)
    })
}

// Nonterminal task progress need not advance the Work version. Bind receipts
// too, so a page cannot silently mix queued/running/failed snapshots.
fn receipt_page_digest(
    db: &Connection,
    work: &str,
    source: &str,
) -> Result<String, RepositoryError> {
    let mut statement = db.prepare("SELECT t.task_id,t.status,t.error FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND (t.operation!='advisory' OR EXISTS(SELECT 1 FROM story_media_adaptations_v1 ma WHERE ma.step_id=s.id)) ORDER BY s.ordinal DESC,s.id DESC").map_err(storage_error)?;
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let quality: i64 = db.query_row("SELECT COALESCE(q.version,0) FROM story_work_tasks_v1 w LEFT JOIN story_quality_state_v1 q ON q.project_id=w.project_id WHERE w.id=?1", [work], |r| r.get(0)).map_err(storage_error)?;
    hasher.update(quality.to_le_bytes());
    let rows = statement
        .query_map([work], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(storage_error)?;
    for row in rows {
        hasher.update(json!(row.map_err(storage_error)?).to_string().as_bytes());
    }
    // Selecting finishChapter persists a decision without changing prose or
    // allocating a new step; it still changes this decision's review obligation.
    let mut selections = db.prepare("SELECT decision_task_id,action_key FROM story_work_decision_actions_v1 WHERE work_task_id=?1 ORDER BY decision_task_id,action_key").map_err(storage_error)?;
    let rows = selections
        .query_map([work], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    for row in rows {
        hasher.update(json!(row.map_err(storage_error)?).to_string().as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn page_error() -> RepositoryError {
    RepositoryError::InvalidContent(
        "Work status cursor is stale or invalid; restart read_work_status with cursor null".into(),
    )
}
fn page_cursor(
    project: &str,
    work: &str,
    task: &str,
    version: i64,
    digest: &str,
    page: usize,
) -> String {
    format!(
        "{page}:{:x}",
        Sha256::digest(json!([project, work, task, version, digest, page]).to_string())
    )
}
fn read_page(
    cursor: Option<&str>,
    project: &str,
    work: &str,
    task: &str,
    version: i64,
    digest: &str,
) -> Result<usize, RepositoryError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    if cursor.len() > 100 {
        return Err(page_error());
    }
    let page = cursor
        .split_once(':')
        .and_then(|(page, _)| page.parse::<usize>().ok())
        .filter(|page| *page > 0 && *page <= i64::MAX as usize / 6)
        .ok_or_else(page_error)?;
    if cursor != page_cursor(project, work, task, version, digest, page) {
        return Err(page_error());
    }
    Ok(page)
}

pub(crate) fn complete(
    db: &Connection,
    task: &crate::TaskRecord,
    review: Option<&Value>,
) -> Result<(), RepositoryError> {
    let Some(bound) = crate::work_decision::binding(db, task)? else {
        if root_review_required(db, task)? {
            let work = crate::model_work_plan::work_for(db, task)?;
            let status = status(db, &task.project_id, &work)?;
            if status["canFinish"] != true
                && (status["canReportPartial"] != true
                    || review.is_none_or(|r| r["goalStatus"] != "partial"))
            {
                return Err(RepositoryError::TaskOutputMissing);
            }
            crate::work_delivery_review::save(db, task, &work, &status, review)?;
            db.execute(
                "UPDATE story_work_decisions_v1 SET finished=1 WHERE work_task_id=?1",
                [work],
            )
            .map_err(storage_error)?;
            return Ok(());
        }
        if review.is_some() {
            return Err(RepositoryError::InvalidContent(
                "deliveryReview belongs only to final Work decisions".into(),
            ));
        }
        return Ok(());
    };
    let work = bound["workTaskId"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let selected: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 WHERE decision_task_id=?1)",
            [&task.task_id],
            |r| r.get(/*idx*/ 0),
        )
        .map_err(storage_error)?;
    if selected || crate::multimedia::adaptation::selected(db, &task.task_id)? || crate::work_decision_generation::completion_selected(db, task)? {
        if crate::work_decision_generation::final_delivery_review_required(db, task)? {
            let current = status(db, &task.project_id, work)?;
            return crate::work_delivery_review::save(db, task, work, &current, review);
        }
        if review.is_some() {
            return Err(RepositoryError::InvalidContent(
                "deliveryReview cannot accompany another selected action".into(),
            ));
        }
        return Ok(());
    }
    let status = status(db, &task.project_id, work)?;
    if status["canFinish"] != true {
        if status["canReportPartial"] != true || review.is_none_or(|r| r["goalStatus"] != "partial")
        {
            return Err(RepositoryError::TaskOutputMissing);
        }
    }
    crate::work_delivery_review::save(db, task, work, &status, review)?;
    db.execute(
        "UPDATE story_work_decisions_v1 SET finished=1 WHERE work_task_id=?1",
        [work],
    )
    .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn resume_selected(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_generation_handoffs_v1 WHERE source_task_id=?1 AND json_type(content_json,'$.resumeRun')='object')",[task],|r|r.get(0)).map_err(storage_error)
}

pub(crate) fn root_review_required(
    db: &Connection,
    task: &crate::TaskRecord,
) -> Result<bool, RepositoryError> {
    // A temporary question completes its own read-only answer. It cannot settle
    // or inherit the original Work's delivery-review obligation.
    if crate::work_interludes::is_attempt(db, &task.task_id)? {
        return Ok(false);
    }
    if resume_selected(db, &task.task_id)? {
        return Ok(true);
    }
    if task.operation != "advisory" {
        return Ok(false);
    }
    if crate::work_decision::binding(db, task)?.is_some() {
        return Ok(false);
    }
    let selected:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_action_requests_v1 WHERE source_task_id=?1) OR EXISTS(SELECT 1 FROM story_planning_review_runs_v2 WHERE source_task_id=?1)",[&task.task_id],|r|r.get(0)).map_err(storage_error)?;
    if selected || crate::multimedia::adaptation::selected(db, &task.task_id)?
        || crate::multimedia::adaptation::is_attempt(db, &task.task_id)? {
        // Registered media steps preserve the original final decision obligation.
        return Ok(false);
    }
    // A resumed read-only discussion can have a new task thread without
    // acquiring the original root's method or final-delivery authority.
    // Checking whether a delivery obligation exists uses its actual current
    // Work membership; completing a real obligation still checks work_for.
    let work: String = db
        .query_row(
            "SELECT w.id FROM story_work_attempts_v1 a
         JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.current_attempt_id=a.attempt_id
         JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
         WHERE a.attempt_id=?1 AND w.project_id=?2",
            params![task.task_id, task.project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    // A descriptive method edit cannot close a previously registered author
    // delivery obligation. Only the actual delivery review settles this ledger.
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=0)",
        [&work],
        |row| row.get(0),
    )
    .map_err(storage_error)
}

fn receipts(db: &Connection, project: &ProjectId, work: &str) -> Result<Value, RepositoryError> {
    let mut statement=db.prepare("SELECT t.task_id,t.operation,t.status,t.error,s.id FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND (t.operation!='advisory' OR EXISTS(SELECT 1 FROM story_media_adaptations_v1 ma WHERE ma.step_id=s.id)) ORDER BY s.ordinal DESC,s.id DESC").map_err(storage_error)?;
    let mut rows = statement
        .query_map([work], |r| {
            Ok((
                r.get::<_, String>(/*idx*/ 0)?,
                r.get::<_, String>(/*idx*/ 1)?,
                r.get::<_, String>(/*idx*/ 2)?,
                r.get::<_, Option<String>>(/*idx*/ 3)?,
                r.get::<_, String>(/*idx*/ 4)?,
            ))
        })
        .map_err(storage_error)?
        .peekable();
    // One validated ledger per projection; never cache across status reads.
    // An empty receipt list must not require a funded Work.
    let failures_by_task: std::collections::HashMap<_, _> = if rows.peek().is_some() {
        crate::work_budget_storage::load(db, project, work)?.charges.into_iter()
            .map(|charge| (charge.task_id, charge.failure_observations)).collect()
    } else {
        std::collections::HashMap::new()
    };
    let mut items = Vec::new();
    for row in rows {
        let (task, operation, status, error, step) = row.map_err(storage_error)?;
        let mut outputs=db.prepare("SELECT output_kind,revision_id FROM story_task_outputs_v2 WHERE task_id=?1 ORDER BY ordinal DESC LIMIT 1").map_err(storage_error)?;
        let mut refs=outputs.query_map([&task],|r|Ok(json!({"kind":r.get::<_,String>(/*idx*/0)?,"revisionId":r.get::<_,String>(/*idx*/1)?}))).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
        for output in &mut refs {
            if output["kind"] == "draft" {
                let content: String = db.query_row(
                    "SELECT r.content_json FROM story_revisions_v2 r JOIN story_tasks_v2 t ON t.project_id=r.project_id WHERE t.task_id=?1 AND r.revision_id=?2",
                    params![task, output["revisionId"].as_str()], |row| row.get(0),
                ).map_err(storage_error)?;
                let content: Value =
                    serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
                output["chapterId"] = content["chapterId"].clone();
                output["chapterNumber"] = content["chapterNumber"].clone();
                output["title"] = content["title"].clone();
                if let Some(body) = content["body"].as_str() {
                    output["characterCount"] = json!(chapter_character_count(body));
                    output["countUnit"] = json!(
                        "Unicode characters excluding whitespace; punctuation included; body only"
                    );
                }
            }
        }
        if operation == "reviewQuality" {
            let packet: Option<String> = db.query_row("SELECT id FROM story_quality_packets_v1 WHERE task_id=?1 AND report_json IS NOT NULL", [&task], |r| r.get(0)).optional().map_err(storage_error)?;
            if let Some(packet) = packet {
                refs.push(json!({"kind":"qualityReading","packetId":packet,"authority":"readerHypotheses"}));
            }
        }
        if operation == "modelKnowledge" {
            let batch:Option<String>=db.query_row("SELECT batch_id FROM story_modeling_batches_v2 WHERE publisher_task_id=?1 AND published_at IS NOT NULL AND status IN ('ready','stale')",[&task],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?;
            if let Some(batch) = batch {
                refs.push(json!({"kind":"knowledgeBatch","batchId":batch}));
            }
        }
        // Retain task validation and the original project/Work rejection boundary.
        if crate::tasks::load_task(db, &task)?.project_id != *project {
            return Err(RepositoryError::CorruptState);
        }
        let failures = failures_by_task.get(&task).and_then(Option::as_ref);
        let mut receipt = json!({"taskId":task,"stepId":step,"operation":operation,"status":status,"outputs":refs});
        // Absence has the same meaning as null; preserve every actual diagnostic.
        for (field, value) in [
            ("failureObservations", json!(failures)),
            ("failureCode", json!(crate::conversation_receipts::failure_code(error.as_deref()))),
            ("toolValidation", json!(crate::conversation_receipts::tool_diagnostic(error.as_deref()))),
        ] {
            if !value.is_null() { receipt[field] = value; }
        }
        items.push(receipt);
    }
    let total:i64=db.query_row("SELECT COUNT(*) FROM story_work_steps_v1 s WHERE work_task_id=?1 AND current_attempt_id IS NOT NULL AND (operation!='advisory' OR EXISTS(SELECT 1 FROM story_media_adaptations_v1 ma WHERE ma.step_id=s.id))",[work],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    Ok(
        json!({"items":items,"coverage":{"complete":total as usize==items.len(),"total":total,"returned":items.len(),"offset":0}}),
    )
}

// Match the editor's Array.from(body.replace(/\s/g, "")).length, including
// ECMAScript's BOM whitespace and excluding Unicode NEXT LINE (U+0085).
pub(crate) fn chapter_character_count(body: &str) -> usize {
    body.chars()
        .filter(|c| {
            !matches!(*c,
                '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
                '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
                '\u{205f}' | '\u{3000}' | '\u{feff}'
            )
        })
        .count()
}

#[cfg(test)]
mod count_tests {
    #[test]
    fn saved_body_count_matches_editor_unicode_and_whitespace_rules() {
        assert_eq!(
            super::chapter_character_count("雨夜，灯亮。\n A 🕯\u{feff}\u{3000}"),
            8
        );
        assert_eq!(super::chapter_character_count("\u{0085}"), 1);
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::*;
    #[test]
    fn cursor_is_bound_to_decision_and_current_work_evidence() {
        let cursor = page_cursor("project", "work", "decision", 8, "digest", 1);
        assert_eq!(
            read_page(Some(&cursor), "project", "work", "decision", 8, "digest").unwrap(),
            1
        );
        for (project, work, task, version, digest) in [
            ("other", "work", "decision", 8, "digest"),
            ("project", "other", "decision", 8, "digest"),
            ("project", "work", "other", 8, "digest"),
            ("project", "work", "decision", 9, "digest"),
            ("project", "work", "decision", 8, "changed"),
        ] {
            assert!(read_page(Some(&cursor), project, work, task, version, digest).is_err());
        }
        for malformed in ["", "null", "0", "1:wrong", "18446744073709551615:wrong"] {
            assert!(
                read_page(Some(malformed), "project", "work", "decision", 8, "digest").is_err()
            );
        }
    }
    #[test]
    fn long_work_receipts_are_small_complete_pages_without_duplicates() {
        use crate::{
            BranchId, CreateTaskRequest, ProjectRecord, SnapshotId, SqliteStoryRepository,
        };
        let directory = tempfile::tempdir().unwrap();
        let mut repo =
            SqliteStoryRepository::open(&directory.path().join("pages.sqlite3")).unwrap();
        let project = ProjectId::parse("page-project").unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "分页".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        let mut work = String::new();
        for index in 0..27 {
            let task_id = format!("receipt-{index}");
            repo.create_task(CreateTaskRequest {
                task_id: task_id.clone(),
                project_id: project.clone(),
                thread_id: "thread".into(),
                operation: "generateBrief".into(),
                input_revision: 0,
                model: None,
                target_json: "{}".into(),
                input_manifest_json: json!({"authorIntent":"整理创作方向"}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: 100,
            })
            .unwrap();
            if index == 0 {
                work = repo.work_task_for_attempt(&project, &task_id).unwrap().id;
            }
            repo.connection.execute("UPDATE story_work_steps_v1 SET work_task_id=?1,ordinal=?2 WHERE current_attempt_id=?3", params![work, index, task_id]).unwrap();
        }
        let budget: crate::ProviderBudget = serde_json::from_value(json!({
            "maxRequests": null, "maxTokens": null, "usedRequests": 0,
            "usedTokens": 0, "unknownTokens": 0, "overrun": false,
            "charges": (0..27).map(|index| json!({
                "taskId":format!("receipt-{index}"),"budgetId":format!("budget-{index}"),
                "status":"failed","requests":0,"tokens":0,"unknownTokens":0,"overrun":false,
                "failureObservations":[{"code":"fixtureFailure","detail":format!("{index}:{}", "诊断".repeat(100))}]
            })).collect::<Vec<_>>()
        })).unwrap();
        repo.connection.execute(
            "INSERT INTO story_work_budgets_v1(work_task_id,ledger_json) VALUES (?1,?2)",
            params![work, serde_json::to_string(&budget).unwrap()],
        ).unwrap();
        let started = std::time::Instant::now();
        let mut projection = Value::Null;
        for _ in 0..20 { projection = receipts(&repo.connection, &project, &work).unwrap(); }
        eprintln!("receipt_projection_27_tasks_27_charges_20_reads_ms={:.3}", started.elapsed().as_secs_f64()*1000.0);
        for receipt in projection["items"].as_array().unwrap() {
            let charge = budget.charges.iter().find(|charge| charge.task_id == receipt["taskId"]).unwrap();
            assert_eq!(receipt["failureObservations"], json!(charge.failure_observations));
        }
        let mut updated_budget = budget.clone();
        updated_budget.charges[0].failure_observations = Some(vec![json!({"code":"newEvidence"})]);
        repo.connection.execute("UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",
            params![serde_json::to_string(&updated_budget).unwrap(),work]).unwrap();
        let updated = receipts(&repo.connection, &project, &work).unwrap();
        assert_eq!(updated["items"].as_array().unwrap().iter().find(|r|r["taskId"]=="receipt-0").unwrap()["failureObservations"], json!([{"code":"newEvidence"}]));
        repo.connection.execute("UPDATE story_work_budgets_v1 SET ledger_json='{}' WHERE work_task_id=?1",[&work]).unwrap();
        assert!(matches!(receipts(&repo.connection, &project, &work),Err(RepositoryError::CorruptState)));
        repo.connection.execute("UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",
            params![serde_json::to_string(&budget).unwrap(),work]).unwrap();
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0;
        loop {
            let status = status_page(
                &repo.connection,
                &project,
                &work,
                "receipt-0",
                cursor.as_deref(),
                json!({"testEnvelope":"文".repeat(1400)}),
            )
            .unwrap();
            assert_eq!(status["receipts"]["coverage"]["total"], 27);
            assert_eq!(status["receipts"]["coverage"]["offset"], seen.len());
            assert!(status.to_string().len() <= crate::work_decision::BOUNDED_JSON_BYTES);
            let items = status["receipts"]["items"].as_array().unwrap();
            assert!(!items.is_empty());
            for item in items {
                assert!(seen.insert(item["taskId"].as_str().unwrap().to_owned()));
            }
            pages += 1;
            cursor = status["pagination"]["nextCursor"]
                .as_str()
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        assert!(pages > 1);
        assert_eq!(seen.len(), 27);
        let first = status_page(&repo.connection, &project, &work, "receipt-0", None, json!({"testEnvelope":"文".repeat(1400)})).unwrap();
        let next = first["pagination"]["nextCursor"].as_str().unwrap();
        assert!(status_page(&repo.connection, &project, &work, "receipt-0", Some(next), json!({"testEnvelope":"changed"})).is_err());
        repo.connection
            .execute(
                "UPDATE story_tasks_v2 SET status='running' WHERE task_id='receipt-6'",
                [],
            )
            .unwrap();
        assert!(status_page(&repo.connection, &project, &work, "receipt-0", Some(next), json!({"testEnvelope":"文".repeat(1400)})).is_err());
        let refreshed = status_page(&repo.connection, &project, &work, "receipt-0", None, json!({"testEnvelope":"文".repeat(1400)})).unwrap();
        let next = refreshed["pagination"]["nextCursor"].as_str().unwrap();
        repo.connection
            .execute(
                "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
                [&work],
            )
            .unwrap();
        assert!(status_page(&repo.connection, &project, &work, "receipt-0", Some(next), json!({"testEnvelope":"文".repeat(1400)})).is_err());
    }

use crate::{ProjectRevisionRepository, TasksRepository, WorkTasksRepository};
}
