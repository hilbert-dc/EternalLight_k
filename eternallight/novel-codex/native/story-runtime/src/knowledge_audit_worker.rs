use std::path::Path;

use novel_codex_story_core::{KnowledgeAuditJob, RepositoryError, SqliteStoryRepository};

/// One bounded, restart-safe knowledge-index audit tick.
pub fn knowledge_audit_tick(path: &Path) -> Result<Option<KnowledgeAuditJob>, RepositoryError> {
    let mut repository = SqliteStoryRepository::open(path)?;
    let Some((project_id, job_id)) = repository.next_knowledge_audit_job()? else {
        return repository.schedule_knowledge_audit();
    };
    repository
        .advance_knowledge_audit(&project_id, &job_id)
        .map(Some)
}
