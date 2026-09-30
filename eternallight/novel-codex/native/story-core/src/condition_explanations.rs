use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{ConditionEvaluationStatus, load, storage};
use crate::{
    ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository,
    event_rule_evaluation::ConditionProgress,
};

#[path = "condition_excerpts.rs"]
mod excerpts;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionExplanationPage {
    pub data: Vec<Value>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    project: String,
    job: String,
    version: i64,
    binding: String,
    offset: usize,
}

pub(super) fn checkpoint(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<ConditionProgress, RepositoryError> {
    let size:i64=db.query_row("SELECT length(CAST(progress_json AS BLOB)) FROM story_condition_evaluations_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|row|row.get(0)).map_err(storage)?;
    if !(0..=8 * 1024 * 1024).contains(&size) {
        return Err(RepositoryError::CorruptState);
    }
    let (encoded,digest):(String,String)=db.query_row("SELECT progress_json,progress_digest FROM story_condition_evaluations_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(storage)?;
    if format!("{:x}", Sha256::digest(encoded.as_bytes())) != digest {
        return Err(RepositoryError::CorruptState);
    }
    serde_json::from_str(&encoded).map_err(|_| RepositoryError::CorruptState)
}

impl SqliteStoryRepository {
    /// Read bounded representative comparisons from a completed, still-bound
    /// evaluation. A cursor carries position, never authorization.
    pub fn condition_explanations(
        &self,
        project: &ProjectId,
        id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ConditionExplanationPage, RepositoryError> {
        if !(1..=16).contains(&limit) || cursor.is_some_and(|value| value.len() > 2048) {
            return Err(RepositoryError::InvalidContent(
                "invalid condition explanation page".into(),
            ));
        }
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let job = load(&tx, project, id)?;
        if job.status != ConditionEvaluationStatus::Completed {
            return Err(RepositoryError::TaskNotWritable);
        }
        let offset = match cursor {
            None => 0,
            Some(encoded) => {
                let cursor: Cursor = serde_json::from_str(encoded).map_err(|_| {
                    RepositoryError::InvalidContent("invalid explanation cursor".into())
                })?;
                if cursor.project != project.as_str()
                    || cursor.job != id
                    || cursor.version != job.version
                    || cursor.binding != job.binding_digest
                {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                cursor.offset
            }
        };
        let rows = checkpoint(&tx, project, id)?.explanations()?;
        if offset > rows.len() {
            return Err(RepositoryError::InvalidContent(
                "explanation cursor out of range".into(),
            ));
        }
        let mut data = Vec::new();
        let mut bytes = 0;
        let snapshot = job
            .target
            .snapshot_id
            .as_ref()
            .map(|id| SnapshotId::parse(id.clone()))
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?;
        let scope = snapshot.as_ref().map_or(
            crate::model_sources::ModelSourceScope::Current,
            crate::model_sources::ModelSourceScope::Snapshot,
        );
        for row in rows.iter().skip(offset).take(limit) {
            let mut row = row.clone();
            row["excerpts"] = json!({
                "support":excerpts::excerpt(&tx, project, scope, &row["assessment"]["support"])?,
                "refute":excerpts::excerpt(&tx, project, scope, &row["assessment"]["refute"])?,
            });
            let size = row.to_string().len();
            if bytes + size > 6000 {
                break;
            }
            bytes += size;
            data.push(row);
        }
        if data.is_empty() && offset < rows.len() {
            return Err(RepositoryError::CorruptState);
        }
        let next = offset + data.len();
        let next_cursor = if next < rows.len() {
            Some(
                serde_json::to_string(&Cursor {
                    project: project.to_string(),
                    job: id.into(),
                    version: job.version,
                    binding: job.binding_digest,
                    offset: next,
                })
                .map_err(|_| RepositoryError::CorruptState)?,
            )
        } else {
            None
        };
        let page = ConditionExplanationPage { data, next_cursor };
        if json!(page).to_string().len() > 9000 {
            return Err(RepositoryError::CorruptState);
        }
        tx.commit().map_err(storage)?;
        Ok(page)
    }
}
