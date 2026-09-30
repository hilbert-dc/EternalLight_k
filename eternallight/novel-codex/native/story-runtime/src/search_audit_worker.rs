use std::path::Path;

use novel_codex_story_core::{RepositoryError, SearchAuditJob, SqliteStoryRepository};

/// One bounded, restart-safe worker tick. The persisted job is authoritative, not this process.
pub fn search_audit_tick(path: &Path) -> Result<Option<SearchAuditJob>, RepositoryError> {
    let mut repository = SqliteStoryRepository::open(path)?;
    let Some((project_id, job_id)) = repository.next_search_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_search_audit(&project_id, &job_id)
        .map(Some)
}
