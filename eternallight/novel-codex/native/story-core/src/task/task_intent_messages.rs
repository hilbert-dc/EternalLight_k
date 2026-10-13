//! Conversation transport projection of the authoritative author-intent ledger.
use crate::{ProjectId, RepositoryError};
use crate::authoring_runs::storage_error;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value,json};

pub(crate) fn read(db:&Connection,project:&ProjectId,thread:&str,id:&str)->Result<Option<Value>,RepositoryError>{
    let identity:Option<(String,i64,Option<String>)>=db.query_row(
        "SELECT i.task_id,i.intent_revision,t.turn_id FROM story_task_intents_v1 i JOIN story_tasks_v2 t ON t.task_id=i.task_id WHERE i.author_message_id=?1 AND t.project_id=?2 AND t.thread_id=?3",
        params![id,project.as_str(),thread],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage_error)?;
    let Some((task,revision,turn))=identity else{return Ok(None)};
    let intent=crate::task_intents::load_intent(db,&task,revision)?;
    let continued:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t WHERE json_extract(t.input_manifest_json,'$.authorIntentBinding.sourceTaskId')=?1 AND json_extract(t.input_manifest_json,'$.authorIntentBinding.intentRevision')>=?2)",params![task,revision],|r|r.get(0)).map_err(storage_error)?;
    let delivered=intent.status=="delivered" || (matches!(intent.status.as_str(),"notSent"|"recorded") && continued);
    Ok(Some(json!({"id":id,"projectId":project.as_str(),"threadId":thread,
        "input":{"text":crate::task_intents::display_author_text(&intent.author_text,&task),"focus":null,"connectionId":null,"media":null},
        "trigger":{"kind":"steer","taskId":task},"sessionId":"author-intent",
        "status":if delivered{"delivered"}else if matches!(intent.status.as_str(),"notSent"|"recorded"){"failed"}else{"sending"},"revision":if delivered{1}else{0},
        "position":intent.created_at,"taskId":task,"turnId":turn,"createdAt":intent.created_at,
        "error":if delivered{Value::Null}else if intent.status=="recorded"{json!("作者回答已保存，回传尚未确认；继续原任务时会读取，不会自动重复发送")}else if intent.status=="notSent"{json!("追加要求尚未发送，已保留；继续原任务时会一并处理")}else{json!("追加指令交付尚未确认，不会自动重发")},
        "requiresContinue":false,"triggerReady":false})))
}
pub(crate) fn pending(db:&Connection,project:&ProjectId,thread:&str)->Result<Vec<Value>,RepositoryError>{
    let mut stmt=db.prepare("SELECT i.author_message_id FROM story_task_intents_v1 i JOIN story_tasks_v2 t ON t.task_id=i.task_id WHERE t.project_id=?1 AND t.thread_id=?2 AND i.status IN ('pending','notSent','recorded') ORDER BY i.created_at,i.author_message_id").map_err(storage_error)?;
    let ids=stmt.query_map(params![project.as_str(),thread],|r|r.get::<_,String>(0)).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
    ids.into_iter().map(|id|read(db,project,thread,&id)?.ok_or(RepositoryError::CorruptState)).collect::<Result<Vec<_>,_>>().map(|rows|rows.into_iter().filter(|row|row["status"]!="delivered").collect())
}
