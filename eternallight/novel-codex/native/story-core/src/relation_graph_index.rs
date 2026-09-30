use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde_json::Value;

use crate::relation_graph::RelationGraphScope;
use crate::{ProjectId, RepositoryError};

const NEIGHBOR_FETCH_LIMIT: usize = 1_001;

pub(super) struct IndexedRelation {
    pub batch_id: String,
    pub assertion_index: usize,
    pub assertion: Value,
    pub source_object_id: String,
    pub source_revision_id: String,
    pub subject_id: String,
    pub target_id: String,
    pub relation_type: String,
}

pub(super) fn source_coverage(
    connection: &Connection,
    project_id: &ProjectId,
    scope: RelationGraphScope<'_>,
) -> Result<(i64, i64), RepositoryError> {
    let (total_sql, modeled_sql) = match scope {
        RelationGraphScope::Snapshot(_) => (
            "SELECT COUNT(*) FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id = m.revision_id AND r.project_id = ?1 AND r.object_id = m.object_id WHERE m.snapshot_id = ?2 AND r.object_kind = 'draft'",
            "SELECT COUNT(*) FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.revision_id = m.revision_id AND r.project_id = ?1 AND r.object_id = m.object_id JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id = m.snapshot_id AND k.object_id = m.object_id JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id AND b.project_id = r.project_id AND b.source_revision_id = m.revision_id WHERE m.snapshot_id = ?2 AND r.object_kind = 'draft'",
        ),
        RelationGraphScope::Current => (
            "SELECT COUNT(*) FROM story_object_heads_v2 WHERE project_id = ?1 AND object_kind = 'draft' AND ?2 IS NULL",
            "SELECT COUNT(*) FROM story_object_heads_v2 o JOIN story_knowledge_heads_v2 h ON h.project_id = o.project_id AND h.object_id = o.object_id JOIN story_modeling_batches_v2 b ON b.batch_id = h.batch_id AND b.project_id = o.project_id AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) AND b.status = 'ready' WHERE o.project_id = ?1 AND o.object_kind = 'draft' AND ?2 IS NULL",
        ),
        RelationGraphScope::WriterBeforeChapter(_) | RelationGraphScope::WriterPov { .. } => (
            "SELECT COUNT(*) FROM story_chapters_v2 ch WHERE ch.project_id = ?1 AND ch.number < ?2",
            "SELECT COUNT(*) FROM story_object_heads_v2 o JOIN story_chapters_v2 ch ON ch.project_id = o.project_id AND o.object_id = 'draft-' || ch.chapter_id JOIN story_knowledge_heads_v2 h ON h.project_id = o.project_id AND h.object_id = o.object_id JOIN story_modeling_batches_v2 b ON b.batch_id = h.batch_id AND b.project_id = o.project_id AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) AND b.status = 'ready' WHERE o.project_id = ?1 AND o.object_kind = 'draft' AND ch.number < ?2",
        ),
    };
    let values = [
        SqlValue::Text(project_id.as_str().to_owned()),
        scope_arg(scope),
    ];
    let total = connection
        .query_row(total_sql, params_from_iter(values.clone()), |row| {
            row.get(0)
        })
        .map_err(storage_error)?;
    let modeled = connection
        .query_row(modeled_sql, params_from_iter(values), |row| row.get(0))
        .map_err(storage_error)?;
    Ok((total, modeled))
}

pub(super) fn seed_is_visible(
    connection: &Connection,
    project_id: &ProjectId,
    scope: RelationGraphScope<'_>,
    raw_ids: &[String],
) -> Result<bool, RepositoryError> {
    let placeholders = vec!["?"; raw_ids.len()].join(", ");
    let visibility = match scope {
        RelationGraphScope::Snapshot(_) => {
            "JOIN story_snapshot_knowledge_v2 k ON k.batch_id = c.batch_id \
         JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id \
         JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id AND b.object_id = m.object_id AND b.source_revision_id = m.revision_id \
         WHERE c.project_id = ? AND k.snapshot_id = ?"
        }
        RelationGraphScope::Current => {
            "JOIN story_knowledge_heads_v2 h ON h.batch_id = c.batch_id AND h.project_id = c.project_id \
         JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id AND b.object_id = h.object_id AND b.status = 'ready' \
         JOIN story_object_heads_v2 o ON o.project_id = c.project_id AND o.object_id = h.object_id AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
         WHERE c.project_id = ? AND ? IS NULL"
        }
        RelationGraphScope::WriterBeforeChapter(_) | RelationGraphScope::WriterPov { .. } => {
            "JOIN story_knowledge_heads_v2 h ON h.batch_id = c.batch_id AND h.project_id = c.project_id \
         JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id AND b.object_id = h.object_id AND b.status = 'ready' \
         JOIN story_object_heads_v2 o ON o.project_id = c.project_id AND o.object_id = h.object_id AND o.object_kind = 'draft' AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
         JOIN story_chapters_v2 ch ON ch.project_id = c.project_id AND o.object_id = 'draft-' || ch.chapter_id \
         WHERE c.project_id = ? AND ch.number < ?"
        }
    };
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM story_entity_claims_v2 c {visibility} AND c.entity_id IN ({placeholders}))"
    );
    let mut values = vec![SqlValue::Text(project_id.as_str().to_owned())];
    values.push(scope_arg(scope));
    values.extend(raw_ids.iter().cloned().map(SqlValue::Text));
    let found: i64 = connection
        .query_row(&sql, params_from_iter(values), |row| row.get(0))
        .map_err(storage_error)?;
    Ok(found != 0)
}

