//! Historical continuation provenance is not a live candidate-write permission.
//! A delivered author adjustment permits bounded analysis of the same step only.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::execution_scope_storage::ScopeRecord;
use crate::{RepositoryError, TaskRecord};

pub(crate) fn assert_analysis_authority(
    db: &Connection,
    task: &TaskRecord,
    scope: &ScopeRecord,
) -> Result<(), RepositoryError> {
    if scope.status == "active" {
        return crate::execution_scope_storage::assert_live(db, scope);
    }
    let now: i64 = db
        .query_row("SELECT unixepoch()", [], |row| row.get(0))
        .map_err(storage_error)?;
    if task.operation != "advisory" || scope.scope.deadline_at <= now {
        return Err(RepositoryError::TaskInputStale);
    }
    let message = scope
        .superseded_by_author_message_id
        .as_deref()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let (owner, revision): (String, i64) = db
        .query_row(
            "SELECT task_id,intent_revision FROM story_task_intents_v1 WHERE author_message_id=?1",
            [message],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let intent = crate::task_intents::load_intent(db, &owner, revision)?;
    let belongs: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 current JOIN story_work_attempts_v1 cause
         ON cause.attempt_id=?2 AND cause.step_id=current.step_id AND cause.sequence<=current.sequence
         JOIN story_work_steps_v1 s ON s.id=current.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
         WHERE current.attempt_id=?1 AND s.work_task_id=?3 AND w.project_id=?4)",
        params![task.task_id,owner,scope.scope.work_task_id,task.project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    if !belongs || intent.status != "delivered" || intent.delivered_at.is_none() {
        return Err(RepositoryError::TaskInputStale);
    }
    crate::task_intents::assert_no_pending_intent(db, &task.task_id)?;
    let mut source = task.task_id.clone();
    let mut included: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",
            [&source],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    // Validates every frozen binding and message before interpreting its ancestry.
    crate::task_author_intent::source_intent(db, &source, included)?;
    for _ in 0..32 {
        if source == owner {
            return if included >= revision {
                Ok(())
            } else {
                Err(RepositoryError::TaskInputStale)
            };
        }
        let prior = crate::tasks::load_task(db, &source)?;
        let manifest: Value = serde_json::from_str(&prior.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let binding = &manifest["authorIntentBinding"];
        source = binding["sourceTaskId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .to_owned();
        included = binding["intentRevision"]
            .as_i64()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
    }
    Err(RepositoryError::TaskBindingInvalid)
}
