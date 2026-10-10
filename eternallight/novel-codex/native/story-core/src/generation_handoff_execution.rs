//! Persisted execution authority for a model-selected continuation. The first
//! run transition and its receipt share one transaction; retries never guess
//! authority from a changed version or from the assistant's prose.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

pub(crate) const SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS story_generation_handoff_executions_v1 (
 proposal_id TEXT PRIMARY KEY REFERENCES story_generation_handoffs_v1(proposal_id),
 run_id TEXT NOT NULL REFERENCES story_generation_runs_v1(run_id),
 expected_version INTEGER NOT NULL, started_version INTEGER NOT NULL,
 binding_json TEXT NOT NULL);";

fn invalid(detail: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("generationHandoff: {detail}"))
}

// Progress, provider charges and source checkpoints may change while the same
// authorized run executes. Author intent, scope and allowance identity may not.
fn execution_binding(run: &Value) -> Value {
    json!({"id":run["id"],"projectId":run["projectId"],"rootThreadId":run["rootThreadId"],
        "authorMessageId":run["authorMessageId"],"intentGeneration":run["intentGeneration"],
        "intentRevisions":run["intentRevisions"],"startChapterNumber":run["startChapterNumber"],
        "targetChapters":run["targetChapters"],"maxTaskAttempts":run["maxTaskAttempts"],
        "deadlineAt":run["deadlineAt"],"maxRetryAttempts":run["maxRetryAttempts"],
        "maxProviderRequests":run["providerBudget"]["maxRequests"],
        "maxProviderTokens":run["providerBudget"]["maxTokens"]})
}

fn proposal(db: &Connection, project: &ProjectId, source: &str) -> Result<Value, RepositoryError> {
    let row: Option<(String, String, String, String, String)> = db.query_row(
        "SELECT h.content_json,t.status,t.operation,t.thread_id,t.target_json FROM story_generation_handoffs_v1 h JOIN story_tasks_v2 t ON t.task_id=h.source_task_id WHERE t.project_id=?1 AND t.task_id=?2",
        params![project.as_str(), source], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    ).optional().map_err(storage_error)?;
    let (encoded, status, operation, thread, target) =
        row.ok_or(RepositoryError::TaskBindingInvalid)?;
    let value: Value = serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    let target: Value = serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
    if operation != "advisory"
        || status != "completed"
        || value["sourceTaskId"] != source
        || value["projectId"] != project.as_str()
        || value["threadId"] != thread
        || value["resumeRun"]["id"].as_str().is_none()
        || target.get("revisionRepairSource").is_some()
        || target.get("runId").is_some()
        || target["conversationFocus"]["readOnly"] == true
        || target["readOnly"] == true
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(value)
}

fn fulfilled_prerequisites(
    db: &Connection,
    project: &ProjectId,
    source: &str,
) -> Result<(), RepositoryError> {
    let (work,status,thread): (String,String,String) = db.query_row(
        "SELECT w.id,w.status,w.thread_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.current_attempt_id=a.attempt_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2",
        params![source,project.as_str()],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    crate::work_pause_barrier::assert_can_schedule(db, &work)?;
    let saved = proposal(db, project, source)?;
    let reviewed: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_delivery_reviews_v1 WHERE work_task_id=?1 AND decision_task_id=?2 AND json_extract(content_json,'$.goalStatus')='fulfilled')",
        params![work,source],|r|r.get(0),
    ).map_err(storage_error)?;
    let pending: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 WHERE work_task_id=?1 AND status IN ('queued','running','waiting')) OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0) OR EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_task_action_requests_v1 WHERE source_task_id=?2)",
        params![work,source],|r|r.get(0),
    ).map_err(storage_error)?;
    if status != "completed" || saved["threadId"] != thread || !reviewed || pending {
        return Err(invalid(
            "prerequisite Work must be fulfilled with every write and usage receipt settled",
        ));
    }
    Ok(())
}