pub(super) fn neighbors(
    connection: &Connection,
    project_id: &ProjectId,
    scope: RelationGraphScope<'_>,
    raw_ids: &[String],
    relation_type: Option<&str>,
    endpoint: &str,
    policy: crate::dependency_rules::RelationTraversalPolicy,
) -> Result<(Vec<IndexedRelation>, bool), RepositoryError> {
    let placeholders = vec!["?"; raw_ids.len()].join(", ");
    let visibility = match scope {
        RelationGraphScope::Snapshot(_) => {
            "JOIN story_snapshot_knowledge_v2 k ON k.batch_id = c.batch_id AND k.object_id = b.object_id \
         JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id AND m.revision_id = b.source_revision_id \
         WHERE c.project_id = ? AND k.snapshot_id = ?"
        }
        RelationGraphScope::Current => {
            "JOIN story_knowledge_heads_v2 h ON h.batch_id = c.batch_id AND h.project_id = c.project_id AND h.object_id = b.object_id \
         JOIN story_object_heads_v2 o ON o.project_id = c.project_id AND o.object_id = h.object_id AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
         WHERE c.project_id = ? AND ? IS NULL AND b.status = 'ready'"
        }
        RelationGraphScope::WriterBeforeChapter(_) | RelationGraphScope::WriterPov { .. } => {
            "JOIN story_knowledge_heads_v2 h ON h.batch_id = c.batch_id AND h.project_id = c.project_id AND h.object_id = b.object_id \
         JOIN story_object_heads_v2 o ON o.project_id = c.project_id AND o.object_id = h.object_id AND o.object_kind = 'draft' AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
         JOIN story_chapters_v2 ch ON ch.project_id = c.project_id AND o.object_id = 'draft-' || ch.chapter_id \
         WHERE c.project_id = ? AND ch.number < ? AND b.status = 'ready'"
        }
    };
    let sql = format!(
        "SELECT c.batch_id, c.assertion_index, c.assertion_json, b.object_id, b.source_revision_id, c.subject_id, c.target_id, c.relation_type \
         FROM story_relation_claims_v1 c JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id \
         {visibility} AND c.{endpoint}_id IN ({placeholders}) AND (? IS NULL OR c.relation_type = ?) \
         AND (? IS NULL OR c.relation_type IN (SELECT value FROM json_each(?))) \
         ORDER BY c.batch_id, c.assertion_index LIMIT ?"
    );
    let mut values = vec![SqlValue::Text(project_id.as_str().to_owned())];
    values.push(scope_arg(scope));
    values.extend(raw_ids.iter().cloned().map(SqlValue::Text));
    let kind = relation_type.map_or(SqlValue::Null, |kind| SqlValue::Text(kind.to_owned()));
    values.push(kind.clone());
    values.push(kind);
    let allowed = match policy {
        crate::dependency_rules::RelationTraversalPolicy::AllRelations => SqlValue::Null,
        crate::dependency_rules::RelationTraversalPolicy::ExplicitDependencies => SqlValue::Text(
            serde_json::to_string(crate::dependency_rules::DEPENDENCY_RELATIONS)
                .map_err(|_| RepositoryError::CorruptState)?,
        ),
    };
    values.push(allowed.clone());
    values.push(allowed);
    values.push(SqlValue::Integer(NEIGHBOR_FETCH_LIMIT as i64));
    let mut statement = connection.prepare(&sql).map_err(storage_error)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(storage_error)?;
    let mut result = Vec::new();
    for row in rows {
        let (
            batch_id,
            index,
            assertion_json,
            source_object_id,
            source_revision_id,
            subject_id,
            target_id,
            relation_type,
        ) = row.map_err(storage_error)?;
        let assertion =
            serde_json::from_str(&assertion_json).map_err(|_| RepositoryError::CorruptState)?;
        result.push(IndexedRelation {
            batch_id,
            assertion_index: usize::try_from(index).map_err(|_| RepositoryError::CorruptState)?,
            assertion,
            source_object_id,
            source_revision_id,
            subject_id,
            target_id,
            relation_type,
        });
    }
    let truncated = result.len() == NEIGHBOR_FETCH_LIMIT;
    if truncated {
        result.pop();
    }
    Ok((result, truncated))
}

