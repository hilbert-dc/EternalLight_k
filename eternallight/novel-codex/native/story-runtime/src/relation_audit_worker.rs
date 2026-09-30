use std::path::Path;

use novel_codex_story_core::{RelationAuditJob, RepositoryError, SqliteStoryRepository};

/// One bounded, restart-safe relation-index audit tick.
pub fn relation_audit_tick(path: &Path) -> Result<Option<RelationAuditJob>, RepositoryError> {
    let mut repository = SqliteStoryRepository::open(path)?;
    let Some((project_id, job_id)) = repository.next_relation_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_relation_audit(&project_id, &job_id)
        .map(Some)
}
