use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{ObjectId, ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};



#[cfg(test)]
#[path = "snapshot_knowledge_tests.rs"]
mod tests;

/// Public snapshot knowledge capability; SQLite retains transaction ownership.
pub trait SnapshotKnowledgeRepository {
    /// Replays knowledge captured when the snapshot was created, not the current modeling head.
    fn knowledge_at_snapshot(
        &self,
        project_id: &ProjectId,
        snapshot_id: &SnapshotId,
        object_id: &ObjectId,
    ) -> Result<Value, RepositoryError>;
}

impl SnapshotKnowledgeRepository for SqliteStoryRepository {
    /// Replays knowledge captured when the snapshot was created, not the current modeling head.
    fn knowledge_at_snapshot(
        &self,
        project_id: &ProjectId,
        snapshot_id: &SnapshotId,
        object_id: &ObjectId,
    ) -> Result<Value, RepositoryError> {
        let row = self.connection.query_row(
            "SELECT m.revision_id, k.snapshot_id, k.batch_id, b.source_revision_id, b.status, b.published_at, b.entities_json, b.assertions_json, \
             EXISTS (SELECT 1 FROM story_revisions_v2 r WHERE r.revision_id = m.revision_id AND r.project_id = s.project_id AND r.object_id = m.object_id) \
             FROM story_snapshots_v2 s \
             JOIN story_snapshot_members_v2 m ON m.snapshot_id = s.snapshot_id \
             LEFT JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id = m.snapshot_id AND k.object_id = m.object_id \
             LEFT JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id AND b.project_id = s.project_id AND b.object_id = m.object_id \
             WHERE s.project_id = ?1 AND s.snapshot_id = ?2 AND m.object_id = ?3",
            params![project_id.as_str(), snapshot_id.as_str(), object_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, Option<i64>>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, Option<String>>(7)?, row.get::<_, bool>(8)?)),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?
            .ok_or(RepositoryError::CandidateNotFound)?;
        if row.1.is_none() || !row.8 {
            return Err(RepositoryError::CorruptState);
        }
        let ready = row.2.is_some();
        if ready
            && (row.3.as_deref() != Some(row.0.as_str())
                || !matches!(row.4.as_deref(), Some("ready" | "stale"))
                || row.5.is_none())
        {
            return Err(RepositoryError::CorruptState);
        }
        let entities: Vec<Value> = if ready {
            serde_json::from_str(row.6.as_deref().ok_or(RepositoryError::CorruptState)?)
                .map_err(|_| RepositoryError::CorruptState)?
        } else {
            Vec::new()
        };
        let assertions: Vec<Value> = if ready {
            serde_json::from_str(row.7.as_deref().ok_or(RepositoryError::CorruptState)?)
                .map_err(|_| RepositoryError::CorruptState)?
        } else {
            Vec::new()
        };
        crate::story_events::validate_batch(&assertions, &entities)
            .map_err(|_| RepositoryError::CorruptState)?;
        if let Some(batch_id) = row.2.as_deref() {
            crate::event_sources::validate_batch(
                &self.connection,
                project_id,
                crate::model_sources::ModelSourceScope::Snapshot(snapshot_id),
                batch_id,
                &assertions,
                &entities,
            )
            .map_err(|_| RepositoryError::CorruptState)?;
        }
        Ok(json!({
            "snapshotId": snapshot_id.as_str(),
            "objectId": object_id.as_str(),
            "sourceRevisionId": row.0,
            "status": if ready { "ready" } else { "model_pending" },
            "batchId": if ready { row.2 } else { None },
            "entities": entities,
            "assertions": assertions,
            "confirmationState": "candidate"
        }))
    }
}
