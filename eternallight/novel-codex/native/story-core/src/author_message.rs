use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{CreateTaskRequest, RepositoryError, TaskRecord};

pub(crate) fn record(
    db: &Connection,
    request: &CreateTaskRequest,
    message_id: &str,
    now: i64,
) -> Result<(), RepositoryError> {
    if !valid_id(message_id) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let manifest: Value = serde_json::from_str(&request.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let text = manifest["authorIntent"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    db.execute(
        "INSERT INTO story_author_messages_v1(message_id,project_id,work_task_id,task_id,thread_id,author_text,created_at,intent_revision)
         VALUES(?1,?2,?3,?4,?5,?6,?7,0)",
        params![message_id,request.project_id.as_str(),format!("work-{}",request.task_id),request.task_id,request.thread_id,text,now],
    ).map_err(storage_error)?;
    Ok(())
}

/// A root constructor without execution authority is not a persisted author message.
/// Funded roots must always have a record; absence is corruption, never a fallback.
pub(crate) fn validate_root(
    db: &Connection,
    source: &TaskRecord,
    manifest: &Value,
) -> Result<(), RepositoryError> {
    let stored: Option<(String,String,String,String)> = db.query_row(
        "SELECT message_id,project_id,thread_id,author_text FROM story_author_messages_v1 WHERE task_id=?1 AND work_task_id='work-'||?1 AND intent_revision=0",
        [&source.task_id], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).optional().map_err(storage_error)?;
    if let Some((id, project, thread, text)) = stored {
        if project != source.project_id.as_str()
            || thread != source.thread_id
            || manifest["authorMessageId"].as_str() != Some(id.as_str())
            || manifest["authorIntent"].as_str() != Some(text.as_str())
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    } else if has_root_authorization(db, &source.task_id)?
        || manifest.get("authorMessageId").is_some()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn validate_execution(
    db: &Connection,
    task: &TaskRecord,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if crate::generation_author_source::owns_task(db, &task.task_id)?
        || crate::planning_review_author_source::owns_task(db, &task.task_id)?
        || task.operation == "reviewPlanningSeat"
        || target.get("reviewRunId").is_some()
        || target.get("runId").is_some()
        || identity(db, &task.task_id)?.is_some()
        || has_root_authorization(db, &task.task_id)?
    {
        crate::task_author_intent::source_intent(db, &task.task_id, /*revision*/ 0)?;
    }
    Ok(())
}

pub(crate) fn valid_id(message_id: &str) -> bool {
    !message_id.is_empty()
        && message_id.len() <= 160
        && message_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn has_root_authorization(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_budget_authorizations_v1 b JOIN story_work_steps_v1 s ON s.work_task_id=b.work_task_id JOIN story_work_attempts_v1 a ON a.step_id=s.id WHERE a.attempt_id=?1 AND b.initial_request_digest IS NOT NULL)",
        [task], |row|row.get(0),
    ).map_err(storage_error)
}

pub(crate) fn identity(
    db: &Connection,
    source_id: &str,
) -> Result<Option<String>, RepositoryError> {
    db.query_row(
        "SELECT m.message_id FROM story_author_messages_v1 m JOIN story_work_steps_v1 s ON s.work_task_id=m.work_task_id JOIN story_work_attempts_v1 a ON a.step_id=s.id WHERE a.attempt_id=?1 AND m.intent_revision=0",
        [source_id], |row|row.get(0),
    ).optional().map_err(storage_error)
}
