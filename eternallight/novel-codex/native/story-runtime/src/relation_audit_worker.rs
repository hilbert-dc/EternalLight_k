use novel_codex_story_core::{RelationAuditJob, RepositoryError, StoryDatabase};

/// One bounded, restart-safe relation-index audit tick.
pub fn relation_audit_tick(
    database: &StoryDatabase,
) -> Result<Option<RelationAuditJob>, RepositoryError> {
    let mut repository = database.repository()?;
    let Some((project_id, job_id)) = repository.next_relation_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_relation_audit(&project_id, &job_id)
        .map(Some)
}

use novel_codex_story_core::{RelationAuditJobsRepository};
