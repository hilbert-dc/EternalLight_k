use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHeadAudit {
    pub published_batches: usize,
    pub expected_heads: usize,
    pub missing_heads: usize,
    pub changed_heads: usize,
    pub extra_heads: usize,
    pub invalid_ready_batches: usize,
    pub ambiguous_ready_batches: usize,
    pub affected_object_ids: Vec<String>,
}

impl KnowledgeHeadAudit {
    pub fn complete(&self) -> bool {
        self.missing_heads == 0
            && self.changed_heads == 0
            && self.extra_heads == 0
            && self.invalid_ready_batches == 0
            && self.ambiguous_ready_batches == 0
    }

    pub(crate) fn empty() -> Self {
        Self {
            published_batches: 0,
            expected_heads: 0,
            missing_heads: 0,
            changed_heads: 0,
            extra_heads: 0,
            invalid_ready_batches: 0,
            ambiguous_ready_batches: 0,
            affected_object_ids: Vec::new(),
        }
    }
}



pub(crate) fn audit_head_batch_on(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
    object_id: &str,
    source_revision_id: &str,
    status: &str,
    audit: &mut KnowledgeHeadAudit,
) -> Result<(), RepositoryError> {
    audit.published_batches += 1;
    if status != "ready" {
        return Ok(());
    }
    let selected = connection.query_row(
        "SELECT COALESCE(o.candidate_revision_id, o.accepted_revision_id) FROM story_object_heads_v2 o \
         JOIN story_revisions_v2 r ON r.revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
         AND r.project_id = o.project_id AND r.object_id = o.object_id AND r.object_kind = 'draft' \
         WHERE o.project_id = ?1 AND o.object_id = ?2 AND o.object_kind = 'draft'",
        params![project_id.as_str(), object_id], |row| row.get::<_, Option<String>>(0),
    ).optional().map_err(storage_error)?.flatten();
    if selected.as_deref() != Some(source_revision_id) {
        audit.invalid_ready_batches += 1;
        record_affected(audit, object_id);
        return Ok(());
    }
    let valid_ready: i64 = connection.query_row(
        "SELECT COUNT(*) FROM story_modeling_batches_v2 WHERE project_id = ?1 AND object_id = ?2 AND source_revision_id = ?3 AND status = 'ready' AND published_at IS NOT NULL",
        params![project_id.as_str(), object_id, source_revision_id], |row| row.get(0),
    ).map_err(storage_error)?;
    if valid_ready > 1 {
        audit.ambiguous_ready_batches += 1;
        record_affected(audit, object_id);
    }
    audit.expected_heads += 1;
    let actual = connection.query_row(
        "SELECT batch_id FROM story_knowledge_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
        params![project_id.as_str(), object_id], |row| row.get::<_, String>(0),
    ).optional().map_err(storage_error)?;
    match actual.as_deref() {
        None => {
            audit.missing_heads += 1;
            record_affected(audit, object_id);
        }
        Some(actual) if actual != batch_id => {
            audit.changed_heads += 1;
            record_affected(audit, object_id);
        }
        Some(_) => {}
    }
    Ok(())
}

pub(crate) fn audit_head_orphans_on(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut KnowledgeHeadAudit,
) -> Result<(), RepositoryError> {
    let condition = "NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b \
        JOIN story_object_heads_v2 o ON o.project_id = b.project_id AND o.object_id = b.object_id \
        AND o.object_kind = 'draft' AND COALESCE(o.candidate_revision_id, o.accepted_revision_id) = b.source_revision_id \
        JOIN story_revisions_v2 r ON r.revision_id = b.source_revision_id \
        AND r.project_id = b.project_id AND r.object_id = b.object_id AND r.object_kind = 'draft' \
        WHERE b.batch_id = h.batch_id AND b.project_id = h.project_id AND b.object_id = h.object_id \
        AND b.status = 'ready' AND b.published_at IS NOT NULL)";
    let count_sql = format!(
        "SELECT COUNT(*) FROM story_knowledge_heads_v2 h WHERE h.project_id = ?1 AND {condition}"
    );
    let count: i64 = connection
        .query_row(&count_sql, [project_id.as_str()], |row| row.get(0))
        .map_err(storage_error)?;
    audit.extra_heads += count as usize;
    let objects_sql = format!(
        "SELECT h.object_id FROM story_knowledge_heads_v2 h WHERE h.project_id = ?1 AND {condition} ORDER BY h.object_id LIMIT 20"
    );
    let mut statement = connection.prepare(&objects_sql).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    for row in rows {
        record_affected(audit, &row.map_err(storage_error)?);
    }
    Ok(())
}

