//! Immutable author-confirmed generation bounds in the shared execution-scope registry.
//! Current progress and replanning inputs do not authorize a larger scope or allowance.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::execution_scope::scope_digest;
use crate::{ProjectId, RepositoryError};

// A projection of run state; required fields have no defaults.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    start_chapter_number: i64,
    target_chapters: i64,
    max_task_attempts: i64,
    deadline_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationScope {
    preview_digest: String,
    project_id: String,
    work_task_id: String,
    run_id: String,
    author_message_id: String,
    initial_snapshot_id: String,
    initial_source_digest: String,
    bounds: Bounds,
    max_requests: u64,
    max_tokens: u64,
}

pub(crate) fn record(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
    confirmed_at: i64,
) -> Result<(), RepositoryError> {
    let work = format!("work-{run}");
    let budget = crate::work_budget_storage::load(db, project, &work)?;
    let mut scope = GenerationScope {
        preview_digest: String::new(),
        project_id: project.to_string(),
        work_task_id: work,
        run_id: run.into(),
        author_message_id: serde_json::from_value(state["authorMessageId"].clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?,
        initial_snapshot_id: serde_json::from_value(state["baseSnapshotId"].clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?,
        initial_source_digest: serde_json::from_value(state["sourceDigest"].clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?,
        bounds: serde_json::from_value(state.clone())
            .map_err(|_| RepositoryError::TaskBindingInvalid)?,
        max_requests: budget.max_requests,
        max_tokens: budget.max_tokens,
    };
    scope.preview_digest =
        scope_digest(serde_json::to_value(&scope).map_err(|_| RepositoryError::CorruptState)?)?;
    db.execute("INSERT INTO story_execution_scopes_v1(permit_id,work_task_id,kind,author_message_id,generation_run_id,scope_json,preview_digest,confirmed_at,status) VALUES(?1,?2,'generation',?3,?4,?5,?6,?7,'active')",
        params![format!("scope-{}",scope.preview_digest),scope.work_task_id,scope.author_message_id,run,serde_json::to_string(&scope).map_err(|_| RepositoryError::CorruptState)?,scope.preview_digest,confirmed_at]).map_err(storage_error)?;
    validate(db, project, run, state)?;
    Ok(())
}

type StoredScope = (String, String, String, String, i64);

/// Returns the frozen cumulative attempt limit after validating scope and account ownership.
pub(crate) fn validate(
    db: &Connection,
    project: &ProjectId,
    run: &str,
    state: &Value,
) -> Result<i64, RepositoryError> {
    let work = format!("work-{run}");
    let (id,payload,digest,message,confirmed_at): StoredScope = db.query_row(
        "SELECT e.permit_id,e.scope_json,e.preview_digest,e.author_message_id,e.confirmed_at
         FROM story_execution_scopes_v1 e JOIN story_work_tasks_v1 w ON w.id=e.work_task_id
         JOIN story_author_messages_v1 m ON m.message_id=e.author_message_id AND m.work_task_id=w.id AND m.project_id=w.project_id AND m.generation_run_id=e.generation_run_id AND m.intent_revision=0 AND m.created_at=e.confirmed_at
         WHERE e.generation_run_id=?1 AND e.work_task_id=?2 AND w.project_id=?3 AND w.kind='generation' AND e.kind='generation' AND e.source_task_id IS NULL AND e.status='active' AND e.superseded_by_author_message_id IS NULL",
        params![run,work,project.as_str()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let scope: GenerationScope =
        serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
    let budget = crate::work_budget_storage::load(db, project, &work)?;
    let current: Bounds =
        serde_json::from_value(state.clone()).map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let snapshot_owned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_snapshots_v2 WHERE snapshot_id=?1 AND project_id=?2)",params![scope.initial_snapshot_id,project.as_str()],|row|row.get(0)).map_err(storage_error)?;
    if id != format!("scope-{digest}")
        || scope.preview_digest != digest
        || scope_digest(serde_json::to_value(&scope).map_err(|_| RepositoryError::CorruptState)?)?
            != digest
        || scope.project_id != project.as_str()
        || scope.work_task_id != work
        || scope.run_id != run
        || scope.author_message_id != message
        || state["authorMessageId"] != message
        || current != scope.bounds
        || budget.max_requests != scope.max_requests
        || budget.max_tokens != scope.max_tokens
        || !snapshot_owned
        || scope.initial_source_digest.len() != 64
        || !scope
            .initial_source_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || scope.bounds.start_chapter_number < 1
        || scope.bounds.target_chapters < scope.bounds.start_chapter_number
        || scope.bounds.target_chapters - scope.bounds.start_chapter_number > 2
        || !(1..=32).contains(&scope.bounds.max_task_attempts)
        || scope.max_requests < scope.bounds.max_task_attempts as u64
        || scope.max_requests > 2048
        || !(1..=32_000_000).contains(&scope.max_tokens)
        || confirmed_at < 0
        || scope.bounds.deadline_at <= confirmed_at
        || scope.bounds.deadline_at.saturating_sub(confirmed_at) > 86_400
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(scope.bounds.max_task_attempts)
}
