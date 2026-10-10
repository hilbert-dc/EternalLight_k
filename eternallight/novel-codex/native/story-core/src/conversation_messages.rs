//! Persistent author submissions. Transport state is separate from Work completion.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_conversation_messages_v1 (
 message_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id),
 thread_id TEXT NOT NULL REFERENCES story_conversations_v1(thread_id),
 original_json TEXT NOT NULL,
 input_json TEXT NOT NULL,
 trigger_json TEXT NOT NULL,
 session_id TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('queued','claimed','sending','delivered','failed','cancelled')),
 revision INTEGER NOT NULL DEFAULT 0,
 requires_confirmation INTEGER NOT NULL DEFAULT 0,
 position INTEGER NOT NULL,
 task_id TEXT REFERENCES story_tasks_v2(task_id),
 turn_id TEXT,
 error TEXT,
 created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS story_conversation_messages_order_v1
 ON story_conversation_messages_v1(project_id,thread_id,position,message_id);
CREATE UNIQUE INDEX IF NOT EXISTS story_conversation_messages_active_v1
 ON story_conversation_messages_v1(thread_id) WHERE status IN ('claimed','sending');
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationMessageInput {
    pub text: String,
    pub focus: Option<Value>,
    pub connection_id: Option<String>,
    pub media: Option<Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MessageTrigger {
    Immediate,
    Interlude { #[serde(rename="interludeId")] interlude_id: String, #[serde(rename="workTaskId")] work_task_id: String },
    AfterWork {
        #[serde(rename = "workTaskId")]
        work_task_id: String,
        #[serde(rename = "controlRevision")]
        control_revision: i64,
    },
    AfterTurn {
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        #[serde(rename = "controlRevision")]
        control_revision: i64,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitConversationMessage {
    pub message_id: String,
    pub input: ConversationMessageInput,
    pub trigger: MessageTrigger,
    pub session_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum ChangeConversationMessage {
    Cancel {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
    },
    Edit {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        input: ConversationMessageInput,
    },
    Move {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        direction: i64,
    },
    Continue {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Claim {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Reconcile {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
    },
    Sending {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Delivered {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "turnId")]
        turn_id: String,
    },
    Fail {
        #[serde(rename = "expectedRevision")]
        expected_revision: i64,
        #[serde(rename = "sessionId")]
        session_id: String,
        error: String,
    },
}
fn invalid() -> RepositoryError {
    RepositoryError::TaskBindingInvalid
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 160 && s.trim() == s && !s.chars().any(char::is_control)
}
fn input_valid(
    db: &Connection, thread: &str,
    input: &ConversationMessageInput,
    project: &ProjectId,
) -> Result<(), RepositoryError> {
    if let Some(media)=&input.media {
        if input.focus.is_some() {return Err(invalid());}
        let target=crate::multimedia::message_target(db,project,thread,media)?;
        crate::author_message::validate_text(db,project.as_str(),&target,&input.text)?;
    }
    if (input.media.is_none() && input.text.trim().is_empty())
        || input.text.len() > crate::task_author_intent::MAX_AUTHOR_HISTORY_BYTES
        || input.connection_id.as_deref().is_some_and(|s| !id(s))
    {
        return Err(invalid());
    }
    if let Some(focus) = &input.focus {
        if !focus.is_object()
            || focus["reference"]
                .get("projectId")
                .is_some_and(|p| p != project.as_str())
            || focus["excerpts"]
                .as_array()
                .is_some_and(|xs| xs.iter().any(|x| x["projectId"] != project.as_str()))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn owns(
    db: &Connection,
    project: &ProjectId,
    thread: &str,
    writable: bool,
) -> Result<(), RepositoryError> {
    if !id(thread) {
        return Err(invalid());
    }
    let archived: Option<bool> = db
        .query_row(
            "SELECT is_archived FROM story_conversations_v1 WHERE project_id=?1 AND thread_id=?2",
            params![project.as_str(), thread],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    match archived {
        Some(false) => Ok(()),
        Some(true) if !writable => Ok(()),
        _ => Err(invalid()),
    }
}
fn load(
    db: &Connection,
    project: &ProjectId,
    thread: &str,
    message: &str,
) -> Result<Value, RepositoryError> {
    db.query_row("SELECT input_json,trigger_json,session_id,status,revision,position,task_id,turn_id,error,created_at FROM story_conversation_messages_v1 WHERE message_id=?1 AND project_id=?2 AND thread_id=?3",params![message,project.as_str(),thread],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,Option<String>>(8)?,r.get::<_,i64>(9)?))).optional().map_err(storage_error)?.map(|(input,trigger,session,status,revision,position,task,turn,error,created)|{
        Ok(json!({"id":message,"projectId":project.as_str(),"threadId":thread,"input":serde_json::from_str::<Value>(&input).map_err(|_|RepositoryError::CorruptState)?,"trigger":serde_json::from_str::<Value>(&trigger).map_err(|_|RepositoryError::CorruptState)?,"sessionId":session,"status":status,"revision":revision,"position":position,"taskId":task,"turnId":turn,"error":error,"createdAt":created}))
    }).unwrap_or(Err(RepositoryError::TaskNotFound))
}
fn trigger_ready(
    db: &Connection,
    project: &ProjectId,
    thread: &str,
    trigger: &MessageTrigger,
    accepting: bool,
) -> Result<bool, RepositoryError> {
    match trigger {
        MessageTrigger::Immediate => db.query_row(
            "SELECT NOT EXISTS(SELECT 1 FROM story_work_interludes_v1 i JOIN story_work_tasks_v1 w ON w.id=i.work_id WHERE w.project_id=?1 AND w.thread_id=?2 AND i.state='open')",
            params![project.as_str(),thread],|r|r.get(0)).map_err(storage_error),
        MessageTrigger::Interlude { interlude_id, .. } => crate::work_interludes::message_ready(db,project,thread,interlude_id),
        MessageTrigger::AfterWork {
            work_task_id,
            control_revision,
        } => {
            let row:Option<(String,i64)>=db.query_row("SELECT status,control_revision FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2 AND thread_id=?3",params![work_task_id,project.as_str(),thread],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
            let (status, revision) = row.ok_or_else(invalid)?;
            if accepting && revision != *control_revision {
                return Err(RepositoryError::WorkTaskStale);
            }
            Ok(revision == *control_revision && status == "completed")
        }
        MessageTrigger::AfterTurn {
            task_id,
            turn_id,
            control_revision,
        } => {
            let row: Option<(String, Option<String>, String, i64)> = db.query_row(
                "SELECT t.status,t.turn_id,w.status,w.control_revision FROM story_tasks_v2 t JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE t.task_id=?1 AND t.project_id=?2 AND t.thread_id=?3 AND w.thread_id=?3",
                params![task_id,project.as_str(),thread], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))
            ).optional().map_err(storage_error)?;
            let (status, bound, work_status, revision) = row.ok_or_else(invalid)?;
            if bound.as_deref() != Some(turn_id) {
                return Err(invalid());
            }
            if accepting && revision != *control_revision {
                return Err(RepositoryError::WorkTaskStale);
            }
            Ok(status == "completed"
                && ["running", "completed"].contains(&work_status.as_str())
                && revision == *control_revision)
        }
    }
}
impl SqliteStoryRepository {
    pub fn submit_conversation_message(
        &mut self,
        project: &ProjectId,
        thread: &str,
        request: SubmitConversationMessage,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let tx=self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage_error)?;
        let result=project_message(&tx,submit(&tx,project,thread,request,now)?)?;
        tx.commit().map_err(storage_error)?; Ok(result)
    }

    pub fn conversation_messages(
        &self,
        project: &ProjectId,
        thread: &str,
    ) -> Result<Vec<Value>, RepositoryError> {
        owns(&self.connection, project, thread, false)?;
        let mut stmt=self.connection.prepare("SELECT message_id FROM story_conversation_messages_v1 WHERE project_id=?1 AND thread_id=?2 AND status NOT IN ('delivered','cancelled') ORDER BY position,message_id").map_err(storage_error)?;
        let ids = stmt
            .query_map(params![project.as_str(), thread], |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let mut messages:Vec<Value> = ids.into_iter()
            .map(|id| {
                load(&self.connection, project, thread, &id)
                    .and_then(|message| project_message(&self.connection, message))
            })
            .collect::<Result<_,_>>()?;
        messages.extend(crate::task_intent_messages::pending(&self.connection,project,thread)?);
        Ok(messages)
    }
    pub fn change_conversation_message(
        &mut self,
        project: &ProjectId,
        thread: &str,
        message: &str,
        change: ChangeConversationMessage,
    ) -> Result<Value, RepositoryError> {
        use ChangeConversationMessage::*;
        if let Some(intent)=crate::task_intent_messages::read(&self.connection,project,thread,message)? {
            owns(&self.connection,project,thread,false)?;
            return match change {
                ChangeConversationMessage::Reconcile{expected_revision} if intent["revision"]==expected_revision=>Ok(intent),
                _=>Err(RepositoryError::TaskNotWritable),
            };
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        owns(&tx, project, thread, true)?;
        let prior = load(&tx, project, thread, message)?;
        let revision = match &change {
            Cancel { expected_revision }
            | Edit {
                expected_revision, ..
            }
            | Move {
                expected_revision, ..
            }
            | Continue {
                expected_revision, ..
            }
            | Claim {
                expected_revision, ..
            }
            | Reconcile { expected_revision }
            | Sending {
                expected_revision, ..
            }
            | Delivered {
                expected_revision, ..
            }
            | Fail {
                expected_revision, ..
            } => *expected_revision,
        };
        if prior["revision"] != revision {
            return Err(RepositoryError::TaskInputStale);
        }
        let session = match &change {
            Continue { session_id, .. }
            | Claim { session_id, .. }
            | Sending { session_id, .. }
            | Delivered { session_id, .. }
            | Fail { session_id, .. } => Some(session_id),
            _ => None,
        };
        if session.is_some_and(|s| !id(s)) {
            return Err(invalid());
        }
        if !matches!(&change, Continue { .. })
            && session.is_some_and(|s| prior["sessionId"] != s.as_str())
        {
            return Err(invalid());
        }
        let status = prior["status"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        match change {
            Cancel { .. } | Edit { .. } | Move { .. } | Continue { .. } if status != "queued" => {
                return Err(RepositoryError::TaskNotWritable);
            }
            Claim { .. } if status != "queued" => return Err(RepositoryError::TaskNotWritable),
            Sending { .. } if status != "claimed" => {
                return Err(RepositoryError::TaskNotWritable);
            }
            Delivered { .. } if status != "sending" => {
                return Err(RepositoryError::TaskNotWritable);
            }
            Fail { .. } if !matches!(status, "claimed" | "sending") => {
                return Err(RepositoryError::TaskNotWritable);
            }
            _ => {}
        }
        match change {
            Reconcile { .. } => {
                if status == "claimed" && prior["taskId"].is_null() {
                    // Atomic creation has no bound task: no dispatch could have begun.
                    // Bumping revision also rejects a late old create request.
                    tx.execute("UPDATE story_conversation_messages_v1 SET status='queued',error=NULL,requires_confirmation=1 WHERE message_id=?1",[message]).map_err(storage_error)?;
                } else if let Some(task) = prior["taskId"].as_str() {
                    let task = crate::tasks::load_task(&tx, task)?;
                    if let Some(turn) = task.turn_id {
                        tx.execute("UPDATE story_conversation_messages_v1 SET status='delivered',turn_id=?2,error=NULL WHERE message_id=?1",params![message,turn]).map_err(storage_error)?;
                    } else if matches!(status, "claimed" | "sending") {
                        // Explicit Work continuation keeps the same admitted Step.
                        // A later proven Turn can settle its original message;
                        // merely creating another attempt is not delivery evidence.
                        let successor: Option<(String,String)> = tx.query_row(
                            "SELECT t.task_id,t.turn_id FROM story_work_attempts_v1 origin
                             JOIN story_work_attempts_v1 next ON next.step_id=origin.step_id AND next.attempt_number>origin.attempt_number
                             JOIN story_tasks_v2 t ON t.task_id=next.attempt_id
                             WHERE origin.attempt_id=?1 AND t.project_id=?2 AND t.thread_id=?3 AND t.operation='advisory'
                               AND t.turn_id IS NOT NULL ORDER BY next.attempt_number LIMIT 1",
                            params![task.task_id,project.as_str(),thread], |r|Ok((r.get(0)?,r.get(1)?)),
                        ).optional().map_err(storage_error)?;
                        if let Some((attempt,turn))=successor {
                            tx.execute("UPDATE story_conversation_messages_v1 SET status='delivered',task_id=?2,turn_id=?3,error=NULL WHERE message_id=?1",params![message,attempt,turn]).map_err(storage_error)?;
                        }
                    }
                }
            }
            Cancel { .. } => {
                tx.execute("UPDATE story_conversation_messages_v1 SET status='cancelled' WHERE message_id=?1",[message]).map_err(storage_error)?;
            }
            Edit { input, .. } => {
                input_valid(&tx, thread, &input, project)?;
                if prior["trigger"]["kind"] == "interlude" {
                    if !prior["input"]["media"].is_null() {
                        if input.media.as_ref()!=Some(&prior["input"]["media"]) || input.focus.is_some() {
                            return Err(RepositoryError::TaskBindingInvalid);
                        }
                    } else {
                    if input.media.is_some() || input.focus.as_ref().is_none_or(|focus|focus["readOnly"]!=true) {
                        return Err(RepositoryError::TaskBindingInvalid);
                    }
                    tx.execute("UPDATE story_work_steps_v1 SET target_json=json_set(target_json,'$.conversationFocus',json(?2)) WHERE id=(SELECT step_id FROM story_work_interludes_v1 WHERE message_id=?1 AND state='open') AND status='queued'",
                        params![message,serde_json::to_string(&input.focus).map_err(|_|invalid())?]).map_err(storage_error)?;
                    }
                }
                tx.execute(
                    "UPDATE story_conversation_messages_v1 SET input_json=?2 WHERE message_id=?1",
                    params![
                        message,
                        serde_json::to_string(&input).map_err(|_| invalid())?
                    ],
                )
                .map_err(storage_error)?;
            }
            Move { direction, .. } => {
                if direction != 1 && direction != -1 {
                    return Err(invalid());
                }
                let mut stmt=tx.prepare("SELECT message_id FROM story_conversation_messages_v1 WHERE thread_id=?1 AND status='queued' ORDER BY position,message_id").map_err(storage_error)?;
                let mut ids = stmt
                    .query_map([thread], |r| r.get::<_, String>(0))
                    .map_err(storage_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(storage_error)?;
                let index = ids.iter().position(|x| x == message).ok_or_else(invalid)?;
                let next = index as i64 + direction;
                if next < 0 || next >= ids.len() as i64 {
                    return Err(invalid());
                }
                ids.swap(index, next as usize);
                for (position, id) in ids.iter().enumerate() {
                    tx.execute("UPDATE story_conversation_messages_v1 SET position=?2,revision=revision+1 WHERE message_id=?1",params![id,position as i64]).map_err(storage_error)?;
                }
            }
            Continue { session_id, .. } => {
                if matches!(
                    prior["trigger"]["kind"].as_str(),
                    Some("afterWork" | "afterTurn")
                ) {
                    let (sql, identity) = if prior["trigger"]["kind"] == "afterWork" {
                        (
                            "SELECT control_revision FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2 AND thread_id=?3",
                            prior["trigger"]["workTaskId"].as_str(),
                        )
                    } else {
                        (
                            "SELECT w.control_revision FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id JOIN story_work_attempts_v1 a ON a.step_id=s.id WHERE a.attempt_id=?1 AND w.project_id=?2 AND w.thread_id=?3",
                            prior["trigger"]["taskId"].as_str(),
                        )
                    };
                    let revision: i64 = tx
                        .query_row(
                            sql,
                            params![identity.ok_or_else(invalid)?, project.as_str(), thread],
                            |r| r.get(0),
                        )
                        .map_err(storage_error)?;
                    let mut trigger = prior["trigger"].clone();
                    trigger["controlRevision"] = json!(revision);
                    tx.execute("UPDATE story_conversation_messages_v1 SET trigger_json=?2 WHERE message_id=?1",params![message,trigger.to_string()]).map_err(storage_error)?;
                }
                tx.execute(
                    "UPDATE story_conversation_messages_v1 SET session_id=?2,requires_confirmation=0 WHERE message_id=?1",
                    params![message, session_id],
                )
                .map_err(storage_error)?;
            }
            Claim { .. } => {
                let confirm:bool=tx.query_row("SELECT requires_confirmation FROM story_conversation_messages_v1 WHERE message_id=?1",[message],|r|r.get(0)).map_err(storage_error)?;
                if confirm {return Err(RepositoryError::TaskNotWritable);}
                let trigger: MessageTrigger =
                    serde_json::from_value(prior["trigger"].clone()).map_err(|_| invalid())?;
                if !trigger_ready(&tx, project, thread, &trigger, false)? {
                    return Err(RepositoryError::TaskNotWritable);
                }
                let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_conversation_messages_v1 WHERE thread_id=?1 AND status IN ('claimed','sending')) OR EXISTS(SELECT 1 FROM story_tasks_v2 WHERE project_id=?2 AND thread_id=?1 AND status IN ('queued','running','waiting'))",params![thread,project.as_str()],|r|r.get(0)).map_err(storage_error)?;
                if active {
                    return Err(RepositoryError::TaskNotWritable);
                }
                tx.execute("UPDATE story_conversation_messages_v1 SET status='claimed' WHERE message_id=?1",[message]).map_err(storage_error)?;
            }
            Sending { .. } => {
                let task = prior["taskId"].as_str().ok_or_else(invalid)?;
                let allowed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE t.task_id=?1 AND t.status IN ('queued','running','waiting') AND t.capability_revoked_at IS NULL AND w.status='running')",[task],|r|r.get(0)).map_err(storage_error)?;
                if !allowed {
                    return Err(RepositoryError::TaskNotWritable);
                }
                tx.execute("UPDATE story_conversation_messages_v1 SET status='sending' WHERE message_id=?1",[message]).map_err(storage_error)?;
            }
            Delivered { turn_id, .. } => {
                if !id(&turn_id) {
                    return Err(invalid());
                }
                let bound:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_tasks_v2 WHERE task_id=?1 AND turn_id=?2)",params![prior["taskId"].as_str(),turn_id],|r|r.get(0)).map_err(storage_error)?;
                if !bound {
                    return Err(invalid());
                }
                tx.execute("UPDATE story_conversation_messages_v1 SET status='delivered',turn_id=?2 WHERE message_id=?1",params![message,turn_id]).map_err(storage_error)?;
            }
            Fail { error, .. } => {
                if error.trim().is_empty() {
                    return Err(invalid());
                }
                tx.execute(
                    "UPDATE story_conversation_messages_v1 SET error=?2 WHERE message_id=?1",
                    params![message, error],
                )
                .map_err(storage_error)?;
            }
        }
        tx.execute(
            "UPDATE story_conversation_messages_v1 SET revision=revision+1 WHERE message_id=?1",
            [message],
        )
        .map_err(storage_error)?;
        let result = project_message(&tx, load(&tx, project, thread, message)?)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

// This runs inside the same transaction that creates and authorizes the Attempt.
// A lost create response therefore retains the exact task identity for reconciliation.
pub(crate) fn bind_submission(
    db: &Connection,
    request: &crate::CreateTaskRequest,
    target: &Value,
) -> Result<(), RepositoryError> {
    let Some(binding) = target.get("messageDispatch") else {
        return Ok(());
    };
    let message = binding["messageId"].as_str().ok_or_else(invalid)?;
    let session = binding["sessionId"].as_str().ok_or_else(invalid)?;
    let revision = binding["revision"].as_i64().ok_or_else(invalid)?;
    let prior = load(db, &request.project_id, &request.thread_id, message)?;
    let manifest: Value = serde_json::from_str(&request.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    if prior["status"] != "claimed"
        || prior["revision"] != revision
        || prior["sessionId"] != session
        || !prior["taskId"].is_null()
        || request.operation != "advisory"
        || prior["input"]["text"] != manifest["authorIntent"]
    {
        return Err(invalid());
    }
    if !prior["input"]["media"].is_null() {
        let expected=crate::multimedia::message_target(db,&request.project_id,&request.thread_id,&prior["input"]["media"])?;
        if expected.as_object().unwrap().iter().any(|(key,value)|target.get(key)!=Some(value)) {return Err(invalid());}
    } else if target["type"]=="edition" {return Err(invalid());}
    db.execute("UPDATE story_conversation_messages_v1 SET task_id=?2,revision=revision+1 WHERE message_id=?1", params![message,request.task_id]).map_err(storage_error)?;
    Ok(())
}
impl SqliteStoryRepository {
    pub fn conversation_message(
        &self,
        project: &ProjectId,
        thread: &str,
        message: &str,
    ) -> Result<Value, RepositoryError> {
        owns(&self.connection, project, thread, false)?;
        if let Some(intent)=crate::task_intent_messages::read(&self.connection,project,thread,message)? {return Ok(intent);}
        project_message(
            &self.connection,
            load(&self.connection, project, thread, message)?,
        )
    }
}

fn project_message(db: &Connection, mut message: Value) -> Result<Value, RepositoryError> {
    let confirm:bool=db.query_row("SELECT requires_confirmation FROM story_conversation_messages_v1 WHERE message_id=?1",[message["id"].as_str().ok_or_else(invalid)?],|r|r.get(0)).map_err(storage_error)?;
    message["requiresContinue"]=json!(confirm);

    if message["status"] != "queued" {
        return Ok(message);
    }
    let trigger: MessageTrigger =
        serde_json::from_value(message["trigger"].clone()).map_err(|_| invalid())?;
    let project = ProjectId::parse(message["projectId"].as_str().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let ready = trigger_ready(
        db,
        &project,
        message["threadId"].as_str().ok_or_else(invalid)?,
        &trigger,
        false,
    )?;
    message["triggerReady"] = json!(ready);
    let control = match trigger {
        MessageTrigger::Immediate | MessageTrigger::Interlude { .. } => None,
        MessageTrigger::AfterWork {
            work_task_id,
            control_revision,
        } => {
            let (status, revision): (String, i64) = db
                .query_row(
                    "SELECT status,control_revision FROM story_work_tasks_v1 WHERE id=?1",
                    [work_task_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(storage_error)?;
            Some((status, revision, control_revision))
        }
        MessageTrigger::AfterTurn {
            task_id,
            control_revision,
            ..
        } => {
            let (status, revision): (String, i64) = db.query_row(
                "SELECT w.status,w.control_revision FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id JOIN story_work_attempts_v1 a ON a.step_id=s.id WHERE a.attempt_id=?1", [task_id],
                |r| Ok((r.get(0)?, r.get(1)?))).map_err(storage_error)?;
            Some((status, revision, control_revision))
        }
    };
    if let Some((status, revision, expected)) = control {
        message["holdReason"] = if revision != expected {
            json!("authorControlChanged")
        } else if !ready {
            json!(status)
        } else {
            Value::Null
        };
    }
    Ok(message)
}

pub(crate) fn submit(db:&Connection,project:&ProjectId,thread:&str,request:SubmitConversationMessage,now:i64)->Result<Value,RepositoryError>{
        if !id(&request.message_id) || !id(&request.session_id) {
            return Err(invalid());
        }
        let intent_id:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_intents_v1 WHERE author_message_id=?1)",[&request.message_id],|r|r.get(0)).map_err(storage_error)?;
        if intent_id {return Err(invalid());}
        input_valid(db, thread, &request.input, project)?;
        owns(db, project, thread, true)?;
        // The retry identity includes the original immutable submission, not its later edit.
        let original =
            json!({"messageId":request.message_id,"input":request.input,"trigger":request.trigger})
                .to_string();
        let prior:Option<(String,String,String)>=db.query_row("SELECT project_id,thread_id,original_json FROM story_conversation_messages_v1 WHERE message_id=?1",[&request.message_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage_error)?;
        if let Some((p, t, o)) = prior {
            if p != project.as_str() || t != thread || o != original {
                return Err(invalid());
            }
            return load(db, project, thread, &request.message_id);
        }
        if let MessageTrigger::Interlude {interlude_id,work_task_id}=&request.trigger {
            let bound:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_interludes_v1 WHERE id=?1 AND message_id=?2 AND work_id=?3 AND state='open')",params![interlude_id,request.message_id,work_task_id],|r|r.get(0)).map_err(storage_error)?;
            if !bound{return Err(invalid());}
        }
        trigger_ready(db, project, thread, &request.trigger, true)?;
        db.execute("INSERT INTO story_conversation_messages_v1(message_id,project_id,thread_id,original_json,input_json,trigger_json,session_id,status,position,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'queued',(SELECT COALESCE(MAX(position),0)+1 FROM story_conversation_messages_v1 WHERE thread_id=?3),?8)",params![request.message_id,project.as_str(),thread,original,serde_json::to_string(&request.input).map_err(|_|invalid())?,serde_json::to_string(&request.trigger).map_err(|_|invalid())?,request.session_id,now]).map_err(storage_error)?;
        let result = load(db, project, thread, &request.message_id)?;
        Ok(result)
}
