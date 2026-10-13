use novel_codex_story_core::{RepositoryError, SearchAuditJob, StoryDatabase};

/// One bounded, restart-safe worker tick. The persisted job is authoritative, not this process.
pub fn search_audit_tick(
    database: &StoryDatabase,
) -> Result<Option<SearchAuditJob>, RepositoryError> {
    let mut repository = database.repository()?;
    let Some((project_id, job_id)) = repository.next_search_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_search_audit(&project_id, &job_id)
        .map(Some)
}

use novel_codex_story_core::{SearchAuditJobsRepository};
