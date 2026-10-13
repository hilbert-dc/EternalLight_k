use novel_codex_story_core::{FactAuditJob, RepositoryError, StoryDatabase};

pub fn fact_audit_tick(database: &StoryDatabase) -> Result<Option<FactAuditJob>, RepositoryError> {
    let mut repository = database.repository()?;
    let Some((project_id, job_id)) = repository.next_fact_audit_job()? else {
        return Ok(None);
    };
    repository
        .advance_fact_audit(&project_id, &job_id)
        .map(Some)
}

use novel_codex_story_core::{FactAuditJobsRepository};
