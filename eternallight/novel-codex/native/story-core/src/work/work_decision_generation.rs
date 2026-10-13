//! Generation uses the same decisions and Work budget. These are domain primitives,
//! not a second planner: the agent selects planning, writing and chapter completion.
use crate::authoring_runs::storage_error;
use crate::work_decision_status::{ReadyAction, action};
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, StoredGenerationRun, TaskRecord};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};

const DECISION_INSTRUCTION: &str = "Decide the next authorized work action from read_work_status; use actual receipts and the author goal.";

fn load_state(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<Option<Value>, RepositoryError> {
    let run = work
        .strip_prefix("work-")
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let content: Option<String> = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",
            params![project.as_str(), run],
            |r| r.get(/*idx*/ 0),
        )
        .optional()
        .map_err(storage_error)?;
    content
        .map(|s| serde_json::from_str(&s).map_err(|_| RepositoryError::CorruptState))
        .transpose()
}
// A guarded checkpoint refresh can supersede the current undelivered chapter's
// old writer. Only an author replan changes acceptance intent identity. Earlier
// delivered chapters and ordinary dialog writes still require their exact source.
fn current_chapter_replan(state: &Value) -> Option<(String, i64)> {
    let chapter = state["currentChapterId"].as_str()?;
    let number = state["currentChapterNumber"].as_i64()?;
    let generation = state["intentGeneration"].as_i64()?;
    let replan = state["replans"].as_array()?.last()?;
    let intent = state["intentRevisions"].as_array()?.last()?;
    (number == state["completedChapters"].as_i64()? + 1
        && replan["restartFrom"] == "currentChapter"
        && replan["intentGeneration"] == generation
        && intent["generation"] == generation
        && intent["effectiveFromChapter"].as_i64().is_some_and(|from| from > 0 && from <= number))
        .then(|| (chapter.to_owned(), generation))
}

pub(crate) fn replanned_current_chapter(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<Option<(String, i64)>, RepositoryError> {
    let Some(state) = load_state(db, project, work)? else {
        return Ok(None);
    };
    let Some(replanned) = current_chapter_replan(&state) else {
        return Ok(None);
    };
    crate::generation_author_source::validate_run(
        db,
        project,
        state["id"].as_str().ok_or(RepositoryError::CorruptState)?,
        &state,
    )?;
    Ok(Some(replanned))
}

fn save(db: &Connection, project: &ProjectId, state: &mut Value) -> Result<(), RepositoryError> {
    let work = format!("work-{}", state["id"].as_str().ok_or(RepositoryError::CorruptState)?);
    if let Some(barrier) = crate::work_pause_barrier::read(db, &work)? {
        state["status"] = json!(if barrier.remaining_attempts == 0 { "paused" } else { "running" });
    }
    let version = state["version"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    state["version"] = json!(version + 1);
    let count=db.execute("UPDATE story_generation_runs_v1 SET version=?3,state_json=?4 WHERE project_id=?1 AND run_id=?2 AND version=?5",params![project.as_str(),state["id"].as_str(),version+1,state.to_string(),version]).map_err(storage_error)?;
    if count != 1 {
        return Err(RepositoryError::AuthoringRunStale);
    }
    db.execute(
        "UPDATE story_work_tasks_v1 SET status=?3,version=version+1 WHERE project_id=?1 AND id='work-'||?2 AND status!=?3",
        params![project.as_str(), state["id"].as_str(), state["status"].as_str()],
    ).map_err(storage_error)?;
    Ok(())
}
pub(crate) fn bind_target(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    target: &mut Value,
) -> Result<(), RepositoryError> {
    let state = load_state(db, project, work)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    crate::generation_author_source::validate_run(
        db,
        project,
        state["id"].as_str().ok_or(RepositoryError::CorruptState)?,
        &state,
    )?;
    target["runId"] = state["id"].clone();
    target["runVersion"] = state["version"].clone();
    target["stepToken"] = state["stepToken"].clone();
    target["intentGeneration"] = state["intentGeneration"].clone();
    Ok(())
}
pub(crate) fn bind_action_target(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    target: &mut Value,
    _operation: &str,
) -> Result<(), RepositoryError> {
    if load_state(db, project, work)?.is_some() {
        bind_target(db, project, work, target)?;
    }
    Ok(())
}
fn selected_plan(
    db: &Connection,
    project: &ProjectId,
    number: i64,
) -> Result<Option<(String, Value)>, RepositoryError> {
    let content:Option<(String,String)>=db.query_row("SELECT r.revision_id,r.content_json FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id) AND r.project_id=h.project_id AND r.object_id=h.object_id WHERE h.project_id=?1 AND h.object_id='artifact-plan'",[project.as_str()],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?))).optional().map_err(storage_error)?;
    let Some((revision, content)) = content else {
        return Ok(None);
    };
    let plan: Value = serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
    let chapters = plan["payload"]["chapters"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let matches: Vec<_> = chapters
        .iter()
        .filter(|c| c["chapterNumber"] == number)
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [chapter] => Ok(Some((revision, (*chapter).clone()))),
        _ => Err(RepositoryError::CorruptState),
    }
}
/// Drafting can start directly from author intent; absent planning is not invented.
fn writer_target(chapter: &str, number: i64, plan: Option<&Value>) -> Value {
    json!({"type":"chapter","chapterId":chapter,"chapterNumber":number,
        "readingChapter":number,"sceneOrder":1,"scope":{"type":"wholeChapter"},
        "povCharacterId":plan.and_then(|p| p.get("povCharacterId")),
        "storyTime":plan.and_then(|p| p.get("storyTime"))})
}

