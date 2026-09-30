use std::path::Path;

use novel_codex_story_core::{FactAuditJob, RepositoryError, SqliteStoryRepository};

pub fn fact_audit_tick(path: &Path) -> Result<Option<FactAuditJob>, RepositoryError> {
    let mut repository = SqliteStoryRepository::open(path)?;
    let Some((project_id, job_id)) = repository.next_fact_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_fact_audit(&project_id, &job_id)
        .map(Some)
}
