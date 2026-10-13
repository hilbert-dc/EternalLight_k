//! Author-owned, same-Work temporary questions. No model or provider dispatch lives here.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, WorkTask};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_work_interludes_v1 (
 id TEXT PRIMARY KEY, work_id TEXT NOT NULL REFERENCES story_work_tasks_v1(id),
 parent_id TEXT REFERENCES story_work_interludes_v1(id), message_id TEXT NOT NULL UNIQUE,
 step_id TEXT NOT NULL UNIQUE REFERENCES story_work_steps_v1(id),
 control_revision INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
 auto_return INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('open','returned','invalidated')),
 request_json TEXT NOT NULL, return_command_id TEXT UNIQUE, return_request_json TEXT, created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS story_work_interludes_open_v1 ON story_work_interludes_v1(work_id,state);
"#;
#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BeginWorkInterlude {
    pub id: String,
    pub thread_id: String,
    pub expected_control_revision: i64,
    pub parent_id: Option<String>,
    pub auto_return: bool,
    pub message: crate::SubmitConversationMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReturnWorkInterlude {
    pub command_id: String,
    pub interlude_id: String,
    pub thread_id: String,
    pub expected_control_revision: i64,
    pub expected_revision: i64,
    pub automatic: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WorkInterlude {
    pub id: String,
    pub parent_id: Option<String>,
    pub message_id: String,
    pub step_id: String,
    pub control_revision: i64,
    pub revision: i64,
    pub auto_return: bool,
    pub phase: String,
}
pub(crate) fn top(db: &Connection, work: &str) -> Result<Option<WorkInterlude>, RepositoryError> {
    db.query_row("SELECT i.id,i.parent_id,i.message_id,i.step_id,i.control_revision,i.revision,i.auto_return,CASE WHEN (m.status IN ('failed','cancelled') OR m.error IS NOT NULL) AND s.status='queued' THEN 'blocked' ELSE s.status END FROM story_work_interludes_v1 i JOIN story_work_steps_v1 s ON s.id=i.step_id LEFT JOIN story_conversation_messages_v1 m ON m.message_id=i.message_id WHERE i.work_id=?1 AND i.state='open' ORDER BY i.rowid DESC LIMIT 1",[work],|r| {
        let status:String=r.get(7)?;
        Ok(WorkInterlude{id:r.get(0)?,parent_id:r.get(1)?,message_id:r.get(2)?,step_id:r.get(3)?,control_revision:r.get(4)?,revision:r.get(5)?,auto_return:r.get(6)?,phase:match status.as_str(){"completed"=>"answered","running"=>"answering","blocked"|"cancelled"=>"failed",_=>"waiting"}.into()})
    }).optional().map_err(storage_error)
}
pub(crate) fn assert_schedule(
    db: &Connection,
    work: &str,
    step: Option<&str>,
) -> Result<(), RepositoryError> {
    if let Some(active) = top(db, work)? {
        if step != Some(active.step_id.as_str()) {
            return Err(RepositoryError::TaskNotWritable);
        }
        settled(db, work)?;
    }
    Ok(())
}
fn settled(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.status IN ('queued','running','waiting')) OR EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_tool_operations_v1 o ON o.task_id=a.attempt_id WHERE s.work_task_id=?1 AND o.state='accepted') OR EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0)",[work],|r|r.get(0)).map_err(storage_error)?;
    if busy {
        return Err(RepositoryError::TaskNotWritable);
    }
    Ok(())
}
pub(crate) fn message_ready(
    db: &Connection,
    project: &ProjectId,
    thread: &str,
    id: &str,
) -> Result<bool, RepositoryError> {
    let row:Option<(String,i64,String,i64)>=db.query_row("SELECT w.id,w.control_revision,w.status,i.control_revision FROM story_work_interludes_v1 i JOIN story_work_tasks_v1 w ON w.id=i.work_id WHERE i.id=?1 AND w.project_id=?2 AND w.thread_id=?3 AND i.state='open'",params![id,project.as_str(),thread],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage_error)?;
    let Some((work, revision, status, expected)) = row else {
        return Ok(false);
    };
    if status != "running" || revision != expected || !top(db, &work)?.is_some_and(|v| v.id == id) {
        return Ok(false);
    }
    match settled(db, &work) {
        Ok(()) => Ok(true),
        Err(RepositoryError::TaskNotWritable) => Ok(false),
        Err(error) => Err(error),
    }
}
pub(crate) fn invalidate(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    // GenerationRun's activeTaskId identifies the original step, not a temporary
    // question. Revoke every active question in the same control transaction.
    let attempts = {
        let mut q=db.prepare("SELECT t.task_id FROM story_tasks_v2 t JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_interludes_v1 i ON i.step_id=a.step_id WHERE i.work_id=?1 AND i.state='open' AND t.status IN ('queued','running','waiting')").map_err(storage_error)?;
        q.query_map([work], |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?
    };
    for task in attempts {
        db.execute("UPDATE story_tasks_v2 SET status='cancelled',stage='cancelled',error='author_stopped_interlude',capability_revoked_at=unixepoch() WHERE task_id=?1",[&task]).map_err(storage_error)?;
        crate::work_tasks::finish_attempt(db, &crate::tasks::load_task(db, &task)?)?;
    }

    db.execute("UPDATE story_work_steps_v1 SET status='superseded' WHERE id IN (SELECT step_id FROM story_work_interludes_v1 WHERE work_id=?1 AND state IN ('open','invalidated')) AND status IN ('queued','blocked','cancelled')",[work]).map_err(storage_error)?;
    db.execute("UPDATE story_conversation_messages_v1 SET status='cancelled',revision=revision+1 WHERE message_id IN (SELECT message_id FROM story_work_interludes_v1 WHERE work_id=?1 AND state='open') AND status='queued'",[work]).map_err(storage_error)?;
    db.execute("UPDATE story_work_interludes_v1 SET state='invalidated',revision=revision+1 WHERE work_id=?1 AND state='open'",[work]).map_err(storage_error)?;
    Ok(())
}
pub(crate) fn is_attempt(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_interludes_v1 i JOIN story_work_attempts_v1 a ON a.step_id=i.step_id WHERE a.attempt_id=?1)",[task],|r|r.get(0)).map_err(storage_error)
}
pub(crate) fn freeze(
    db: &Connection,
    request: &crate::CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<bool, RepositoryError> {
    let Some(id) = target.get("interludeId").and_then(Value::as_str) else {
        return Ok(false);
    };
    let (project,thread,step,text,work):(String,String,String,String,String)=db.query_row("SELECT w.project_id,w.thread_id,i.step_id,json_extract(m.input_json,'$.text'),w.id FROM story_work_interludes_v1 i JOIN story_work_tasks_v1 w ON w.id=i.work_id JOIN story_conversation_messages_v1 m ON m.message_id=i.message_id WHERE i.id=?1 AND i.state='open'",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    if project != request.project_id.as_str()
        || thread != request.thread_id
        || request.operation != "advisory"
        || target["resumeStepId"] != step
        || !(target["conversationFocus"]["readOnly"] == true || (target["type"] == "edition" && target["readOnly"] == true))
        || manifest["authorIntent"] != text
        || !message_ready(db, &request.project_id, &thread, id)?
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let latest: i64 = db
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
            [&project],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    db.execute(
        "UPDATE story_work_steps_v1 SET input_revision=?2 WHERE id=?1 AND status='queued'",
        params![step, latest],
    )
    .map_err(storage_error)?;
    manifest["interludeBinding"] = json!({"id":id,"workId":work});
    Ok(true)
}
pub(crate) fn validate_source(
    db: &Connection,
    task: &crate::TaskRecord,
    manifest: &Value,
) -> Result<(), RepositoryError> {
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_interludes_v1 i JOIN story_work_attempts_v1 a ON a.step_id=i.step_id JOIN story_conversation_messages_v1 m ON m.message_id=i.message_id JOIN story_work_tasks_v1 w ON w.id=i.work_id WHERE a.attempt_id=?1 AND i.id=?2 AND w.id=?3 AND w.project_id=?4 AND w.thread_id=?5 AND json_extract(m.input_json,'$.text')=?6)",params![task.task_id,manifest["interludeBinding"]["id"].as_str(),manifest["interludeBinding"]["workId"].as_str(),task.project_id.as_str(),task.thread_id,manifest["authorIntent"].as_str()],|r|r.get(0)).map_err(storage_error)?;
    if !valid {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}


pub(crate) fn deadline(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    task: &str,
) -> Result<Option<i64>, RepositoryError> {
    let kind:Option<String>=db.query_row("SELECT w.kind FROM story_work_tasks_v1 w JOIN story_work_interludes_v1 i ON i.work_id=w.id JOIN story_work_attempts_v1 a ON a.step_id=i.step_id WHERE w.id=?1 AND w.project_id=?2 AND a.attempt_id=?3",params![work,project.as_str(),task],|r|r.get(0)).optional().map_err(storage_error)?;
    match kind.as_deref(){
        Some("generation")=>db.query_row("SELECT json_extract(state_json,'$.deadlineAt') FROM story_generation_runs_v1 WHERE 'work-'||run_id=?1 AND project_id=?2",params![work,project.as_str()],|r|r.get(0)).map_err(storage_error),
        Some("conversation"|"direct")=>Ok(crate::ordinary_work_budget::authorization(db,project,work,task)?.ok_or(RepositoryError::TaskBindingInvalid)?.deadline_at),
        _=>Err(RepositoryError::TaskBindingInvalid),
    }
}

/// Public work interludes capability; SQLite retains transaction ownership.
pub trait WorkInterludesRepository {
    fn begin_work_interlude(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: BeginWorkInterlude,
        now: i64,
    ) -> Result<WorkTask, RepositoryError>;
    fn return_work_interlude(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: ReturnWorkInterlude,
    ) -> Result<WorkTask, RepositoryError>;
}

impl WorkInterludesRepository for SqliteStoryRepository {
    fn begin_work_interlude(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: BeginWorkInterlude,
        now: i64,
    ) -> Result<WorkTask, RepositoryError> {
        if uuid::Uuid::parse_str(&request.id).is_err()
            || request.message.message_id != request.id
            || !matches!(&request.message.trigger,crate::MessageTrigger::Interlude{interlude_id,work_task_id} if interlude_id==&request.id && work_task_id==work)
            || match &request.message.input.media {
                Some(_) => request.message.input.focus.is_some(),
                None => request
                    .message
                    .input
                    .focus
                    .as_ref()
                    .is_none_or(|f| f["readOnly"] != true),
            }
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        let encoded = serde_json::to_string(&request).map_err(|_| RepositoryError::CorruptState)?;
        let prior: Option<(String, String)> = tx
            .query_row(
                "SELECT work_id,request_json FROM story_work_interludes_v1 WHERE id=?1",
                [&request.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        if let Some((owner, body)) = prior {
            if owner != work || body != encoded {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        } else {
            let before = crate::work_tasks::load_work_task(self, &tx, project, work)?;
            if before.thread_id != request.thread_id
                || before.control_revision != request.expected_control_revision
            {
                return Err(RepositoryError::WorkTaskStale);
            }
            if before.status != "running"
                || before.pause_after_step.is_some()
                || top(&tx, work)?.map(|v| v.id) != request.parent_id
            {
                return Err(RepositoryError::TaskNotWritable);
            }
            let depth:i64=tx.query_row("SELECT COUNT(*) FROM story_work_interludes_v1 WHERE work_id=?1 AND state='open'",[work],|r|r.get(0)).map_err(storage_error)?;
            if depth >= 8 {
                return Err(RepositoryError::TaskNotWritable);
            }
            crate::work_budget_storage::load(&tx, project, work)?;
            let step = format!("interlude-{}", request.id);
            let target = if let Some(media) = &request.message.input.media {
                let mut target =
                    crate::multimedia::message_target(&tx, project, &request.thread_id, media)?;
                target["interludeId"] = json!(request.id);
                target["readOnly"] = json!(true);
                target
            } else {
                json!({"type":"project","interludeId":request.id,"conversationFocus":request.message.input.focus})
            };
            tx.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,status) SELECT ?1,?2,COALESCE(MAX(ordinal),0)+1,'advisory',?3,(SELECT revision FROM story_workspaces_v2 WHERE project_id=?4),'queued' FROM story_work_steps_v1 WHERE work_task_id=?2",params![step,work,target.to_string(),project.as_str()]).map_err(storage_error)?;
            tx.execute("INSERT INTO story_work_interludes_v1(id,work_id,parent_id,message_id,step_id,control_revision,auto_return,state,request_json,created_at) VALUES(?1,?2,?3,?1,?4,?5,?6,'open',?7,?8)",params![request.id,work,request.parent_id,step,before.control_revision,request.auto_return,encoded,now]).map_err(storage_error)?;
            crate::conversation_messages::submit(
                &tx,
                project,
                &request.thread_id,
                request.message,
                now,
            )?;
            tx.execute(
                "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
                [work],
            )
            .map_err(storage_error)?;
        }
        let result = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        tx.commit()?;
        Ok(result)
    }

    fn return_work_interlude(
        &mut self,
        project: &ProjectId,
        work: &str,
        request: ReturnWorkInterlude,
    ) -> Result<WorkTask, RepositoryError> {
        if uuid::Uuid::parse_str(&request.command_id).is_err() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        let replay:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_interludes_v1 WHERE id=?1 AND work_id=?2 AND state='returned' AND return_command_id=?3)",params![request.interlude_id,work,request.command_id],|r|r.get(0)).map_err(storage_error)?;
        let before = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        let encoded = serde_json::to_string(&request).map_err(|_| RepositoryError::CorruptState)?;
        if replay {
            let prior: String = tx
                .query_row(
                    "SELECT return_request_json FROM story_work_interludes_v1 WHERE id=?1",
                    [&request.interlude_id],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if prior != encoded {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        if !replay {
            let active = top(&tx, work)?.ok_or(RepositoryError::TaskNotWritable)?;
            if before.thread_id != request.thread_id
                || before.control_revision != request.expected_control_revision
                || active.control_revision != before.control_revision
                || active.id != request.interlude_id
                || active.revision != request.expected_revision
            {
                return Err(RepositoryError::WorkTaskStale);
            }
            if before.status != "running"
                || (request.automatic && (!active.auto_return || active.phase != "answered"))
            {
                return Err(RepositoryError::TaskNotWritable);
            }
            settled(&tx, work)?;
            crate::work_tasks::assert_inputs_current(&tx, work)?;
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_task_intents_v1 i ON i.task_id=a.attempt_id WHERE s.work_task_id=?1 AND i.status='pending')",[work],|r|r.get(0)).map_err(storage_error)?;
            if pending {
                return Err(RepositoryError::TaskInputStale);
            }
            let original_attempts = {
                let mut query=tx.prepare("SELECT s.current_attempt_id FROM story_work_steps_v1 s WHERE s.work_task_id=?1 AND s.status IN ('queued','blocked') AND s.current_attempt_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM story_work_interludes_v1 i WHERE i.step_id=s.id)").map_err(storage_error)?;
                query
                    .query_map([work], |r| r.get::<_, String>(0))
                    .map_err(storage_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(storage_error)?
            };
            for attempt in original_attempts {
                crate::task_input::assert_task_inputs_current(
                    &tx,
                    &crate::tasks::load_task(&tx, &attempt)?,
                )?;
            }

            tx.execute("UPDATE story_work_steps_v1 SET status='superseded' WHERE id=?1 AND status IN ('queued','blocked','cancelled')",[&active.step_id]).map_err(storage_error)?;
            tx.execute("UPDATE story_conversation_messages_v1 SET status='cancelled',revision=revision+1 WHERE message_id=?1 AND status='queued'",[&active.message_id]).map_err(storage_error)?;
            tx.execute("UPDATE story_work_interludes_v1 SET state='returned',revision=revision+1,return_command_id=?2,return_request_json=?3 WHERE id=?1",params![active.id,request.command_id,encoded]).map_err(storage_error)?;
            crate::work_tasks::refresh_work_task(&tx, work)?;
            if top(&tx, work)?.is_none() && before.kind == "generation" {
                tx.execute("UPDATE story_work_tasks_v1 SET status=(SELECT json_extract(state_json,'$.status') FROM story_generation_runs_v1 WHERE 'work-'||run_id=?1) WHERE id=?1",[work]).map_err(storage_error)?;
            }
            tx.execute(
                "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
                [work],
            )
            .map_err(storage_error)?;
        }
        let result = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        // Closing the final answered interlude of a completed Work dispatches
        // nothing. Only a return that leaves work to execute needs allowance.
        // Keep this inside the transaction so rejected returns roll back fully.
        if !replay && result.status != "completed" {
            let budget = crate::work_budget_storage::load(&tx, project, work)?;
            if budget.continuation_block_reason().is_some() {
                return Err(RepositoryError::WorkUsageUnsettled);
            }
        }
        tx.commit()?;
        Ok(result)
    }
}