pub(crate) fn available(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    requirements: &mut Vec<Value>,
    available: &mut Vec<ReadyAction>,
) -> Result<(), RepositoryError> {
    let Some(state) = load_state(db, project, work)? else {
        return Ok(());
    };
    if state["status"] != "running" {
        return Err(RepositoryError::TaskNotWritable);
    }
    let number = state["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?
        + 1;
    let target = state["targetChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    if number > target {
        return Ok(());
    }
    requirements.push(json!({"kind":"chapterDelivery","chapterNumber":number,"status":"pending"}));
    if let Some(chapter) = state["currentChapterId"].as_str() {
        let draft:Option<String>=db.query_row("SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_object_heads_v2 WHERE project_id=?1 AND object_id='draft-'||?2",params![project.as_str(),chapter],|r|r.get(/*idx*/0)).optional().map_err(storage_error)?.flatten();
        if let Some(revision) = draft {
            let acceptance =
                crate::chapter_delivery_review::current_acceptance(db, project, chapter)?;
            if let Some(feedback) =
                crate::work_decision_delivery::scene_feedback(db, project, work, chapter)?
                && feedback.needs_recheck()
            {
                requirements.push(json!({"kind":"sceneFeedbackRecheck","chapterId":chapter,
                    "assessmentRevisionId":feedback.revision,"status":"pending",
                    "reason":"savedFindingsNeedCurrentSourceRecheck"}));
            }
            if state["feedbackRepairPrompt"].is_string() {
                let plan =
                    crate::bound_writer_plan::bound_chapter_plan_for_chapter(db, project, chapter)?;
                available.push(action(
                    "generateChapter",
                    {
                        let mut target = writer_target(chapter, number, plan.as_ref());
                        target["chapterAcceptance"] = acceptance.clone();
                        target
                    },
                    None,
                    Some(revision.clone()),
                    true,
                ));
                return Ok(());
            }
            if current_chapter_replan(&state).is_some() {
                let rewritten: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND t.operation='generateChapter' AND t.status='completed' AND json_extract(t.target_json,'$.chapterId')=?2 AND json_extract(t.target_json,'$.intentGeneration')=?3)",
                    params![work, chapter, state["intentGeneration"].as_i64()], |r| r.get(0),
                ).map_err(storage_error)?;
                if !rewritten {
                    let plan = crate::bound_writer_plan::bound_chapter_plan_for_chapter(
                        db, project, chapter,
                    )?;
                    available.push(action(
                        "generateChapter",
                        writer_target(chapter, number, plan.as_ref()),
                        None,
                        Some(revision.clone()),
                        false,
                    ));
                }
            }
            let acceptance =
                crate::chapter_delivery_review::current_acceptance(db, project, chapter)?;
            // Revision is a first-class choice after saving, without a fabricated
            // failed task or mandatory analysis. The model selects its necessity.
            if !available
                .iter()
                .any(|a| a.operation == "generateChapter" && a.target["chapterId"] == chapter)
            {
                let plan =
                    crate::bound_writer_plan::bound_chapter_plan_for_chapter(db, project, chapter)?;
                available.push(action(
                    "generateChapter",
                    writer_target(chapter, number, plan.as_ref()),
                    None,
                    Some(revision.clone()),
                    false,
                ));
            }
            for choice in available.iter_mut().filter(|a| {
                matches!(a.operation.as_str(), "generateChapter" | "finishChapter")
                    && a.target["chapterId"] == chapter
            }) {
                choice.target["chapterAcceptance"] = acceptance.clone();
                let recovery = choice.recovery.clone();
                *choice = action(
                    &choice.operation,
                    choice.target.clone(),
                    choice.source_task.clone(),
                    choice.source_revision.clone(),
                    choice.required,
                );
                choice.recovery = recovery;
            }
            let mandatory = requirements
                .iter()
                .any(|r| r["status"] == "pending" && r["kind"] != "chapterDelivery");
            let verified = if mandatory {
                false
            } else {
                match crate::revision_outcome::verify_chapter(db, project, work, chapter, &revision) {
                    Ok(_) => true,
                    Err(RepositoryError::TaskOutputMissing | RepositoryError::InvalidContent(_)) => false,
                    Err(error) => return Err(error),
                }
            };
            if verified {
                available.push(action(
                    "finishChapter",
                    json!({"chapterId":chapter,"chapterNumber":number,"chapterAcceptance":acceptance}),
                    None,
                    Some(revision),
                    true,
                ));
            }
        } else {
            let plan =
                crate::bound_writer_plan::bound_chapter_plan_for_chapter(db, project, chapter)?;
            available.push(action(
                "generateChapter",
                writer_target(chapter, number, plan.as_ref()),
                None,
                None,
                true,
            ));
        }
    } else {
        // The agent can plan first or draft directly from author intent.
        for operation in ["generateBrief", "generateOutline"] {
            available.push(action(
                operation,
                json!({"type":"artifact"}),
                None,
                None,
                false,
            ));
        }
        available.push(action(
            "generatePlan",
            json!({"type":"artifact","planFromChapter":number}),
            None,
            None,
            false,
        ));
        let plan_revision = selected_plan(db, project, number)?.map(|(revision, _)| revision);
        available.push(action(
            "writeNextChapter",
            json!({"chapterNumber":number,"planRevisionId":plan_revision}),
            None,
            plan_revision,
            true,
        ));
    }
    Ok(())
}
pub(crate) fn request_finish(
    db: &Connection,
    task: &TaskRecord,
    bound: &Value,
    action: &ReadyAction,
) -> Result<(), RepositoryError> {
    let work = bound["workTaskId"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let state =
        load_state(db, &task.project_id, work)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    if state["activeTaskId"] != task.task_id
        || state["currentChapterId"] != action.target["chapterId"]
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if let Some(chapter) = state["currentChapterId"].as_str()
        && crate::work_decision_delivery::scene_feedback(db, &task.project_id, work, chapter)?
            .is_some_and(|feedback| feedback.needs_recheck())
    {
        return Err(RepositoryError::InvalidContent(
            "chapterAssessmentNeedsRecheck".into(),
        ));
    }
    let other:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND id!=?2 AND status IN ('queued','running','blocked'))",params![work,bound["decisionStepId"].as_str()],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    if other {
        return Err(RepositoryError::TaskNotWritable);
    }
    Ok(())
}
pub(crate) fn completion_selected(
    db: &Connection,
    task: &TaskRecord,
) -> Result<bool, RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let Some(run) = target["runId"].as_str() else {
        return Ok(false);
    };
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decision_actions_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.work_task_id=a.work_task_id WHERE a.decision_task_id=?1 AND a.work_task_id=?2 AND a.operation='finishChapter' AND s.current_attempt_id=?1)",params![task.task_id,format!("work-{run}")],|row|row.get(/*idx*/0)).map_err(storage_error)
}

