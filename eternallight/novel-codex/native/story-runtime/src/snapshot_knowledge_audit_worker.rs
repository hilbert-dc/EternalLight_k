use std::path::Path;

use novel_codex_story_core::{RepositoryError, SnapshotKnowledgeAuditJob, SqliteStoryRepository};

/// One bounded, restart-safe historical-knowledge audit tick.
pub fn snapshot_knowledge_audit_tick(
    path: &Path,
) -> Result<Option<SnapshotKnowledgeAuditJob>, RepositoryError> {
    let mut repository = SqliteStoryRepository::open(path)?;
    let Some((project_id, job_id)) = repository.next_snapshot_knowledge_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_snapshot_knowledge_audit(&project_id, &job_id)
        .map(Some)
}
