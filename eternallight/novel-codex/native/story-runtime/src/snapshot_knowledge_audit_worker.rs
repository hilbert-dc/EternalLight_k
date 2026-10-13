use novel_codex_story_core::{RepositoryError, SnapshotKnowledgeAuditJob, StoryDatabase};

/// One bounded, restart-safe historical-knowledge audit tick.
pub fn snapshot_knowledge_audit_tick(
    database: &StoryDatabase,
) -> Result<Option<SnapshotKnowledgeAuditJob>, RepositoryError> {
    let mut repository = database.repository()?;
    let Some((project_id, job_id)) = repository.next_snapshot_knowledge_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_snapshot_knowledge_audit(&project_id, &job_id)
        .map(Some)
}

use novel_codex_story_core::{SnapshotKnowledgeAuditJobsRepository};
