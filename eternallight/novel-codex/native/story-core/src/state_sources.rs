use std::collections::HashMap;

use rusqlite::{OptionalExtension, params, params_from_iter, types::Value as SqlValue};

use crate::knowledge_projection::{MAX_PENDING_SOURCE_IDS, MAX_PROJECTED_ENTRIES};
use crate::snapshot_state::SnapshotStateSources;
use crate::state_selector::StateSelector;
use crate::{ObjectId, RepositoryError, SqliteStoryRepository};

impl SqliteStoryRepository {
    /// Select relevant authoritative sources before enforcing response bounds.
    pub(crate) fn projected_state_sources(
        &self,
        selector: &StateSelector<'_>,
    ) -> Result<SnapshotStateSources, RepositoryError> {
        let query = selector.query;
        let reading = selector.visible_through_chapter;
        let (base, scope, object, chapter, pending, mut args) = if let Some(snapshot) =
            query.snapshot_id
        {
            let exists = self.connection.query_row("SELECT 1 FROM story_snapshots_v2 WHERE project_id = ?1 AND branch_id = 'main' AND snapshot_id = ?2",
                params![query.project_id.as_str(), snapshot.as_str()], |row| row.get::<_, i64>(0)).optional().map_err(storage_error)?;
            if exists.is_none() {
                return Err(RepositoryError::CandidateNotFound);
            }
            let base = "FROM story_snapshot_members_v2 m LEFT JOIN story_revisions_v2 r ON r.revision_id = m.revision_id AND r.project_id = ?1 AND r.object_id = m.object_id AND r.object_kind = 'draft'
                LEFT JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id = m.snapshot_id AND k.object_id = m.object_id
                LEFT JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id AND b.project_id = ?1 AND b.object_id = m.object_id";
            let scope = "m.snapshot_id = ?2 AND m.object_id LIKE 'draft-%'";
            let invalid: i64 = self.connection.query_row(&format!("SELECT COUNT(*) {base} WHERE {scope} AND (r.revision_id IS NULL OR k.snapshot_id IS NULL OR (k.batch_id IS NOT NULL AND (b.batch_id IS NULL OR b.source_revision_id != m.revision_id OR b.status NOT IN ('ready','stale') OR b.published_at IS NULL)))"),
                params![query.project_id.as_str(), snapshot.as_str()], |row| row.get(0)).map_err(storage_error)?;
            if invalid != 0 {
                return Err(RepositoryError::CorruptState);
            }
            (
                base,
                scope.to_owned(),
                "m.object_id",
                "CASE WHEN json_type(r.content_json, '$.chapterNumber') = 'integer' AND json_extract(r.content_json, '$.chapterNumber') > 0 THEN json_extract(r.content_json, '$.chapterNumber') END",
                "k.batch_id IS NULL",
                vec![
                    SqlValue::Text(query.project_id.as_str().to_owned()),
                    SqlValue::Text(snapshot.as_str().to_owned()),
                ],
            )
        } else {
            let base = "FROM story_object_heads_v2 o LEFT JOIN story_chapters_v2 c ON c.project_id = o.project_id AND o.object_id = 'draft-' || c.chapter_id
                LEFT JOIN story_knowledge_heads_v2 k ON k.project_id = o.project_id AND k.object_id = o.object_id
                LEFT JOIN story_modeling_batches_v2 b ON b.project_id = o.project_id AND b.object_id = o.object_id AND b.batch_id = k.batch_id AND b.status = 'ready' AND b.published_at IS NOT NULL AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id)
                LEFT JOIN story_revisions_v2 r ON r.project_id = o.project_id AND r.object_id = o.object_id AND r.revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) AND r.object_kind = 'draft'";
            (
                base,
                "o.project_id = ?1 AND o.object_kind = 'draft'".to_owned(),
                "o.object_id",
                "c.number",
                "b.batch_id IS NULL OR r.revision_id IS NULL",
                vec![SqlValue::Text(query.project_id.as_str().to_owned())],
            )
        };
        let unknown_chapter_sources: i64 = self
            .connection
            .query_row(
                &format!("SELECT COUNT(*) {base} WHERE {scope} AND ({chapter}) IS NULL"),
                params_from_iter(args.clone()),
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let scope = if let Some(reading) = reading {
            args.push(SqlValue::Integer(reading));
            format!("{scope} AND ({chapter}) <= ?")
        } else {
            scope
        };
        let pending_count: i64 = self
            .connection
            .query_row(
                &format!("SELECT COUNT(*) {base} WHERE {scope} AND ({pending})"),
                params_from_iter(args.clone()),
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let mut pending_statement = self.connection.prepare(&format!("SELECT {object}, {chapter} {base} WHERE {scope} AND ({pending}) ORDER BY 2, 1 LIMIT {MAX_PENDING_SOURCE_IDS}")).map_err(storage_error)?;
        let pending_ids = pending_statement
            .query_map(params_from_iter(args.clone()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let filter = selector.sql(&mut args);
        let relevant = format!(
            "{base}, json_each(b.assertions_json) assertion WHERE {scope} AND NOT ({pending}) AND ({filter})"
        );
        let count: i64 = self
            .connection
            .query_row(
                &format!("SELECT COUNT(DISTINCT {object}) {relevant}"),
                params_from_iter(args.clone()),
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let mut statement = self.connection.prepare(&format!("SELECT DISTINCT {object}, {chapter} {relevant} ORDER BY 2, 1 LIMIT {MAX_PROJECTED_ENTRIES}")).map_err(storage_error)?;
        let ids = statement
            .query_map(params_from_iter(args), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let mut chapters = HashMap::new();
        let mut sources = Vec::new();
        for (id, number) in ids {
            if let Some(number) = number {
                chapters.insert(id.clone(), number);
            }
            if let Some(snapshot) = query.snapshot_id {
                let object = ObjectId::parse(id).map_err(|_| RepositoryError::CorruptState)?;
                sources.push(self.knowledge_at_snapshot(query.project_id, snapshot, &object)?);
            } else {
                sources.extend(self.knowledge_for_ids(query.project_id, vec![id])?);
            }
        }
        // Do not let an invalid/mismatched derived head regain ready status via
        // a less strictly scoped batch loader. Pending sources carry no claims.
        for (id, number) in pending_ids {
            if let Some(number) = number {
                chapters.insert(id.clone(), number);
            }
            sources.push(serde_json::json!({"objectId":id,"status":"model_pending"}));
        }
        Ok(SnapshotStateSources {
            sources,
            chapters,
            total_sources: count
                .checked_add(pending_count)
                .ok_or(RepositoryError::CorruptState)?,
            pending_sources: pending_count,
            unknown_chapter_sources,
        })
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
