use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    AuthorReadingProgress, ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord,
    WorkResumeMode,
};

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkStep {
    pub id: String,
    pub operation: String,
    pub expected_output_kind: Option<String>,
    pub instruction: Option<String>,
    pub source_task_id: Option<String>,
    pub target: Value,
    pub status: String,
    pub input_revision: i64,
    pub depends_on_step_id: Option<String>,
    pub current_attempt_id: Option<String>,
    pub attempts: Vec<WorkAttempt>,
    pub output_refs: Value,
    pub changes: Vec<Value>,
    pub reading_progress: Option<AuthorReadingProgress>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkAttempt {
    pub failure_observations: Option<Vec<Value>>,
    pub id: String,
    pub attempt_number: i64,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkTask {
    pub id: String,
    pub project_id: String,
    pub thread_id: String,
    pub goal: String,
    pub kind: String,
    pub status: String,
    pub delivery_status: String,
    pub delivery_report: crate::WorkDeliveryReport,
    pub version: i64,
    pub control_revision: i64,
    pub pause_after_step: Option<crate::WorkPauseAfterStep>,
    pub interlude: Option<crate::WorkInterlude>,
    pub error: Option<String>,
    pub resume_block_reason: Option<String>,
    pub resume_mode: Option<WorkResumeMode>,
    pub pending_continuation: bool,
    pub steps: Vec<WorkStep>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkTaskPage {
    pub tasks: Vec<WorkTask>,
    pub next_cursor: Option<String>,
    pub active_count: i64,
}



pub(crate) fn pause_active_attempts(db: &Connection, work_id: &str, now: i64) -> Result<(), RepositoryError> {
    let attempts = {
        let mut query = db.prepare(
            "SELECT t.task_id FROM story_tasks_v2 t JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id \
             JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 \
             AND t.status IN ('queued','running','waiting')",
        ).map_err(storage_error)?;
        query.query_map([work_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?.collect::<Result<Vec<_>, _>>().map_err(storage_error)?
    };
    for id in attempts {
        db.execute(
            "UPDATE story_tasks_v2 SET status='cancelled',stage='cancelled',error='author_paused', \
             delivery_status=CASE WHEN output_refs_json='[]' THEN 'pending' ELSE 'incomplete' END, \
             capability_revoked_at=?2 WHERE task_id=?1",
            params![id, now],
        ).map_err(storage_error)?;
        finish_attempt(db, &crate::tasks::load_task(db, &id)?)?;
        // The attempt is terminal, while its original step remains eligible
        // for an author-requested retry after input and receipt reconciliation.
        db.execute(
            "UPDATE story_work_steps_v1 SET status='blocked' WHERE work_task_id=?1 \
             AND current_attempt_id=?2 AND status='cancelled'",
            params![work_id, id],
        ).map_err(storage_error)?;
    }
    Ok(())
}

pub(crate) fn finish_attempt(db: &Connection, attempt: &TaskRecord) -> Result<(), RepositoryError> {
    let row: Option<(String, String)> = db.query_row(
        "SELECT s.id, s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id = a.step_id WHERE a.attempt_id = ?1",
        [&attempt.task_id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(storage_error)?;
    let Some((step, parent)) = row else {
        return Err(RepositoryError::CorruptState);
    };
    db.execute("UPDATE story_work_attempts_v1 SET completed_at = unixepoch() WHERE attempt_id = ?1 AND completed_at IS NULL", [&attempt.task_id]).map_err(storage_error)?;
    let step_status = match attempt.status.as_str() {
        "completed" => "completed",
        "cancelled" => "cancelled",
        "failed" => "blocked",
        _ => return Err(RepositoryError::TaskBindingInvalid),
    };
    db.execute(
        "UPDATE story_work_steps_v1 SET status = ?1 WHERE id = ?2 AND current_attempt_id = ?3 AND status != 'superseded'",
        params![step_status, step, attempt.task_id],
    )
    .map_err(storage_error)?;
    db.execute(
        "UPDATE story_work_tasks_v1 SET version = version + 1 WHERE id = ?1",
        [&parent],
    )
    .map_err(storage_error)?;
    if attempt.status == "completed" {
        db.execute("UPDATE story_task_action_requests_v1 SET status = 'completed' WHERE action_id = ?1 AND status = 'running' AND result_task_id = ?2", params![step, attempt.task_id]).map_err(storage_error)?;
    }
    if attempt.status == "failed" {
        db.execute("UPDATE story_task_action_requests_v1 SET status = 'failed', error = ?1 WHERE action_id = ?2 AND status = 'running' AND result_task_id = ?3", params![attempt.error, step, attempt.task_id]).map_err(storage_error)?;
    }
    if attempt.status == "cancelled" {
        db.execute("UPDATE story_task_action_requests_v1 SET status = 'failed', error = COALESCE(?1, '创作任务已取消') WHERE action_id = ?2 AND status = 'running' AND result_task_id = ?3", params![attempt.error, step, attempt.task_id]).map_err(storage_error)?;
    }
    refresh_work_task(db, &parent)?;
    crate::work_decision_generation::observe_terminal_attempt(db, &parent, attempt)?;
    crate::work_execution_events::record_terminal(db, attempt)
}

pub(crate) fn refresh_work_task(db: &Connection, id: &str) -> Result<(), RepositoryError> {
    if crate::work_pause_barrier::reconcile(db, id)? || crate::work_interludes::top(db,id)?.is_some() { return Ok(()); }
    let kind: String = db
        .query_row(
            "SELECT kind FROM story_work_tasks_v1 WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if kind == "generation" {
        return Ok(());
    }
    let status: String = db
        .query_row(
            "SELECT status FROM story_work_tasks_v1 WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if status == "cancelled" || status == "paused" {
        return Ok(());
    }
    crate::task_actions::reconsider_unstarted_planning(db, id)?;
    let (queued, running, blocked, cancelled): (i64, i64, i64, i64) = db.query_row(
        "SELECT SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END), SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), SUM(CASE WHEN status = 'blocked' THEN 1 ELSE 0 END), SUM(CASE WHEN status = 'cancelled' AND NOT (operation='reviewPlanningSeat' AND EXISTS(SELECT 1 FROM story_planning_review_runs_v2 r WHERE r.work_task_id=?1 AND r.source_task_id IS NOT NULL AND r.run_id=json_extract(target_json,'$.reviewRunId') AND r.status='cancelled')) THEN 1 ELSE 0 END) FROM story_work_steps_v1 WHERE work_task_id = ?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).map_err(storage_error)?;
    let review_open: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_planning_review_runs_v2 WHERE work_task_id=?1 AND status='open')",
        [id], |row|row.get(0),
    ).map_err(storage_error)?;
    let next = if cancelled > 0 {
        "cancelled"
    } else if blocked > 0 {
        "blocked"
    } else if queued + running > 0 {
        "running"
    } else if db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=0)",[id],|r|r.get::<_,bool>(/*idx*/0)).map_err(storage_error)? {
        "running"
    } else if review_open {
        "blocked"
    } else {
        "completed"
    };
    db.execute("UPDATE story_work_tasks_v1 SET status = ?1, version = version + 1 WHERE id = ?2 AND status != ?1", params![next, id]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn load_work_task(
    repo: &SqliteStoryRepository,
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<WorkTask, RepositoryError> {
    let (thread_id, goal, kind, status, version, error, control_revision): (String, String, String, String, i64, Option<String>, i64) = db.query_row(
        "SELECT thread_id, goal, kind, status, version, error, control_revision FROM story_work_tasks_v1 WHERE id = ?1 AND project_id = ?2",
        params![id, project.as_str()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)?;
    let mut statement = db.prepare("SELECT id, operation, target_json, status, input_revision, depends_on_step_id, current_attempt_id FROM story_work_steps_v1 WHERE work_task_id = ?1 ORDER BY ordinal").map_err(storage_error)?;
    let steps = statement.query_map([id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?, row.get::<_, Option<String>>(5)?, row.get::<_, Option<String>>(6)?))).map_err(storage_error)?
        .map(|row| {
            let (step_id, operation, target_json, status, input_revision, depends_on_step_id, current_attempt_id) = row.map_err(storage_error)?;
            let mut attempts = db.prepare("SELECT a.attempt_id, a.attempt_number, a.started_at, a.completed_at, t.thread_id, t.turn_id, t.status, t.error FROM story_work_attempts_v1 a JOIN story_tasks_v2 t ON t.task_id = a.attempt_id WHERE a.step_id = ?1 ORDER BY a.attempt_number").map_err(storage_error)?;
            let attempts = attempts.query_map([&step_id], |row| Ok(WorkAttempt { failure_observations: None, id: row.get(0)?, attempt_number: row.get(1)?, started_at: row.get(2)?, completed_at: row.get(3)?, thread_id: row.get(4)?, turn_id: row.get(5)?, status: row.get(6)?, error: row.get(7)? })).map_err(storage_error)?.map(|value| value.map_err(storage_error)).collect::<Result<Vec<_>, _>>()?;
            let output_refs: String = if let Some(attempt_id) = current_attempt_id.as_deref() {
                db.query_row("SELECT output_refs_json FROM story_tasks_v2 WHERE task_id = ?1", [attempt_id], |row| row.get(0)).map_err(storage_error)?
            } else { "[]".into() };
            let instruction: Option<String> = db.query_row("SELECT instruction FROM story_work_decision_actions_v1 WHERE step_id=?1 UNION ALL SELECT instruction FROM story_task_action_requests_v1 WHERE action_id = ?1 UNION ALL SELECT instruction FROM story_media_adaptations_v1 WHERE step_id=?1 LIMIT 1", [&step_id], |row| row.get(0)).optional().map_err(storage_error)?;
            let source_task_id: Option<String> = db.query_row("SELECT source_task_id FROM story_work_decision_actions_v1 WHERE step_id=?1 UNION ALL SELECT source_task_id FROM story_task_action_requests_v1 WHERE action_id = ?1 LIMIT 1", [&step_id], |row| row.get::<_,Option<String>>(/*idx*/0)).optional().map_err(storage_error)?.flatten();
            let mut changes = db.prepare("SELECT c.commit_id, c.operation, c.base_revision, c.result_revision, c.undone_by_commit_id FROM story_acceptances_v2 c JOIN story_task_outputs_v2 o ON o.revision_id = c.accepted_revision_id JOIN story_work_attempts_v1 a ON a.attempt_id = o.task_id WHERE a.step_id = ?1 AND c.project_id = ?2 AND c.operation = 'accept' ORDER BY c.result_revision").map_err(storage_error)?;
            let changes = changes.query_map(params![step_id, project.as_str()], |row| Ok(json!({"commitId": row.get::<_, String>(0)?, "operation": row.get::<_, String>(1)?, "baseRevision": row.get::<_, i64>(2)?, "resultRevision": row.get::<_, i64>(3)?, "undoneByCommitId": row.get::<_, Option<String>>(4)?}))).map_err(storage_error)?.map(|value| value.map_err(storage_error)).collect::<Result<Vec<_>, _>>()?;
            let expected_output_kind = match operation.as_str() {
                "advisory" | "assessChapterStructure" => Some("completionReport".to_owned()),
                "modelKnowledge" => Some("knowledgeBatch".to_owned()),
                "reviewQuality" => Some("qualityReading".to_owned()),
                "writeNextChapter" => Some("draft".to_owned()),
                _ => crate::tasks::operation_contract(&operation)?.2.map(str::to_owned),
            };
            let reading_progress = repo.author_reading_progress(project, &step_id)?;
            Ok(WorkStep { id: step_id, operation, expected_output_kind, instruction, source_task_id, target: serde_json::from_str(&target_json).map_err(|_| RepositoryError::CorruptState)?, status, input_revision, depends_on_step_id, current_attempt_id, attempts, output_refs: serde_json::from_str(&output_refs).map_err(|_| RepositoryError::CorruptState)?, changes, reading_progress })
        }).collect::<Result<Vec<_>, RepositoryError>>()?;
    let mut steps = steps;
    let ledger: Option<String> = db.query_row("SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1", [id], |r|r.get(0)).optional().map_err(storage_error)?;
    let observed_budget = ledger.as_deref().map(serde_json::from_str::<crate::ProviderBudget>).transpose().map_err(|_|RepositoryError::CorruptState)?;
    for step in &mut steps {
        for attempt in &mut step.attempts {
            attempt.failure_observations = observed_budget.as_ref().and_then(|budget| budget.charges.iter()
                .find(|charge| charge.task_id == attempt.id).and_then(|charge| charge.failure_observations.clone()));
        }
    }
    let status = if kind == "generation" {
        crate::work_delivery::generation_status(db, project, id)?
    } else {
        status
    };
    let delivery_report = crate::work_delivery::load(db, project, id, &steps)?;
    let resume_block_reason = if matches!(status.as_str(), "running" | "paused" | "blocked") {
        if kind == "generation" && matches!(status.as_str(), "paused" | "blocked") {
            crate::work_modeling_recovery::pending_reason(db, project, id)?.map(str::to_owned)
                .or(usage_resume_block_reason(db, project, id)?)
        } else if kind != "generation" && matches!(status.as_str(), "paused" | "blocked") {
            resume_block_reason(db, project, id)?
        } else {
            usage_resume_block_reason(db, project, id)?
        }
    } else {
        None
    };
    let resume_mode = crate::work_modeling_recovery::resume_mode(
        db,
        project,
        id,
        &kind,
        &status,
        resume_block_reason.as_deref(),
    )?;
    let mut work = WorkTask {
        id: id.into(),
        project_id: project.to_string(),
        thread_id,
        goal,
        kind,
        status,
        delivery_status: "pending".into(),
        delivery_report,
        version,
        control_revision,
        pause_after_step: crate::work_pause_barrier::read(db, id)?,
        interlude: crate::work_interludes::top(db,id)?,
        error,
        resume_block_reason,
        resume_mode,
        pending_continuation: crate::generation_handoff_execution::pending(db, project, id)?,
        steps,
    };
    work.delivery_status = work.delivery_report.status(db, project, &work)?.into();
    Ok(work)
}

pub(crate) fn assert_inputs_current(db: &Connection, id: &str) -> Result<(), RepositoryError> {
    let latest: Option<String> = db.query_row(
        "SELECT a.attempt_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 ORDER BY a.sequence DESC LIMIT 1",
        [id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    if let Some(latest) = latest {
        let previous = crate::tasks::load_task(db, &latest)?;
        crate::task_input::assert_task_inputs_current(db, &previous)?;
    }
    Ok(())
}

fn usage_resume_block_reason(
    db: &Connection,
    project_id: &ProjectId,
    id: &str,
) -> Result<Option<String>, RepositoryError> {
    let stalled: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_generation_runs_v1 WHERE project_id=?1 AND 'work-'||run_id=?2 AND json_extract(state_json,'$.status')='blocked' AND json_extract(state_json,'$.blockedReason')='recoveryStalled')",
        params![project_id.as_str(),id], |row| row.get(0),
    ).map_err(storage_error)?;
    if stalled {
        let fingerprint = crate::work_decision::recovery_fingerprint(db, id)?;
        if crate::work_decision::unchanged_failures(db, id, &fingerprint)? >= 2 { return Ok(Some("recoveryStalled".into())); }
    }
    Ok((observed_budget_block_reason(db, project_id, id)?
        == Some(crate::provider_budget::ProviderContinuationBlock::UnknownUsage))
        .then(|| "unknownUsage".into()))
}

fn observed_budget_block_reason(
    db: &Connection,
    project_id: &ProjectId,
    id: &str,
) -> Result<Option<crate::provider_budget::ProviderContinuationBlock>, RepositoryError> {
    // An unfunded Work remains readable; absence is not a zero-use grant.
    let ledger: Option<String> = db.query_row(
        "SELECT b.ledger_json FROM story_work_budgets_v1 b JOIN story_work_tasks_v1 w ON w.id=b.work_task_id WHERE w.id=?1 AND w.project_id=?2",
        params![id, project_id.as_str()], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let budget = ledger.as_deref().map(serde_json::from_str::<crate::ProviderBudget>)
        .transpose().map_err(|_| RepositoryError::CorruptState)?;
    Ok(budget.and_then(|budget| budget.continuation_block_reason()))
}

fn resume_block_reason(
    db: &Connection,
    project_id: &ProjectId,
    id: &str,
) -> Result<Option<String>, RepositoryError> {
    if let Some(reason) = baseline_resume_block_reason(db, project_id, id)? {
        return Ok(Some(reason));
    }
    usage_resume_block_reason(db, project_id, id)
}

fn baseline_resume_block_reason(
    db: &Connection,
    project_id: &ProjectId,
    id: &str,
) -> Result<Option<String>, RepositoryError> {
    if let Some(reason) = crate::work_modeling_recovery::pending_reason(db, project_id, id)? {
        return Ok(Some(reason.into()));
    }
    let reader: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND operation='reviewQuality' AND json_extract(target_json,'$.workDecisionAction') IS NULL)", [id], |row| row.get(0)).map_err(storage_error)?;
    if reader {
        return Ok(Some("readerRestart".into()));
    }
    if crate::work_modeling_recovery::publication_changed(db, project_id, id)? {
        return Ok(Some("inputChanged".into()));
    }
    // Aggregating frozen terminal reports does not read a new creative input.
    // No new Attempt can be claimed: all persisted review steps are completed.
    if !crate::planning_reviews::eligible_saved_reviews(db, project_id, id)?.is_empty() {
        return Ok(None);
    }
    let pending_intent: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_task_intents_v1 i ON i.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND s.status IN ('queued','blocked') AND i.status='pending')",
        [id], |row| row.get(0),
    ).map_err(storage_error)?;
    if pending_intent {
        return Ok(Some("inputChanged".into()));
    }
    let partial_output: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id = s.id JOIN story_tasks_v2 t ON t.task_id = a.attempt_id WHERE s.work_task_id = ?1 AND s.status = 'blocked' AND (json_array_length(t.output_refs_json) > 0 OR (t.operation!='modelKnowledge' AND EXISTS(SELECT 1 FROM story_task_outputs_v2 o WHERE o.task_id=t.task_id))))",
        [id], |row| row.get(0),
    ).map_err(storage_error)?;
    if partial_output {
        return Ok(Some("partialOutput".into()));
    }
    let failures = {
        let mut query = db.prepare("SELECT t.task_id,t.error FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND s.status='blocked' AND s.operation!='advisory' AND t.status='failed'").map_err(storage_error)?;
        query
            .query_map([id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?
    };
    let recoverable_publications =
        crate::work_modeling_recovery::eligible_attempts(db, project_id, id)?;
    for (task, error) in failures {
        if crate::conversation_receipts::failure_code(error.as_deref()).is_some()
            && !recoverable_publications.contains(&task)
            && !crate::work_decision_status::retryable(db, &task)?
        {
            return Ok(Some(observed_budget_block_reason(db, project_id, id)?
                .map(|reason| reason.as_str())
                .unwrap_or("executionBlocked").into()));
        }
    }
    match assert_inputs_current(db, id) {
        Err(RepositoryError::TaskInputStale) => return Ok(Some("inputChanged".into())),
        Err(error) => return Err(error),
        Ok(()) => {}
    }
    let current: i64 = db
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    let changed_input: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id = ?1 AND status IN ('queued','blocked') AND input_revision != ?2)",
        params![id, current], |row| row.get(0),
    ).map_err(storage_error)?;
    Ok(changed_input.then(|| "inputChanged".into()))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public work tasks capability; SQLite retains transaction ownership.
pub trait WorkTasksRepository {
    fn work_task_for_attempt(
        &self,
        project_id: &ProjectId,
        attempt_id: &str,
    ) -> Result<WorkTask, RepositoryError>;
    fn work_task(&self, project_id: &ProjectId, id: &str) -> Result<WorkTask, RepositoryError>;
    fn work_tasks_page(
        &self,
        project_id: &ProjectId,
        cursor: Option<&str>,
    ) -> Result<WorkTaskPage, RepositoryError>;
    fn control_work_task(
        &mut self,
        project_id: &ProjectId,
        id: &str,
        command: &str,
        expected_control_revision: i64,
        now: i64,
    ) -> Result<WorkTask, RepositoryError>;
}

impl WorkTasksRepository for SqliteStoryRepository {
    fn work_task_for_attempt(
        &self,
        project_id: &ProjectId,
        attempt_id: &str,
    ) -> Result<WorkTask, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let id: String = read.query_row(
            "SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id = a.step_id JOIN story_work_tasks_v1 t ON t.id = s.work_task_id WHERE a.attempt_id = ?1 AND t.project_id = ?2",
            params![attempt_id, project_id.as_str()], |row| row.get(0),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)?;
        let task = load_work_task(self, &read, project_id, &id)?;
        read.commit().map_err(storage_error)?;
        Ok(task)
    }

    fn work_task(&self, project_id: &ProjectId, id: &str) -> Result<WorkTask, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let task = load_work_task(self, &read, project_id, id)?;
        read.commit().map_err(storage_error)?;
        Ok(task)
    }

    fn work_tasks_page(
        &self,
        project_id: &ProjectId,
        cursor: Option<&str>,
    ) -> Result<WorkTaskPage, RepositoryError> {
        let position = cursor
            .map(|value| {
                let parts = value.split(':').collect::<Vec<_>>();
                let [priority, rowid, count, version_sum] = parts.as_slice() else {
                    return Err(RepositoryError::InvalidContent(
                        "invalid work task cursor".into(),
                    ));
                };
                let priority = priority
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value <= 1 && *value >= 0);
                let rowid = rowid.parse::<i64>().ok().filter(|value| *value > 0);
                let count = count.parse::<i64>().ok().filter(|value| *value >= 0);
                let version_sum = version_sum.parse::<i64>().ok().filter(|value| *value >= 0);
                priority
                    .zip(rowid)
                    .zip(count)
                    .zip(version_sum)
                    .map(|(((priority, rowid), count), version_sum)| {
                        (priority, rowid, count, version_sum)
                    })
                    .ok_or_else(|| {
                        RepositoryError::InvalidContent("invalid work task cursor".into())
                    })
            })
            .transpose()?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let (task_count, version_sum): (i64, i64) = read.query_row(
            "SELECT COUNT(*), COALESCE(SUM(version), 0) FROM story_work_tasks_v1 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(storage_error)?;
        if position.is_some_and(|(_, _, count, sum)| count != task_count || sum != version_sum) {
            return Err(RepositoryError::WorkTaskStale);
        }
        let active_count = read.query_row(
            "SELECT COUNT(*) FROM story_work_tasks_v1 WHERE project_id = ?1 AND status IN ('running','paused','blocked')",
            [project_id.as_str()], |row| row.get(0),
        ).map_err(storage_error)?;
        let mut statement = read.prepare(
            "WITH ranked AS (SELECT id, rowid AS position, CASE WHEN status IN ('running','paused','blocked') THEN 0 ELSE 1 END AS priority FROM story_work_tasks_v1 WHERE project_id = ?1) SELECT id, position, priority FROM ranked WHERE ?2 IS NULL OR priority > ?2 OR (priority = ?2 AND position < ?3) ORDER BY priority, position DESC LIMIT 31",
        ).map_err(storage_error)?;
        let mut rows = statement
            .query_map(
                params![
                    project_id.as_str(),
                    position.map(|value| value.0),
                    position.map(|value| value.1)
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let has_more = rows.len() > 30;
        rows.truncate(30);
        let next_cursor = if has_more {
            rows.last().map(|(_, position, priority)| {
                format!("{priority}:{position}:{task_count}:{version_sum}")
            })
        } else {
            None
        };
        drop(statement);
        let tasks = rows
            .into_iter()
            .map(|(id, _, _)| load_work_task(self, &read, project_id, &id))
            .collect::<Result<Vec<_>, _>>()?;
        read.commit().map_err(storage_error)?;
        Ok(WorkTaskPage {
            tasks,
            next_cursor,
            active_count,
        })
    }

    fn control_work_task(
        &mut self,
        project_id: &ProjectId,
        id: &str,
        command: &str,
        expected_control_revision: i64,
        now: i64,
    ) -> Result<WorkTask, RepositoryError> {
        let tx = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        let (status, control_revision, kind): (String, i64, String) = tx.query_row(
            "SELECT status, control_revision, kind FROM story_work_tasks_v1 WHERE id = ?1 AND project_id = ?2",
            params![id, project_id.as_str()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)?;
        if kind == "generation" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if control_revision != expected_control_revision {
            return Err(RepositoryError::WorkTaskStale);
        }
        crate::work_pause_barrier::clear(&tx, id)?;
        let next = match (command, status.as_str()) {
            ("pause", "running" | "blocked") => "paused",
            ("resume", "paused" | "blocked") => {
                let reason = resume_block_reason(&tx, project_id, id)?;
                if reason.as_deref() == Some("unknownUsage") {
                    return Err(RepositoryError::WorkUsageUnknown);
                }
                if reason.as_deref() == Some("budgetExhausted") {
                    return Err(RepositoryError::WorkBudgetExhausted);
                }
                if reason.is_some() {
                    return Err(RepositoryError::TaskInputStale);
                }
                "running"
            }
            ("recoverSavedResult", "paused" | "blocked") => {
                if crate::work_modeling_recovery::resume_mode(
                    &tx,
                    project_id,
                    id,
                    &kind,
                    &status,
                    resume_block_reason(&tx, project_id, id)?.as_deref(),
                )? != Some(WorkResumeMode::RecoverSavedResult)
                {
                    return Err(RepositoryError::TaskInputStale);
                }
                "paused"
            }
            ("cancel", "running" | "paused" | "blocked") => {
                tx.execute("UPDATE story_planning_review_runs_v2 SET status='cancelled',updated_at=?2 WHERE work_task_id=?1 AND status='open'", params![id,now]).map_err(storage_error)?;
                tx.execute("UPDATE story_task_action_requests_v1 SET status = 'failed', error = '创作任务已取消' WHERE action_id IN (SELECT id FROM story_work_steps_v1 WHERE work_task_id = ?1) AND status IN ('queued','running')", [id]).map_err(storage_error)?;
                tx.execute("UPDATE story_work_steps_v1 SET status = 'cancelled' WHERE work_task_id = ?1 AND status IN ('queued','blocked','running')", [id]).map_err(storage_error)?;
                tx.execute(
                    "UPDATE story_tasks_v2 SET status = 'cancelled', stage = 'cancelled', delivery_status = CASE WHEN output_refs_json = '[]' THEN 'pending' ELSE 'incomplete' END, capability_revoked_at = ?2 WHERE task_id IN (SELECT a.attempt_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id = a.step_id WHERE s.work_task_id = ?1) AND status IN ('queued','running','waiting')",
                    params![id, now],
                ).map_err(storage_error)?;
                tx.execute("UPDATE story_work_attempts_v1 SET completed_at = ?2 WHERE step_id IN (SELECT id FROM story_work_steps_v1 WHERE work_task_id = ?1) AND completed_at IS NULL", params![id, now]).map_err(storage_error)?;
                "cancelled"
            }
            _ => return Err(RepositoryError::TaskNotWritable),
        };
        let changed = tx.execute(
            "UPDATE story_work_tasks_v1 SET status = ?1, version = version + 1, control_revision = control_revision + 1 WHERE id = ?2 AND control_revision = ?3",
            params![next, id, expected_control_revision],
        ).map_err(storage_error)?;
        if changed != 1 {
            return Err(RepositoryError::WorkTaskStale);
        }
        if command == "pause" {
            // Pause is a durable write fence, not merely a scheduler flag.
            // Ending the old attempts in this transaction prevents a later
            // resume from restoring their capability. Receipts and budgets
            // remain owned by the original attempts and are reconciled first.
            pause_active_attempts(&tx, id, now)?;
        }
        if matches!(command, "pause" | "cancel") {
            crate::work_interludes::invalidate(&tx, id)?;
        }
        if command == "resume" {
            crate::work_modeling_recovery::recover_published_steps(&tx, project_id, id, now)?;
            let blocked = {
                let mut query = tx.prepare("SELECT s.id,s.operation,t.status,t.error FROM story_work_steps_v1 s LEFT JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id WHERE s.work_task_id=?1 AND s.status='blocked'").map_err(storage_error)?;
                query
                    .query_map([id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    })
                    .map_err(storage_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(storage_error)?
            };
            for (step, operation, task_status, error) in blocked {
                if !matches!(operation.as_str(), "advisory" | "reviewPlanningSeat")
                    && task_status.as_deref() == Some("failed")
                    && crate::conversation_receipts::failure_code(error.as_deref()).is_some()
                {
                    // Retain the failed Attempt for the same-Work model decision.
                    // Author resume does not select a correction or requeue its writer.
                    // Planning seats are consumed by their dedicated Host primitive.
                    continue;
                }
                tx.execute("UPDATE story_task_action_requests_v1 SET status='queued',result_task_id=NULL,error=NULL WHERE action_id=?1", [&step]).map_err(storage_error)?;
                tx.execute("UPDATE story_work_steps_v1 SET status='queued',current_attempt_id=NULL WHERE id=?1", [&step]).map_err(storage_error)?;
            }
            refresh_work_task(&tx, id)?;
        }
        if command == "recoverSavedResult" {
            crate::planning_reviews::recover_saved_reviews(&tx, project_id, id, now)?;
            crate::work_modeling_recovery::recover_published_steps(&tx, project_id, id, now)?;
            // Recovery is not authorization to run another step, including a
            // follow-up queued by finish_attempt. Leave that work author-paused.
            tx.execute(
                "UPDATE story_work_tasks_v1 SET status='running' WHERE id=?1 AND NOT EXISTS \
                 (SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND status IN ('queued','running','blocked')) \
                 AND NOT EXISTS(SELECT 1 FROM story_work_decisions_v1 WHERE work_task_id=?1 AND finished=0) \
                 AND NOT EXISTS(SELECT 1 FROM story_planning_review_runs_v2 WHERE work_task_id=?1 AND status='open')",
                [id],
            ).map_err(storage_error)?;
            refresh_work_task(&tx, id)?;
        }
        let task = load_work_task(self, &tx, project_id, id)?;
        tx.commit()?;
        Ok(task)
    }
}
