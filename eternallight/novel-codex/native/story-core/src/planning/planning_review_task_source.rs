use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

pub(crate) struct AuthorSource {
    pub id: String,
    pub original: String,
    pub history: String,
}

pub(crate) fn load(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    task_id: &str,
    intent_revision: i64,
) -> Result<AuthorSource, RepositoryError> {
    let task = crate::tasks::load_task(db, task_id)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if task.project_id != *project
        || task.operation != "advisory"
        || target["type"] != "project"
        || ["revisionRepairSource", "workDecision", "requestedByTaskId"]
            .iter()
            .any(|key| target.get(*key).is_some())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let author_id =
        crate::author_message::identity(db, task_id)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let original: String = db.query_row("SELECT m.author_text FROM story_author_messages_v1 m
        JOIN story_work_tasks_v1 w ON w.id=m.work_task_id AND w.project_id=m.project_id AND w.kind='conversation' AND w.goal=m.author_text
        JOIN story_work_steps_v1 s ON s.work_task_id=w.id JOIN story_work_attempts_v1 a ON a.step_id=s.id AND a.attempt_id=?1
        WHERE m.message_id=?2 AND m.work_task_id=?3 AND m.project_id=?4 AND m.intent_revision=0 AND m.review_run_id IS NULL AND m.task_id IS NOT NULL",
        params![task_id,author_id,work,project.as_str()], |row|row.get(/*idx*/0)).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let history = crate::task_author_intent::source_intent(db, task_id, intent_revision)?;
    Ok(AuthorSource {
        id: author_id,
        original,
        history,
    })
}
