use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{BranchId, ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

#[derive(Serialize, Deserialize, ts_rs::TS)]
#[ts(rename = "CommitmentCatalogCursor")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    project_id: String,
    snapshot_id: String,
    after_object_id: String,
    limit: usize,
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("invalid commitment catalog cursor or limit".into())
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public commitment catalog capability; SQLite retains transaction ownership.
pub trait CommitmentCatalogRepository {
    /// Discovers independent author objects at one immutable snapshot. Page
    /// summaries carry no live write controls; opening an object rechecks them.
    fn narrative_commitments_page(
        &self,
        project: &ProjectId,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Value, RepositoryError>;
}

impl CommitmentCatalogRepository for SqliteStoryRepository {
    /// Discovers independent author objects at one immutable snapshot. Page
    /// summaries carry no live write controls; opening an object rechecks them.
    fn narrative_commitments_page(
        &self,
        project: &ProjectId,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if !(1..=20).contains(&limit) {
            return Err(invalid());
        }
        let cursor: Option<Cursor> = cursor
            .map(|text| {
                if text.len() > 1024 {
                    return Err(invalid());
                }
                serde_json::from_str(text).map_err(|_| invalid())
            })
            .transpose()?;
        if cursor.as_ref().is_some_and(|c| {
            c.project_id != project.as_str()
                || c.limit != limit
                || c.after_object_id.len() > 160
                || !c.after_object_id.starts_with("commitment-")
        }) {
            return Err(invalid());
        }
        self.with_read_snapshot(|| {
            let snapshot = match &cursor {
                Some(cursor) => SnapshotId::parse(cursor.snapshot_id.clone()).map_err(|_|invalid())?,
                None => self.branch_head(project,&BranchId::parse("main").expect("static branch"))?.ok_or(RepositoryError::ProjectNotFound)?.snapshot_id,
            };
            let owner: Option<String> = self.connection.query_row("SELECT project_id FROM story_snapshots_v2 WHERE snapshot_id=?1", [snapshot.as_str()], |row|row.get(0)).optional().map_err(storage)?;
            if owner.as_deref() != Some(project.as_str()) { return Err(invalid()); }
            let after = cursor.as_ref().map(|c|c.after_object_id.as_str());
            let mut statement = self.connection.prepare("SELECT m.object_id FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.object_id=m.object_id WHERE m.snapshot_id=?1 AND r.project_id=?2 AND r.object_kind='commitment' AND (?3 IS NULL OR m.object_id>?3) ORDER BY m.object_id LIMIT ?4").map_err(storage)?;
            let objects = statement.query_map(params![snapshot.as_str(),project.as_str(),after,limit+1], |row|row.get::<_,String>(0)).map_err(storage)?.collect::<Result<Vec<_>,_>>().map_err(storage)?;
            let has_more = objects.len() > limit;
            let mut data = Vec::with_capacity(limit);
            for object in objects.iter().take(limit) {
                let id = object.strip_prefix("commitment-").ok_or(RepositoryError::CorruptState)?;
                let detail = self.narrative_commitment_in_read_snapshot(project,id,snapshot.as_str(),crate::narrative_commitments::CommitmentReadControls::ReadOnly)?;
                let payload = &detail["payload"];
                data.push(json!({"id":id,"summary":payload["summary"],"kind":payload["kind"],"status":payload["status"],"revisionId":detail["revisionId"],"confirmationState":detail["confirmationState"],"sourceCurrent":detail["sourceCurrent"]}));
            }
            let next = if has_more {
                Some(serde_json::to_string(&Cursor { project_id:project.as_str().into(),snapshot_id:snapshot.as_str().into(),after_object_id:objects[limit-1].clone(),limit }).map_err(|_|RepositoryError::CorruptState)?)
            } else { None };
            Ok(json!({"snapshotId":snapshot.as_str(),"data":data,"nextCursor":next}))
        })?
    }
}

use crate::{ProjectRevisionRepository, ReadSnapshotRepository};
