use novel_codex_story_core::{EntityAuditJob, RepositoryError, StoryDatabase};

/// One bounded, restart-safe entity-index audit tick.
pub fn entity_audit_tick(
    database: &StoryDatabase,
) -> Result<Option<EntityAuditJob>, RepositoryError> {
    let mut repository = database.repository()?;
    let Some((project_id, job_id)) = repository.next_entity_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_entity_audit(&project_id, &job_id)
        .map(Some)
}

use novel_codex_story_core::{EntityAuditJobsRepository};
