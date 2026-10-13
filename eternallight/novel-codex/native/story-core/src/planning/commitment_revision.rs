use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

pub(crate) fn validate_target(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<(), RepositoryError> {
    if target["type"] != "commitment" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let id = target["commitmentId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 120)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let revision = target["revisionId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let selected: Option<String> = db.query_row(
        "SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_object_heads_v2 WHERE project_id=?1 AND object_id=?2 AND object_kind='commitment'",
        params![project.as_str(),format!("commitment-{id}")], |row|row.get(0),
    ).optional().map_err(storage)?;
    if selected.as_deref() != Some(revision) {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

fn content(
    db: &Connection,
    project: &ProjectId,
    id: &str,
    revision: &str,
) -> Result<Value, RepositoryError> {
    let raw: String = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3 AND object_kind='commitment'",
        params![project.as_str(),format!("commitment-{id}"),revision], |row|row.get(0),
    ).optional().map_err(storage)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    if raw.len() > 32_000 {
        return Err(RepositoryError::CorruptState);
    }
    serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn validate_write(
    db: &Connection,
    task: &TaskRecord,
    raw: &str,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let id = target["commitmentId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let revision = target["revisionId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let base = content(db, &task.project_id, id, revision)?;
    let output: Value =
        serde_json::from_str(raw).map_err(|_| RepositoryError::TaskOutputForbidden)?;
    if output["payload"]["id"] != id || output["extractionOrigin"] != base["extractionOrigin"] {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    Ok(())
}

/// Public commitment revision capability; SQLite retains transaction ownership.
pub trait CommitmentRevisionRepository {
    /// Derive immutable origin for submission; save_candidate remains the write
    /// authority and resolves idempotent receipts before checking current inputs.
    fn commitment_revision_origin(&self, task_id: &str, id: &str)
    -> Result<Value, RepositoryError>;
    /// Read one commitment under an active task's captured input watermarks.
    /// Revision tasks keep their immutable base even after their own checkpoints.
    fn task_commitment(&self, task_id: &str, id: &str) -> Result<Value, RepositoryError>;
}

impl CommitmentRevisionRepository for SqliteStoryRepository {
    /// Derive immutable origin for submission; save_candidate remains the write
    /// authority and resolves idempotent receipts before checking current inputs.
    fn commitment_revision_origin(
        &self,
        task_id: &str,
        id: &str,
    ) -> Result<Value, RepositoryError> {
        let task = crate::tasks::load_task(&self.connection, task_id)?;
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        if task.operation != "reviseCommitment" || target["commitmentId"] != id {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let revision = target["revisionId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        Ok(content(&self.connection, &task.project_id, id, revision)?["extractionOrigin"].clone())
    }

    /// Read one commitment under an active task's captured input watermarks.
    /// Revision tasks keep their immutable base even after their own checkpoints.
    fn task_commitment(&self, task_id: &str, id: &str) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let task = crate::tasks::load_task(&self.connection, task_id)?;
            if !matches!(task.status.as_str(), "queued" | "running" | "waiting") {
                return Err(RepositoryError::TaskNotWritable);
            }
            crate::task_input::assert_task_inputs_current(&self.connection, &task)?;
            let target: Value = serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
            let revision = match task.operation.as_str() {
                "advisory" if target["type"] == "project" => self.connection.query_row(
                    "SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_task_input_heads_v2 WHERE task_id=?1 AND object_id=?2",
                    params![task_id,format!("commitment-{id}")], |row|row.get::<_, String>(0),
                ).optional().map_err(storage)?.ok_or(RepositoryError::TaskBindingInvalid)?,
                "reviseCommitment" if target["commitmentId"] == id => target["revisionId"].as_str().ok_or(RepositoryError::TaskBindingInvalid)?.to_owned(),
                _ => return Err(RepositoryError::TaskOutputForbidden),
            };
            let base = content(&self.connection, &task.project_id, id, &revision)?;
            Ok(json!({"revisionId":revision,"payload":base["payload"],"extractionOrigin":base["extractionOrigin"]}))
        })?
    }
}

use crate::ReadSnapshotRepository;
