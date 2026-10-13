use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SnapshotId};

const MAX_SOURCE_BYTES: i64 = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum ModelSourceScope<'a> {
    Current,
    Snapshot(&'a SnapshotId),
    Published,
}

pub(crate) struct ValidatedModelSource {
    pub entities: Vec<Value>,
    pub assertions: Vec<Value>,
    pub chapter_number: Option<i64>,
}

pub(crate) fn visibility(scope: ModelSourceScope<'_>) -> (&'static str, Option<&str>) {
    match scope {
        ModelSourceScope::Current => (
            "JOIN story_knowledge_heads_v2 h ON h.project_id=b.project_id AND h.object_id=b.object_id AND h.batch_id=b.batch_id \
             JOIN story_object_heads_v2 o ON o.project_id=b.project_id AND o.object_id=b.object_id AND o.object_kind='draft' AND b.source_revision_id=COALESCE(o.candidate_revision_id,o.accepted_revision_id) \
             WHERE b.status='ready' AND ?3 IS NULL",
            None,
        ),
        ModelSourceScope::Snapshot(snapshot_id) => (
            "JOIN story_snapshot_knowledge_v2 k ON k.batch_id=b.batch_id AND k.object_id=b.object_id \
             JOIN story_snapshot_members_v2 m ON m.snapshot_id=k.snapshot_id AND m.object_id=k.object_id AND m.revision_id=b.source_revision_id \
             JOIN story_snapshots_v2 s ON s.snapshot_id=k.snapshot_id AND s.project_id=b.project_id \
             WHERE k.snapshot_id=?3 AND b.status IN ('ready','stale')",
            Some(snapshot_id.as_str()),
        ),
        ModelSourceScope::Published => ("WHERE b.status IN ('ready','stale') AND ?3 IS NULL", None),
    }
}

