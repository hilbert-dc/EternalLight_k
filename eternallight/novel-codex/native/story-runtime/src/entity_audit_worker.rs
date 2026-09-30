use std::path::Path;

use novel_codex_story_core::{EntityAuditJob, RepositoryError, SqliteStoryRepository};

/// One bounded, restart-safe entity-index audit tick.
pub fn entity_audit_tick(path: &Path) -> Result<Option<EntityAuditJob>, RepositoryError> {
    let mut repository = SqliteStoryRepository::open(path)?;
    let Some((project_id, job_id)) = repository.next_entity_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_entity_audit(&project_id, &job_id)
        .map(Some)
}