fn scope_arg(scope: RelationGraphScope<'_>) -> SqlValue {
    match scope {
        RelationGraphScope::Current => SqlValue::Null,
        RelationGraphScope::Snapshot(id) => SqlValue::Text(id.as_str().to_owned()),
        RelationGraphScope::WriterBeforeChapter(number) => SqlValue::Integer(number),
        RelationGraphScope::WriterPov { chapter, .. } => SqlValue::Integer(chapter),
    }
}

/// A conservative, source-bound knowledge gate. Proposition IDs are local to a
/// modeling batch until explicit cross-batch identity is established; the same
/// string in another batch cannot authorize this relation. Conflicting or
/// undated claims never establish POV certainty; absence means unknown.
pub(crate) fn epistemic_visibility(
    connection: &Connection,
    project_id: &ProjectId,
    relation_batch_id: &str,
    chapter: i64,
    character_id: &str,
    proposition_id: &str,
    world_time: Option<i64>,
) -> Result<bool, RepositoryError> {
    let Some(at) = world_time else {
        return Ok(false);
    };
    let mut statement = connection.prepare(
        "SELECT c.epistemic_state, c.epistemic_time, c.assertion_json, json_extract(b.assertions_json, '$[' || c.assertion_index || ']'), c.batch_id, c.proposition_source_batch_id \
         FROM story_epistemic_claims_v1 c \
         JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id AND b.status = 'ready' \
         JOIN story_knowledge_heads_v2 h ON h.batch_id = b.batch_id AND h.project_id = b.project_id AND h.object_id = b.object_id \
         JOIN story_object_heads_v2 o ON o.project_id = b.project_id AND o.object_id = b.object_id AND o.object_kind = 'draft' AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
         JOIN story_chapters_v2 ch ON ch.project_id = b.project_id AND b.object_id = 'draft-' || ch.chapter_id \
         WHERE c.project_id = ?1 AND c.owner_id = ?2 AND c.proposition_id = ?3 \
           AND c.proposition_source_batch_id = ?4 AND ch.number < ?5 \
         ORDER BY c.batch_id, c.assertion_index LIMIT 1001"
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(
            (
                project_id.as_str(),
                character_id,
                proposition_id,
                relation_batch_id,
                chapter,
            ),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(storage_error)?;
    let mut known = false;
    let mut count = 0;
    for row in rows {
        let (state, time, payload, source, claim_batch_id, source_batch_id) =
            row.map_err(storage_error)?;
        count += 1;
        if count > 1000 {
            return Ok(false);
        }
        let assertion: Value =
            serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
        let source: Value =
            serde_json::from_str(source.as_deref().ok_or(RepositoryError::CorruptState)?)
                .map_err(|_| RepositoryError::CorruptState)?;
        if assertion["kind"] != "epistemic"
            || !crate::epistemic_time::epistemic_interval_valid(&assertion)
            || assertion != source
            || assertion["epistemicOwnerId"] != character_id
            || assertion["propositionId"] != proposition_id
            || assertion["epistemicState"] != state
            || assertion["epistemicTime"].as_i64() != time
            || assertion["propositionSourceBatchId"]
                .as_str()
                .unwrap_or(&claim_batch_id)
                != source_batch_id
            || source_batch_id != relation_batch_id
        {
            return Err(RepositoryError::CorruptState);
        }
        let Some(knowledge_time) = time else {
            return Ok(false);
        };
        if knowledge_time > at {
            continue;
        }
        if !crate::epistemic_claim_active_at(&assertion, at) {
            continue;
        }
        if state != "known" || assertion["value"] != true {
            return Ok(false);
        }
        known = true;
    }
    Ok(known)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