pub(crate) fn pending_sources(
    connection: &Connection,
    project: &ProjectId,
    scope: ModelSourceScope<'_>,
) -> Result<i64, RepositoryError> {
    Ok(match scope {
        ModelSourceScope::Current => connection.query_row(
            "SELECT COUNT(*) FROM story_object_heads_v2 o \
             LEFT JOIN story_knowledge_heads_v2 h ON h.project_id=o.project_id AND h.object_id=o.object_id \
             LEFT JOIN story_modeling_batches_v2 b ON b.project_id=o.project_id AND b.object_id=o.object_id AND b.batch_id=h.batch_id AND b.source_revision_id=COALESCE(o.candidate_revision_id,o.accepted_revision_id) AND b.status='ready' AND b.published_at IS NOT NULL \
             WHERE o.project_id=?1 AND o.object_kind='draft' AND b.batch_id IS NULL",
            [project.as_str()], |row| row.get::<_, i64>(0),
        ).map_err(|error| RepositoryError::Storage(error.to_string()))?,
        ModelSourceScope::Snapshot(snapshot) => {
            let invalid: i64 = connection.query_row(
                "SELECT COUNT(*) FROM story_snapshot_members_v2 m \
                 JOIN story_snapshots_v2 s ON s.snapshot_id=m.snapshot_id AND s.project_id=?1 \
                 LEFT JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id=m.snapshot_id AND k.object_id=m.object_id \
                 LEFT JOIN story_modeling_batches_v2 b ON b.batch_id=k.batch_id AND b.project_id=?1 AND b.object_id=m.object_id \
                 LEFT JOIN story_revisions_v2 r ON r.project_id=?1 AND r.object_id=m.object_id AND r.revision_id=m.revision_id AND r.object_kind='draft' \
                 WHERE m.snapshot_id=?2 AND m.object_id LIKE 'draft-%' AND (k.snapshot_id IS NULL OR r.revision_id IS NULL OR (k.batch_id IS NOT NULL AND (b.batch_id IS NULL OR b.source_revision_id!=m.revision_id OR b.status NOT IN ('ready','stale') OR b.published_at IS NULL)))",
                params![project.as_str(),snapshot.as_str()], |row| row.get(0),
            ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
            if invalid != 0 { return Err(RepositoryError::CorruptState); }
            connection.query_row(
                "SELECT COUNT(*) FROM story_snapshot_members_v2 m JOIN story_snapshots_v2 s ON s.snapshot_id=m.snapshot_id AND s.project_id=?1 JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id=m.snapshot_id AND k.object_id=m.object_id WHERE m.snapshot_id=?2 AND m.object_id LIKE 'draft-%' AND k.batch_id IS NULL",
                params![project.as_str(),snapshot.as_str()], |row| row.get::<_, i64>(0),
            ).map_err(|error| RepositoryError::Storage(error.to_string()))?
        }
        ModelSourceScope::Published => return Err(RepositoryError::TaskBindingInvalid),
    })
}

/// Resolve exact source authority before interpreting cross-source identities.
/// Published is an audit scope, not current or snapshot authorization.
pub(crate) fn validated_source(
    connection: &Connection,
    project_id: &ProjectId,
    scope: ModelSourceScope<'_>,
    batch_id: &str,
) -> Result<Option<ValidatedModelSource>, RepositoryError> {
    let (visibility, scope_id) = visibility(scope);
    let sql = format!(
        "SELECT source_bytes,CASE WHEN source_bytes<={MAX_SOURCE_BYTES} THEN entities_json END,\
        CASE WHEN source_bytes<={MAX_SOURCE_BYTES} THEN assertions_json END,\
        CASE WHEN source_bytes<={MAX_SOURCE_BYTES} THEN content_json END,chapter_number FROM (\
        SELECT b.entities_json,b.assertions_json,r.content_json,\
        length(CAST(b.entities_json AS BLOB))+length(CAST(b.assertions_json AS BLOB))+length(CAST(r.content_json AS BLOB)) AS source_bytes,\
        (SELECT c.number FROM story_chapters_v2 c WHERE c.project_id=b.project_id AND b.object_id='draft-'||c.chapter_id) AS chapter_number FROM story_modeling_batches_v2 b \
        LEFT JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.object_id=b.object_id AND r.revision_id=b.source_revision_id AND r.object_kind='draft' \
        {visibility} AND b.project_id=?1 AND b.batch_id=?2 AND b.published_at IS NOT NULL)"
    );
    let row = connection
        .query_row(
            &sql,
            params![project_id.as_str(), batch_id, scope_id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let Some((size, entities_json, assertions_json, content_json, current_chapter)) = row else {
        return Ok(None);
    };
    // One SQL statement keeps authorization, byte checks and payload selection
    // in the same read snapshot. Oversized values never become Rust strings.
    if size.ok_or(RepositoryError::CorruptState)? > MAX_SOURCE_BYTES {
        return Err(RepositoryError::InvalidContent(
            "model source exceeds 8 MiB read budget".into(),
        ));
    }
    let entities_json = entities_json.ok_or(RepositoryError::CorruptState)?;
    let assertions_json = assertions_json.ok_or(RepositoryError::CorruptState)?;
    let entities: Vec<Value> =
        serde_json::from_str(&entities_json).map_err(|_| RepositoryError::CorruptState)?;
    let assertions: Vec<Value> =
        serde_json::from_str(&assertions_json).map_err(|_| RepositoryError::CorruptState)?;
    let content: Value = serde_json::from_str(
        content_json
            .as_deref()
            .ok_or(RepositoryError::CorruptState)?,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    let chars = content["body"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?
        .chars()
        .collect::<Vec<_>>();
    crate::modeling::validate_payload(&entities, &assertions, &chars)
        .map_err(|_| RepositoryError::CorruptState)?;
    let chapter_number = match scope {
        ModelSourceScope::Current => current_chapter,
        ModelSourceScope::Snapshot(_) | ModelSourceScope::Published => {
            content["chapterNumber"].as_i64()
        }
    };
    Ok(Some(ValidatedModelSource {
        entities,
        assertions,
        chapter_number,
    }))
}
