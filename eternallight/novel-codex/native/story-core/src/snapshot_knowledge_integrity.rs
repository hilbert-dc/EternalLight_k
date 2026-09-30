use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotKnowledgeAudit {
    pub members: usize,
    pub ready_captures: usize,
    pub pending_captures: usize,
    pub missing_captures: usize,
    pub invalid_captures: usize,
    pub invalid_members: usize,
    pub orphan_captures: usize,
    pub affected_members: Vec<String>,
}

impl SnapshotKnowledgeAudit {
    pub fn complete(&self) -> bool {
        self.missing_captures == 0
            && self.invalid_captures == 0
            && self.invalid_members == 0
            && self.orphan_captures == 0
    }

    pub(crate) fn empty() -> Self {
        Self {
            members: 0,
            ready_captures: 0,
            pending_captures: 0,
            missing_captures: 0,
            invalid_captures: 0,
            invalid_members: 0,
            orphan_captures: 0,
            affected_members: Vec::new(),
        }
    }
}

pub(crate) const MEMBER_SELECT: &str = "SELECT m.rowid, m.snapshot_id, m.object_id, m.revision_id, r.object_kind, k.snapshot_id, k.batch_id, \
    b.source_revision_id, b.status, b.published_at \
    FROM story_snapshot_members_v2 m \
    JOIN story_snapshots_v2 s ON s.snapshot_id = m.snapshot_id \
    LEFT JOIN story_revisions_v2 r ON r.revision_id = m.revision_id \
      AND r.project_id = s.project_id AND r.object_id = m.object_id \
    LEFT JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id = m.snapshot_id AND k.object_id = m.object_id \
    LEFT JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id \
      AND b.project_id = s.project_id AND b.object_id = m.object_id";

pub(crate) struct SnapshotKnowledgeMember {
    pub rowid: i64,
    snapshot_id: String,
    object_id: String,
    revision_id: String,
    member_kind: Option<String>,
    capture: Option<String>,
    batch_id: Option<String>,
    source_revision: Option<String>,
    status: Option<String>,
    published_at: Option<i64>,
}

impl SnapshotKnowledgeMember {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            rowid: row.get(0)?,
            snapshot_id: row.get(1)?,
            object_id: row.get(2)?,
            revision_id: row.get(3)?,
            member_kind: row.get(4)?,
            capture: row.get(5)?,
            batch_id: row.get(6)?,
            source_revision: row.get(7)?,
            status: row.get(8)?,
            published_at: row.get(9)?,
        })
    }
}

impl SqliteStoryRepository {
    /// Audits immutable, point-in-time knowledge captures; never infers absent history from a current head.
    pub fn audit_snapshot_knowledge(
        &self,
        project_id: &ProjectId,
    ) -> Result<SnapshotKnowledgeAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let audit = audit_on(&read, project_id)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }
}

fn audit_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<SnapshotKnowledgeAudit, RepositoryError> {
    let mut audit = SnapshotKnowledgeAudit::empty();
    let sql = format!("{MEMBER_SELECT} WHERE s.project_id = ?1 ORDER BY m.rowid");
    let mut statement = connection.prepare(&sql).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], SnapshotKnowledgeMember::from_row)
        .map_err(storage_error)?;
    for row in rows {
        audit_member(&mut audit, row.map_err(storage_error)?);
    }
    audit_orphans(connection, project_id, &mut audit)?;
    Ok(audit)
}

pub(crate) fn audit_member(audit: &mut SnapshotKnowledgeAudit, member: SnapshotKnowledgeMember) {
    audit.members += 1;
    let mut affected = false;
    if member.member_kind.is_none() {
        audit.invalid_members += 1;
        affected = true;
    }
    if member.capture.is_none() {
        audit.missing_captures += 1;
        affected = true;
    } else if member.batch_id.is_none() {
        audit.pending_captures += 1;
    } else if member.member_kind.as_deref() != Some("draft")
        || member.source_revision.as_deref() != Some(member.revision_id.as_str())
        || !matches!(member.status.as_deref(), Some("ready" | "stale"))
        || member.published_at.is_none()
    {
        audit.invalid_captures += 1;
        affected = true;
    } else {
        audit.ready_captures += 1;
    }
    if affected && audit.affected_members.len() < 20 {
        audit
            .affected_members
            .push(format!("{}/{}", member.snapshot_id, member.object_id));
    }
}

pub(crate) fn audit_orphans(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut SnapshotKnowledgeAudit,
) -> Result<(), RepositoryError> {
    let orphan_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM story_snapshot_knowledge_v2 k \
         JOIN story_snapshots_v2 s ON s.snapshot_id = k.snapshot_id AND s.project_id = ?1 \
         WHERE NOT EXISTS (SELECT 1 FROM story_snapshot_members_v2 m \
           WHERE m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id)",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    audit.orphan_captures = orphan_count as usize;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "snapshot_knowledge_integrity_tests.rs"]
mod tests;
