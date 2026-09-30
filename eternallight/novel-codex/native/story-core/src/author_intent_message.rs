use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::authoring_runs::storage_error;
use crate::{RepositoryError, TaskIntentRecord, TaskRecord};

pub(crate) fn record(
    db: &Connection,
    task: &TaskRecord,
    revision: i64,
    text: &str,
    now: i64,
) -> Result<String, RepositoryError> {
    let work = running_work(db, task)?;
    let id = format!("author-message-{}", Uuid::new_v4().simple());
    db.execute(
        "INSERT INTO story_author_messages_v1(message_id,project_id,work_task_id,task_id,thread_id,intent_revision,author_text,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id,task.project_id.as_str(),work,task.task_id,task.thread_id,revision,text,now],
    ).map_err(storage_error)?;
    Ok(id)
}

pub(crate) fn running_work(db: &Connection, task: &TaskRecord) -> Result<String, RepositoryError> {
    let work: String = db.query_row(
        "SELECT w.id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2 AND w.kind='conversation' AND w.status='running'",
        params![task.task_id,task.project_id.as_str()], |row|row.get(0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    Ok(work)
}

pub(crate) fn validate(db: &Connection, record: &TaskIntentRecord) -> Result<(), RepositoryError> {
    if !crate::author_message::valid_id(&record.author_message_id) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_author_messages_v1 m JOIN story_tasks_v2 t ON t.task_id=m.task_id AND t.project_id=m.project_id AND t.thread_id=m.thread_id JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.work_task_id=m.work_task_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id AND w.project_id=m.project_id AND w.kind='conversation' WHERE m.message_id=?1 AND m.task_id=?2 AND m.intent_revision=?3 AND m.author_text=?4 AND m.created_at=?5 AND m.generation_run_id IS NULL AND t.operation='advisory')",
        params![record.author_message_id,record.task_id,record.intent_revision,record.author_text,record.created_at], |row|row.get(0),
    ).map_err(storage_error)?;
    if !valid {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