pub(crate) fn eligibility(
    db: &Connection,
    project: &ProjectId,
    source: &str,
) -> Result<Value, RepositoryError> {
    let proposal = proposal(db, project, source)?;
    crate::task_intents::assert_action_intent(
        db,
        source,
        proposal["intentRevision"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?,
    )?;
    fulfilled_prerequisites(db, project, source)?;
    let id = proposal["proposalId"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let receipt: Option<(String,i64,i64,String)> = db.query_row(
        "SELECT run_id,expected_version,started_version,binding_json FROM story_generation_handoff_executions_v1 WHERE proposal_id=?1",
        [id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
    ).optional().map_err(storage_error)?;
    let run_id = proposal["resumeRun"]["id"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let encoded: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",
            params![project.as_str(), run_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut run: Value =
        serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)?;
    // Provider authority lives in its single durable Work ledger, rather than
    // in serialized run state.
    run["providerBudget"] = serde_json::to_value(crate::work_budget_storage::load(
        db,
        project,
        &format!("work-{run_id}"),
    )?)
    .map_err(|_| RepositoryError::CorruptState)?;
    if run["projectId"] != proposal["projectId"]
        || run["rootThreadId"] != proposal["threadId"]
        || run["targetChapters"] != proposal["targetChapters"]
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if let Some((consumed_run, expected, started, binding)) = receipt {
        let binding: Value =
            serde_json::from_str(&binding).map_err(|_| RepositoryError::CorruptState)?;
        if consumed_run != run_id
            || proposal["resumeRun"]["expectedVersion"] != expected
            || run["version"].as_i64().is_none_or(|v| v < started)
            || execution_binding(&run) != binding
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        return Ok(
            json!({"proposal":proposal,"receipt":{"runId":run_id,"expectedVersion":expected,"startedVersion":started},"shouldExecute":run["status"]=="running"}),
        );
    }
    if run["version"] != proposal["resumeRun"]["expectedVersion"] {
        return Err(RepositoryError::AuthoringRunStale);
    }
    let unknown:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 o JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0)",[format!("work-{run_id}")],|r|r.get(0)).map_err(storage_error)?;
    if unknown {
        return Err(RepositoryError::TaskNotWritable);
    }
    let revision: i64 = db
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
            [project.as_str()],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    let snapshot:String=db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project.as_str()],|r|r.get(0)).map_err(storage_error)?;
    if proposal["expectedRevision"] != revision
        || proposal["baseSnapshotId"] != snapshot
        || proposal["sourceDigest"] != crate::task_input::project_source_digest(db, project)?
    {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(json!({"proposal":proposal,"receipt":null,"shouldExecute":true}))
}

/// Executed inside the same transaction that updates the run and its Work.
pub(crate) fn record_start(
    db: &Connection,
    project: &ProjectId,
    source: &str,
    run_id: &str,
    expected: i64,
    next: &Value,
) -> Result<(), RepositoryError> {
    let admission = eligibility(db, project, source)?;
    let proposal = &admission["proposal"];
    if !admission["receipt"].is_null()
        || proposal["resumeRun"]["id"] != run_id
        || proposal["resumeRun"]["expectedVersion"] != expected
        || next["version"] != expected + 1
        || next["status"] != "running"
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let before: String = db.query_row("SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2 AND version=?3",params![project.as_str(),run_id,expected],|r|r.get(0)).map_err(storage_error)?;
    let mut before: Value =
        serde_json::from_str(&before).map_err(|_| RepositoryError::CorruptState)?;
    before["providerBudget"] = serde_json::to_value(crate::work_budget_storage::load(
        db,
        project,
        &format!("work-{run_id}"),
    )?)
    .map_err(|_| RepositoryError::CorruptState)?;
    if execution_binding(&before) != execution_binding(next) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let changed=db.execute("UPDATE story_generation_handoffs_v1 SET consumed_run_id=?2 WHERE proposal_id=?1 AND consumed_run_id IS NULL",params![proposal["proposalId"].as_str(),run_id]).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    db.execute("INSERT INTO story_generation_handoff_executions_v1(proposal_id,run_id,expected_version,started_version,binding_json) VALUES(?1,?2,?3,?4,?5)",params![proposal["proposalId"].as_str(),run_id,expected,expected+1,execution_binding(next).to_string()]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn pending(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<bool, RepositoryError> {
    let source:Option<String>=db.query_row("SELECT h.source_task_id FROM story_generation_handoffs_v1 h JOIN story_work_steps_v1 s ON s.current_attempt_id=h.source_task_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE w.project_id=?1 AND w.id=?2 AND w.status='completed' AND json_extract(h.content_json,'$.resumeRun.id') IS NOT NULL ORDER BY s.ordinal DESC LIMIT 1",params![project.as_str(),work],|r|r.get(0)).optional().map_err(storage_error)?;
    let Some(source) = source else {
        return Ok(false);
    };
    match eligibility(db, project, &source) {
        Ok(value) => Ok(value["shouldExecute"] == true),
        Err(
            RepositoryError::TaskBindingInvalid
            | RepositoryError::AuthoringRunStale
            | RepositoryError::TaskInputStale
            | RepositoryError::TaskNotWritable
            | RepositoryError::InvalidContent(_),
        ) => Ok(false),
        Err(error) => Err(error),
    }
}

impl SqliteStoryRepository {
    pub fn generation_handoff_execution(
        &self,
        project: &ProjectId,
        source: &str,
    ) -> Result<Value, RepositoryError> {
        eligibility(&self.connection, project, source)
    }
    /// Recovery reads the Native proposal, never a remembered Host result.
    pub fn work_generation_handoff(
        &self,
        project: &ProjectId,
        work: &str,
    ) -> Result<Option<Value>, RepositoryError> {
        self.work_task(project, work)?;
        let row:Option<(String,String)>=self.connection.query_row("SELECT h.source_task_id,h.content_json FROM story_generation_handoffs_v1 h JOIN story_tasks_v2 t ON t.task_id=h.source_task_id JOIN story_work_steps_v1 s ON s.current_attempt_id=t.task_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE w.project_id=?1 AND w.id=?2 AND t.status='completed' AND json_extract(h.content_json,'$.resumeRun.id') IS NOT NULL ORDER BY s.ordinal DESC LIMIT 1",params![project.as_str(),work],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
        row.map(|(source,encoded)|{
            let proposal:Value=serde_json::from_str(&encoded).map_err(|_|RepositoryError::CorruptState)?;
            let task=self.task(project,&source)?.ok_or(RepositoryError::TaskNotFound)?;
            let target:Value=serde_json::from_str(&task.target_json).map_err(|_|RepositoryError::CorruptState)?;
            Ok(json!({"proposal":proposal,"task":{"id":task.task_id,"projectId":task.project_id.as_str(),"threadId":task.thread_id,"operation":task.operation,"target":target,"status":task.status}}))
        }).transpose()
    }
}
