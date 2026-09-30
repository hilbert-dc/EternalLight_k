use crate::{ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub struct ModelingOutboxQuery<'a> {
    pub project_id: &'a ProjectId,
    pub cursor: Option<&'a str>,
    pub limit: usize,
    pub now_ms: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    project_id: String,
    rowid: i64,
    count: i64,
    generation: i64,
}

impl SqliteStoryRepository {
    /// Reads durable latest-source intents; due is a hint, never an execution grant.
    pub fn modeling_outbox_page(
        &self,
        query: ModelingOutboxQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        if !(1..=20).contains(&query.limit)
            || query.now_ms < 0
            || query.cursor.is_some_and(|cursor| cursor.len() > 512)
        {
            return Err(RepositoryError::InvalidContent(
                "invalid outbox query".into(),
            ));
        }
        self.with_read_snapshot(|| {
            if self.project(query.project_id)?.is_none() { return Err(RepositoryError::ProjectNotFound); }
            let (count,generation): (i64,i64) = self.connection.query_row("SELECT COUNT(*),COALESCE((SELECT generation FROM story_modeling_outbox_state_v1 WHERE project_id=?1),0) FROM story_modeling_outbox_v1 WHERE project_id=?1",[query.project_id.as_str()],|row| Ok((row.get(0)?,row.get(1)?))).map_err(storage_error)?;
            let cursor=query.cursor.map(serde_json::from_str::<Cursor>).transpose().map_err(|_| RepositoryError::InvalidContent("invalid outbox cursor".into()))?;
            if cursor.as_ref().is_some_and(|cursor| cursor.project_id!=query.project_id.as_str() || cursor.count!=count || cursor.generation!=generation || cursor.rowid<=0) {
                return Err(RepositoryError::InvalidContent("stale or foreign outbox cursor".into()));
            }
            let pending:i64=self.connection.query_row("SELECT COUNT(*) FROM story_modeling_outbox_v1 WHERE project_id=?1 AND status='pending'",[query.project_id.as_str()],|row| row.get(0)).map_err(storage_error)?;
            let mut statement=self.connection.prepare("SELECT q.rowid,q.object_id,q.source_revision_id,q.origin_snapshot_id,q.generation,q.first_dirty_ms,q.last_dirty_ms,COALESCE(h.candidate_revision_id,h.accepted_revision_id) FROM story_modeling_outbox_v1 q JOIN story_object_heads_v2 h ON h.project_id=q.project_id AND h.object_id=q.object_id WHERE q.project_id=?1 AND q.status='pending' AND q.rowid>?2 ORDER BY q.rowid LIMIT ?3").map_err(storage_error)?;
            let rows=statement.query_map(params![query.project_id.as_str(),cursor.as_ref().map_or(0,|cursor|cursor.rowid),query.limit+1],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,i64>(4)?,row.get::<_,i64>(5)?,row.get::<_,i64>(6)?,row.get::<_,Option<String>>(7)?)))
                .map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
            let next_cursor=if rows.len()>query.limit { Some(serde_json::to_string(&Cursor{project_id:query.project_id.as_str().into(),rowid:rows[query.limit-1].0,count,generation}).map_err(|_|RepositoryError::CorruptState)?) } else {None};
            let mut data=Vec::new();
            for (_,object,source,snapshot,version,first,last,current) in rows.into_iter().take(query.limit) {
                if current.as_deref()!=Some(&source) {return Err(RepositoryError::CorruptState);}
                let due_at=last.saturating_add(1000).min(first.saturating_add(5000));
                data.push(json!({"objectId":object,"sourceRevisionId":source,"originSnapshotId":snapshot,"generation":version,"status":"pending","firstDirtyMs":first,"lastDirtyMs":last,"dueAtMs":due_at,"due":query.now_ms>=due_at}));
            }
            Ok(json!({"data":data,"nextCursor":next_cursor,"pendingCount":pending,"branchId":"main","authority":"intentOnly"}))
        })?
    }
}

pub(crate) fn enqueue_dependency(
    connection: &Connection,
    project: &ProjectId,
    object: &str,
) -> Result<(), RepositoryError> {
    connection.execute("INSERT INTO story_modeling_outbox_v1(project_id,object_id,source_revision_id,origin_snapshot_id,status,first_dirty_ms,last_dirty_ms) SELECT h.project_id,h.object_id,COALESCE(h.candidate_revision_id,h.accepted_revision_id),b.head_snapshot_id,'pending',CAST((julianday('now')-2440587.5)*86400000 AS INTEGER),CAST((julianday('now')-2440587.5)*86400000 AS INTEGER) FROM story_object_heads_v2 h JOIN story_branches_v2 b ON b.project_id=h.project_id AND b.branch_id='main' WHERE h.project_id=?1 AND h.object_id=?2 AND h.object_kind='draft' AND COALESCE(h.candidate_revision_id,h.accepted_revision_id) IS NOT NULL ON CONFLICT(project_id,object_id) DO UPDATE SET generation=generation+1,status='pending',source_revision_id=excluded.source_revision_id,origin_snapshot_id=excluded.origin_snapshot_id,completed_batch_id=NULL,first_dirty_ms=CASE WHEN status='pending' THEN first_dirty_ms ELSE excluded.first_dirty_ms END,last_dirty_ms=excluded.last_dirty_ms",params![project.as_str(),object]).map_err(storage_error)?;
    Ok(())
}
fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "modeling_outbox_tests.rs"]
mod tests;