fn audit_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<KnowledgeHeadAudit, RepositoryError> {
    let mut statement = connection
        .prepare(
            "SELECT batch_id, object_id, source_revision_id, status FROM story_modeling_batches_v2 \
         WHERE project_id = ?1 AND published_at IS NOT NULL ORDER BY rowid",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(storage_error)?;
    let mut audit = KnowledgeHeadAudit::empty();
    for row in rows {
        let (batch_id, object_id, source_revision_id, status) = row.map_err(storage_error)?;
        audit_head_batch_on(
            connection,
            project_id,
            &batch_id,
            &object_id,
            &source_revision_id,
            &status,
            &mut audit,
        )?;
    }
    audit_head_orphans_on(connection, project_id, &mut audit)?;
    Ok(audit)
}

fn record_affected(audit: &mut KnowledgeHeadAudit, object_id: &str) {
    if !audit.affected_object_ids.iter().any(|id| id == object_id) {
        audit.affected_object_ids.push(object_id.to_owned());
        audit.affected_object_ids.sort();
        audit.affected_object_ids.truncate(20);
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "knowledge_head_integrity_tests.rs"]
mod tests;

/// Public knowledge head integrity capability; SQLite retains transaction ownership.
pub trait KnowledgeHeadIntegrityRepository {
    /// Compares current knowledge heads with published batches and selected draft revisions.
    fn audit_knowledge_heads(
        &self,
        project_id: &ProjectId,
    ) -> Result<KnowledgeHeadAudit, RepositoryError>;
    /// Rebuilds only the current derived heads; ambiguous or invalid ready sources abort.
    fn rebuild_knowledge_heads(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<KnowledgeHeadAudit, RepositoryError>;
}

impl KnowledgeHeadIntegrityRepository for SqliteStoryRepository {
    /// Compares current knowledge heads with published batches and selected draft revisions.
    fn audit_knowledge_heads(
        &self,
        project_id: &ProjectId,
    ) -> Result<KnowledgeHeadAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let audit = audit_on(&read, project_id)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }

    /// Rebuilds only the current derived heads; ambiguous or invalid ready sources abort.
    fn rebuild_knowledge_heads(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<KnowledgeHeadAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let before = audit_on(&tx, project_id)?;
        if before.invalid_ready_batches > 0 || before.ambiguous_ready_batches > 0 {
            return Err(RepositoryError::CorruptState);
        }
        tx.execute(
            "DELETE FROM story_knowledge_heads_v2 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) \
             SELECT b.project_id, b.object_id, b.batch_id FROM story_modeling_batches_v2 b \
             JOIN story_object_heads_v2 o ON o.project_id = b.project_id AND o.object_id = b.object_id \
             AND o.object_kind = 'draft' AND COALESCE(o.candidate_revision_id, o.accepted_revision_id) = b.source_revision_id \
             JOIN story_revisions_v2 r ON r.revision_id = b.source_revision_id \
             AND r.project_id = b.project_id AND r.object_id = b.object_id AND r.object_kind = 'draft' \
             WHERE b.project_id = ?1 AND b.status = 'ready' AND b.published_at IS NOT NULL",
            [project_id.as_str()],
        ).map_err(storage_error)?;
        let audit = audit_on(&tx, project_id)?;
        if !audit.complete() {
            return Err(RepositoryError::CorruptState);
        }
        crate::knowledge_audit_jobs::bump_knowledge_generation(&tx, project_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(audit)
    }
}

use crate::ProjectRevisionRepository;
