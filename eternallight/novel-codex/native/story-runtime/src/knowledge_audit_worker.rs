use novel_codex_story_core::{KnowledgeAuditJob, RepositoryError, StoryDatabase};

/// One bounded, restart-safe knowledge-index audit tick.
pub fn knowledge_audit_tick(
    database: &StoryDatabase,
) -> Result<Option<KnowledgeAuditJob>, RepositoryError> {
    let mut repository = database.repository()?;
    let Some((project_id, job_id)) = repository.next_knowledge_audit_job()? else {
        return repository.schedule_knowledge_audit();
    };
    repository
        .advance_knowledge_audit(&project_id, &job_id)
        .map(Some)
}

use novel_codex_story_core::{KnowledgeAuditJobsRepository, KnowledgeAuditSchedulingRepository};
