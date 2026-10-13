use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::model_sources::{ModelSourceScope, validated_source};
use crate::story_events::EventClaimRef;
use crate::{ProjectId, RepositoryError};

/// Revalidate the representative claim against its authorized immutable source.
/// Return a Unicode prefix with its own range, never a fabricated full quote.
pub(super) fn excerpt(
    db: &Connection,
    project: &ProjectId,
    scope: ModelSourceScope<'_>,
    reference: &Value,
) -> Result<Value, RepositoryError> {
    if reference.is_null() {
        return Ok(Value::Null);
    }
    let reference: EventClaimRef =
        serde_json::from_value(reference.clone()).map_err(|_| RepositoryError::CorruptState)?;
    let batch = reference
        .source_batch_id
        .as_deref()
        .ok_or(RepositoryError::CorruptState)?;
    let source =
        validated_source(db, project, scope, batch)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let claim = source
        .assertions
        .get(reference.assertion_index)
        .ok_or(RepositoryError::CorruptState)?;
    let original = claim["evidence"]["quote"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let start = claim["evidence"]["start"]
        .as_u64()
        .ok_or(RepositoryError::CorruptState)?;
    let quote = original.chars().take(160).collect::<String>();
    let count = quote.chars().count() as u64;
    let end = start
        .checked_add(count)
        .ok_or(RepositoryError::CorruptState)?;
    let (object, revision): (String, String) = db.query_row(
        "SELECT object_id,source_revision_id FROM story_modeling_batches_v2 WHERE project_id=?1 AND batch_id=?2",
        params![project.as_str(),batch], |row| Ok((row.get(0)?,row.get(1)?)),
    ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    Ok(
        json!({"sourceBatchId":batch,"assertionIndex":reference.assertion_index,
        "sourceObjectId":object,"sourceRevisionId":revision,"quote":quote,
        "start":start,"end":end,"truncated":quote.len()<original.len()}),
    )
}