/// Expose final-delivery responsibility without accepting a model-supplied scope.
pub(crate) fn final_delivery_pending(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<bool, RepositoryError> {
    Ok(load_state(db, project, work)?.is_some_and(|state| {
        state["status"] == "running"
            && state["currentChapterId"].is_string()
            && state["currentChapterNumber"] == state["targetChapters"]
    }))
}

/// Only the selected last-chapter finish may accompany a delivery review.
pub(crate) fn final_delivery_review_required(
    db: &Connection,
    task: &TaskRecord,
) -> Result<bool, RepositoryError> {
    if !completion_selected(db, task)? {
        return Ok(false);
    }
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let run = target["runId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let work = format!("work-{run}");
    let state =
        load_state(db, &task.project_id, &work)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    if state["activeTaskId"] != task.task_id || state["status"] != "running" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let number = state["currentChapterNumber"]
        .as_i64()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let completed = state["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    if number != completed + 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(state["targetChapters"] == number)
}

fn materialize_selected(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    state: &mut Value,
    step: &str,
) -> Result<(), RepositoryError> {
    crate::execution_scope_storage::assert_attempt_capacity(db, project, work)?;
    let now: i64 = db
        .query_row("SELECT unixepoch()", [], |row| row.get(/*idx*/ 0))
        .map_err(storage_error)?;
    let deadline: Option<i64> = serde_json::from_value(state["deadlineAt"].clone())
        .map_err(|_| RepositoryError::CorruptState)?;
    if deadline.is_some_and(|deadline| now >= deadline)
        || crate::work_budget_storage::load(db, project, work)?.exhausted()
    {
        return Err(RepositoryError::TaskNotWritable);
    }
    let number = state["completedChapters"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?
        + 1;
    if !state["currentChapterId"].is_null()
        || number
            > state["targetChapters"]
                .as_i64()
                .ok_or(RepositoryError::CorruptState)?
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let encoded:String=db.query_row("SELECT target_json FROM story_work_steps_v1 WHERE id=?1 AND work_task_id=?2 AND operation='writeNextChapter' AND status='queued'",params![step,work],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    let mut target: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    let selected = selected_plan(db, project, number)?;
    if target["planRevisionId"].as_str() != selected.as_ref().map(|(revision, _)| revision.as_str())
    {
        return Err(RepositoryError::TaskInputStale);
    }
    let plan = selected.as_ref().map(|(_, plan)| plan);
    let actual: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(number),0)+1 FROM story_chapters_v2 WHERE project_id=?1",
            [project.as_str()],
            |r| r.get(/*idx*/ 0),
        )
        .map_err(storage_error)?;
    if actual != number {
        return Err(RepositoryError::TaskInputStale);
    }
    if let Some(plan) = plan {
        crate::bound_writer_plan::validate_plan_position(plan)?;
    }
    let chapter = format!("chapter-{}", uuid::Uuid::new_v4());
    let title = plan
        .and_then(|plan| {
            plan["publicTitle"]
                .as_str()
                .or_else(|| plan["title"].as_str())
        })
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("第 {number} 章"));
    db.execute("INSERT INTO story_chapters_v2(chapter_id,project_id,number,title,status,summary) VALUES(?1,?2,?3,?4,'draft','')",params![chapter,project.as_str(),number,title]).map_err(storage_error)?;
    if let Some((revision, plan)) = &selected {
        let plan_id = plan["chapterPlanId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or(RepositoryError::CorruptState)?;
        db.execute("INSERT INTO story_chapter_plan_bindings_v2(project_id,plan_revision_id,chapter_plan_id,chapter_id,created_at) VALUES(?1,?2,?3,?4,unixepoch())",params![project.as_str(),revision,plan_id,chapter]).map_err(storage_error)?;
    }
    db.execute(
        "UPDATE story_workspaces_v2 SET revision=revision+1 WHERE project_id=?1",
        [project.as_str()],
    )
    .map_err(storage_error)?;
    state["currentChapterId"] = json!(chapter);
    state["currentChapterNumber"] = json!(number);
    state["sourceDigest"] = json!(crate::task_input::project_source_digest(db, project)?);
    for (key, value) in writer_target(&chapter, number, plan)
        .as_object()
        .ok_or(RepositoryError::CorruptState)?
    {
        target[key] = value.clone()
    }
    target
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?
        .remove("planRevisionId");
    db.execute("UPDATE story_work_steps_v1 SET operation='generateChapter',target_json=?2,input_revision=(SELECT revision FROM story_workspaces_v2 WHERE project_id=?3) WHERE id=?1",params![step,target.to_string(),project.as_str()]).map_err(storage_error)?;
    db.execute("UPDATE story_work_decision_actions_v1 SET operation='generateChapter',target_json=?2 WHERE step_id=?1",params![step,target.to_string()]).map_err(storage_error)?;
    db.execute("UPDATE story_work_tasks_v1 SET version=version+1,input_revision=(SELECT revision FROM story_workspaces_v2 WHERE project_id=?2) WHERE id=?1",params![work,project.as_str()]).map_err(storage_error)?;
    save(db, project, state)
}

pub(crate) fn recoverable_stopped_run(db: &Connection, state: &Value) -> Result<bool, RepositoryError> {
    if state["status"] == "blocked" && state["blockedReason"] == "recoveryStalled" {
        let work = format!("work-{}", state["id"].as_str().ok_or(RepositoryError::CorruptState)?);
        let fingerprint = crate::work_decision::recovery_fingerprint(db, &work)?;
        return Ok(crate::work_decision::unchanged_failures(db, &work, &fingerprint)? < 2);
    }
    if state["status"] != "blocked" || !matches!(state["blockedReason"].as_str(), Some("executionBlocked" | "pendingRecovery" | "pendingUsage" | "pendingWriteAndUsage")) {
        return Ok(false);
    }
    let Some(task) = state["activeTaskId"].as_str() else { return Ok(false); };
    Ok(crate::work_decision_status::retryable(db, task)?
        || crate::conversation_receipts::recoverable_saved_chapter(db, task)?)
}

// Observe termination in the same transaction as the Attempt. Scheduling a
// later /next is not required to make the absence of a live writer visible.
pub(crate) fn observe_terminal_attempt(db: &Connection, work: &str, task: &TaskRecord) -> Result<(), RepositoryError> {
    if !matches!(task.status.as_str(), "failed" | "cancelled") { return Ok(()); }
    let kind: String = db.query_row("SELECT kind FROM story_work_tasks_v1 WHERE id=?1", [work], |r|r.get(0)).map_err(storage_error)?;
    if kind != "generation" { return Ok(()); }
    let mut state = load_state(db, &task.project_id, work)?.ok_or(RepositoryError::CorruptState)?;
    if state["status"] != "running" || state["activeTaskId"] != task.task_id
        || crate::work_pause_barrier::read(db, work)?.is_some()
        || crate::work_interludes::top(db, work)?.is_some() { return Ok(()); }
    state["status"] = json!("blocked");
    // Pending writes/usage are projected from their live ledgers; copying them
    // here would leave a stale "pendingUsage" label after settlement succeeds.
    state["blockedReason"] = json!("executionBlocked");
    save(db, &task.project_id, &mut state)
}



pub(crate) fn goal_complete(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<bool, RepositoryError> {
    Ok(load_state(db, project, work)?
        .is_some_and(|s| s["completedChapters"] == s["targetChapters"]))
}

/// Public work decision generation capability; SQLite retains transaction ownership.
pub trait WorkDecisionGenerationRepository {
    fn next_generation_decision(
        &mut self,
        project: &ProjectId,
        run: &str,
    ) -> Result<Value, RepositoryError>;
    fn advance_generation_decision(
        &mut self,
        project: &ProjectId,
        run: &str,
        version: i64,
        task_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError>;
}

impl WorkDecisionGenerationRepository for SqliteStoryRepository {
    fn next_generation_decision(
        &mut self,
        project: &ProjectId,
        run: &str,
    ) -> Result<Value, RepositoryError> {
        let work = format!("work-{run}");
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        let mut state =
            load_state(&tx, project, &work)?.ok_or(RepositoryError::AuthoringRunNotFound)?;
        crate::generation_author_source::validate_run(&tx, project, run, &state)?;
        if crate::work_pause_barrier::read(&tx, &work)?.is_some() {
            tx.commit().map_err(storage_error)?;
            return Ok(json!({"action":null,"stopReason":"authorPauseAfterStep"}));
        }
        if recoverable_stopped_run(&tx, &state)? {
            state["status"] = json!("running");
            state["blockedReason"] = Value::Null;
            save(&tx, project, &mut state)?;
        }
        if state["status"] != "running" {
            // An already-projected failure still exposes its live write/usage
            // fence. Do not cache ledger-derived reasons in the Run state.
            let reason = if state["status"] == "blocked" {
                crate::work_modeling_recovery::pending_reason(&tx, project, &work)?
                    .map(str::to_owned)
                    .or_else(|| state["blockedReason"].as_str().map(str::to_owned))
            } else {
                None
            };
            tx.commit().map_err(storage_error)?;
            return Ok(json!({"action":null,"stopReason":reason}));
        }
        if let Some(task) = state["activeTaskId"].as_str().map(str::to_owned) {
            let record = crate::tasks::load_task(&tx, &task)?;
            if crate::conversation_receipts::recoverable_saved_chapter(&tx, &task)? {
                // Restore artifact readiness, retaining execution failure and all charges.
                // Chapter delivery still requires the agent's assessment and finishChapter.
                tx.execute("UPDATE story_tasks_v2 SET status='completed',stage='ready_for_review',delivery_status='ready_for_review',result_summary='已核验保存的完整正文，待后续评估；原请求失败记录保留' WHERE task_id=?1 AND status='failed'", [&task]).map_err(storage_error)?;
                crate::work_tasks::finish_attempt(&tx, &crate::tasks::load_task(&tx, &task)?)?;
                state["activeTaskId"] = Value::Null;
                state["lastTaskId"] = json!(task);
                state["sourceDigest"] =
                    json!(crate::task_input::project_source_digest(&tx, project)?);
                state["stepToken"] = json!(uuid::Uuid::new_v4().simple().to_string());
                save(&tx, project, &mut state)?;
            } else if record.status == "failed"
                && crate::work_decision_status::retryable(&tx, &task)?
            {
                state["activeTaskId"] = Value::Null;
                state["lastTaskId"] = json!(task);
                state["stepToken"] = json!(uuid::Uuid::new_v4().simple().to_string());
                save(&tx, project, &mut state)?;
            } else if matches!(record.status.as_str(), "failed" | "cancelled") {
                let reason = crate::work_modeling_recovery::pending_reason(&tx, project, &work)?
                    .unwrap_or("executionBlocked");
                state["status"] = json!("blocked");
                state["blockedReason"] = json!(reason);
                save(&tx, project, &mut state)?;
                tx.commit().map_err(storage_error)?;
                return Ok(json!({"action":null,"stopReason":reason}));
            } else {
                tx.commit().map_err(storage_error)?;
                return Ok(
                    json!({"action":{"kind":"awaitTask","taskId":task,"taskStatus":record.status}}),
                );
            }
        }
        if crate::task_input::project_source_digest(&tx, project)? != state["sourceDigest"] {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let queued:Option<(String,String)>=tx.query_row("SELECT s.id,s.operation FROM story_work_steps_v1 s WHERE s.work_task_id=?1 AND s.status='queued' AND (s.depends_on_step_id IS NULL OR EXISTS(SELECT 1 FROM story_work_steps_v1 d WHERE d.id=s.depends_on_step_id AND d.status='completed')) ORDER BY s.ordinal LIMIT 1",[&work],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?))).optional().map_err(storage_error)?;
        if let Some((step, operation)) = queued {
            if operation == "writeNextChapter" {
                materialize_selected(&tx, project, &work, &mut state, &step)?;
            }
            let encoded: String = tx
                .query_row(
                    "SELECT target_json FROM story_work_steps_v1 WHERE id=?1",
                    [&step],
                    |r| r.get(/*idx*/ 0),
                )
                .map_err(storage_error)?;
            let mut target: Value =
                serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
            bind_target(&tx, project, &work, &mut target)?;
            tx.execute("UPDATE story_work_steps_v1 SET target_json=?2,input_revision=(SELECT revision FROM story_workspaces_v2 WHERE project_id=?3) WHERE id=?1",params![step,target.to_string(),project.as_str()]).map_err(storage_error)?;
            tx.execute(
                "UPDATE story_work_decision_actions_v1 SET target_json=?2 WHERE step_id=?1",
                params![step, target.to_string()],
            )
            .map_err(storage_error)?;
            let operation: String = tx
                .query_row(
                    "SELECT operation FROM story_work_steps_v1 WHERE id=?1",
                    [&step],
                    |r| r.get(/*idx*/ 0),
                )
                .map_err(storage_error)?;
            target["resumeStepId"] = json!(step);
            let instruction: Option<String> = tx
                .query_row(
                    "SELECT instruction FROM story_work_decision_actions_v1 WHERE step_id=?1",
                    [&step],
                    |r| r.get(/*idx*/ 0),
                )
                .optional()
                .map_err(storage_error)?;
            let instruction = if operation == "advisory" {
                DECISION_INSTRUCTION.to_owned()
            } else {
                instruction
                    .filter(|text| !text.trim().is_empty())
                    .ok_or(RepositoryError::CorruptState)?
            };
            tx.commit().map_err(storage_error)?;
            return Ok(
                json!({"action":{"kind":"agentTask","operation":operation,"target":target,"instruction":instruction}}),
            );
        }
        tx.commit().map_err(storage_error)?;
        let work_state = self.work_task(project, &work)?;
        let response = self.prepare_work_decision(
            project,
            &work,
            &crate::WorkDecisionRequest {
                expected_work_version: work_state.version,
                expected_revision: self.project_revision(project)?,
                expected_source_digest: self.project_source_watermarks(project)?["digest"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?
                    .into(),
            },
        )?;
        if let Some(step) = response.step {
            let mut target = step.target;
            target["resumeStepId"] = json!(step.id);
            Ok(
                json!({"action":{"kind":"agentTask","operation":"advisory","target":target,"instruction":DECISION_INSTRUCTION}}),
            )
        } else {
            if let Some(reason) = response.stop_reason.filter(|reason| *reason != "completed") {
                let tx = rusqlite::Transaction::new_unchecked(
                    &self.connection,
                    TransactionBehavior::Immediate,
                )
                .map_err(storage_error)?;
                let mut blocked = load_state(&tx, project, &work)?
                    .ok_or(RepositoryError::AuthoringRunNotFound)?;
                blocked["status"] = json!("blocked");
                blocked["blockedReason"] = json!(reason);
                save(&tx, project, &mut blocked)?;
                tx.commit().map_err(storage_error)?;
            }
            Ok(json!({"action":null,"stopReason":response.stop_reason}))
        }
    }

    fn advance_generation_decision(
        &mut self,
        project: &ProjectId,
        run: &str,
        version: i64,
        task_id: &str,
    ) -> Result<StoredGenerationRun, RepositoryError> {
        let tx = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        let work = format!("work-{run}");
        let mut state =
            load_state(&tx, project, &work)?.ok_or(RepositoryError::AuthoringRunNotFound)?;
        if state["version"] != version {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let task = crate::tasks::load_task(&tx, task_id)?;
        if task.project_id != *project
            || task.status != "completed"
            || state["activeTaskId"] != task_id
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_input::assert_task_inputs_current(&tx, &task)?;
        let budget = crate::work_budget_storage::load(&tx, project, &work)?;
        if !budget.has_charge(task_id) {
            return Err(RepositoryError::TaskNotWritable);
        }
        state["activeTaskId"] = Value::Null;
        state["lastTaskId"] = json!(task_id);
        state["stage"] = json!("coordinate");
        state["sourceDigest"] = json!(crate::task_input::project_source_digest(&tx, project)?);
        state["stepToken"] = json!(uuid::Uuid::new_v4().simple().to_string());
        if task.operation == "generateChapter" {
            state["feedbackRepairPrompt"] = Value::Null;
        }
        if completion_selected(&tx, &task)? {
            let selected:String=tx.query_row("SELECT target_json FROM story_work_decision_actions_v1 WHERE decision_task_id=?1 AND operation='finishChapter'",[&task.task_id],|row|row.get(0)).map_err(storage_error)?;
            let selected: Value =
                serde_json::from_str(&selected).map_err(|_| RepositoryError::CorruptState)?;
            let receipt = &selected["chapterReviewReceipt"];
            crate::chapter_delivery_review::verify(
                &tx,
                project,
                selected["chapterId"]
                    .as_str()
                    .ok_or(RepositoryError::TaskBindingInvalid)?,
                receipt["revisionId"]
                    .as_str()
                    .ok_or(RepositoryError::TaskBindingInvalid)?,
                Some(receipt),
            )?;
            match crate::work_decision_delivery::checkpoint(&tx, project, run, &mut state) {
                Ok(()) => {}
                Err(RepositoryError::InvalidContent(reason))
                    if reason == "chapterAssessmentNeedsAuthorDecision" =>
                {
                    state["status"] = json!("blocked");
                    state["blockedReason"] = json!("scene_feedback_needs_author");
                    tx.execute("UPDATE story_work_tasks_v1 SET status='blocked',version=version+1 WHERE id=?1",[&work]).map_err(storage_error)?;
                }
                Err(error) => return Err(error),
            }
        }
        save(&tx, project, &mut state)?;
        tx.commit()?;
        self.generation_run(project, run)
    }
}

use crate::{
    AuthoringRunsRepository, ProjectRevisionRepository, TaskInputRepository,
    WorkDecisionRepository, WorkTasksRepository,
};
