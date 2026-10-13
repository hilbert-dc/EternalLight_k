use rusqlite::{Connection, params};
use serde_json::Value;

use crate::{ProjectId, RepositoryError};

/// Analysis requests preserve the same current chapter/source contract as extraction.
pub(crate) fn validate_target(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<(), RepositoryError> {
    crate::commitment_extraction::validate_target(db, project, target).map(|_| ())
}

/// Only the Host-resolved batch may extend an exact author-approved analysis target.
pub(crate) fn normalize_execution(
    db: &Connection,
    project: &ProjectId,
    input_revision: i64,
    target: &mut Value,
) -> Result<(), RepositoryError> {
    let batch = target
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("batchId")
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let batch = batch.as_str().ok_or(RepositoryError::TaskBindingInvalid)?;
    validate_target(db, project, target)?;
    let valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_modeling_batches_v2 WHERE project_id=?1 AND batch_id=?2 AND object_id='draft-'||?3 AND source_revision_id=?4 AND input_project_revision=?5 AND status IN ('queued','ready'))",
        params![project.as_str(), batch, target["chapterId"].as_str(), target["sourceRevisionId"].as_str(), input_revision],
        |row| row.get(0),
    ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    if !valid {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
