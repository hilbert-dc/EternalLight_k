use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::{ConditionEvaluationStatus, ConditionEvaluationTarget, binding, load, storage};
use crate::{ProjectId, RepositoryError};

/// Resolve only an already authorized, exact target. Callers must hold their
/// source read transaction and must not use this for a restricted perspective.
pub(crate) fn resolve(
    db: &Connection,
    project: &ProjectId,
    target: ConditionEvaluationTarget,
    evaluate: impl FnOnce() -> Result<Value, RepositoryError>,
) -> Result<Value, RepositoryError> {
    if db.is_autocommit() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let encoded = serde_json::to_string(&target).map_err(|_| RepositoryError::CorruptState)?;
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_condition_evaluations_v1 WHERE project_id=?1 AND target_json=?2)",params![project.as_str(),encoded],|row|row.get(0)).map_err(storage)?;
    let job = if exists {
        match binding::capture(db, project, &target)? {
            Some(digest) => {
                let id:Option<String>=db.query_row("SELECT id FROM story_condition_evaluations_v1 WHERE project_id=?1 AND target_json=?2 AND binding_digest=?3 ORDER BY rowid DESC LIMIT 1",params![project.as_str(),encoded,digest],|row|row.get(0)).optional().map_err(storage)?;
                id.map(|id| load(db, project, &id)).transpose()?
            }
            None => None,
        }
    } else {
        None
    };
    if let Some(job) = &job
        && job.status == ConditionEvaluationStatus::Completed
    {
        return job.result.clone().ok_or(RepositoryError::CorruptState);
    }
    let mut report = evaluate()?;
    if let Some(job) = job {
        report["evaluationJob"] = json!({"id":job.id,"status":job.status,"version":job.version,"scannedSources":job.scanned_sources});
    }
    if report.to_string().len() > 6144 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(report)
}
