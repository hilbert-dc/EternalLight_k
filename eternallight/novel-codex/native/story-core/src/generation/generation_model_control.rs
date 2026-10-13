//! Model-selected pause uses the same generation state transaction and author scope.
use crate::{RepositoryError, SqliteStoryRepository, TaskRecord, authoring_runs::storage_error};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) const SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS story_model_run_controls_v1(
 task_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, request_json TEXT NOT NULL,
 receipt_json TEXT NOT NULL, PRIMARY KEY(task_id,idempotency_key));"#;
#[derive(Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRunPauseRequest {
    pub run_id: String,
    pub expected_version: i64,
    pub expected_intent_revision: i64,
    pub instruction: String,
    pub idempotency_key: String,
}
pub(crate) fn validate(
    db: &Connection,
    task: &TaskRecord,
    request: &ModelRunPauseRequest,
) -> Result<(), RepositoryError> {
    if task.operation != "advisory"
        || request.instruction.trim().is_empty()
        || request.instruction.chars().count() > 1000
        || !crate::author_message::valid_id(&request.idempotency_key)
        || request.idempotency_key.len() > 160
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if target["conversationFocus"]["readOnly"] == true || target["readOnly"] == true {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    crate::tasks::validate_active_task(
        db,
        task,
        &task.project_id,
        &task.thread_id,
        task.input_revision,
    )?;
    crate::task_input::assert_task_inputs_current(db, task)?;
    crate::task_intents::assert_action_intent(db, &task.task_id, request.expected_intent_revision)?;
    let work = crate::model_work_plan::work_for(db, task)?;
    let owned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 source JOIN story_work_tasks_v1 run ON run.project_id=source.project_id AND run.thread_id=source.thread_id WHERE source.id=?1 AND source.project_id=?2 AND run.id='work-'||?3 AND run.kind='generation')",params![work,task.project_id.as_str(),request.run_id],|r|r.get(0)).map_err(storage_error)?;
    if !owned {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
pub(crate) fn finish(
    db: &Connection,
    task: &TaskRecord,
    request: &ModelRunPauseRequest,
    version: i64,
) -> Result<Value, RepositoryError> {
    let receipt = json!({"runId":request.run_id,"version":version,"status":"paused","sourceTaskId":task.task_id,"authority":"committedGenerationControl"});
    let encoded=json!({"runId":request.run_id,"expectedVersion":request.expected_version,"expectedIntentRevision":request.expected_intent_revision,"instruction":request.instruction,"idempotencyKey":request.idempotency_key}).to_string();
    db.execute(
        "INSERT INTO story_model_run_controls_v1 VALUES(?1,?2,?3,?4)",
        params![
            task.task_id,
            request.idempotency_key,
            encoded,
            receipt.to_string()
        ],
    )
    .map_err(storage_error)?;
    Ok(receipt)
}
impl SqliteStoryRepository {
    
    fn pause_generation_run_for_model_inner(
        &mut self,
        task: &TaskRecord,
        request: &ModelRunPauseRequest,
    ) -> Result<Value, RepositoryError> {
        let encoded=json!({"runId":request.run_id,"expectedVersion":request.expected_version,"expectedIntentRevision":request.expected_intent_revision,"instruction":request.instruction,"idempotencyKey":request.idempotency_key}).to_string();
        let replay:Option<(String,String)>=self.connection.query_row("SELECT request_json,receipt_json FROM story_model_run_controls_v1 WHERE task_id=?1 AND idempotency_key=?2",params![task.task_id,request.idempotency_key],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(storage_error)?;
        if let Some((prior, receipt)) = replay {
            if prior != encoded {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let value: Value =
                serde_json::from_str(&receipt).map_err(|_| RepositoryError::CorruptState)?;
            let run = self.generation_run(&task.project_id, &request.run_id)?;
            let state: Value =
                serde_json::from_str(&run.state_json).map_err(|_| RepositoryError::CorruptState)?;
            if run.version
                != value["version"]
                    .as_i64()
                    .ok_or(RepositoryError::CorruptState)?
                || state["status"] != "paused"
            {
                return Err(RepositoryError::AuthoringRunStale);
            }
            return Ok(value);
        }
        let prior = self.generation_run(&task.project_id, &request.run_id)?;
        if prior.version != request.expected_version {
            return Err(RepositoryError::AuthoringRunStale);
        }
        let mut state: Value =
            serde_json::from_str(&prior.state_json).map_err(|_| RepositoryError::CorruptState)?;
        if !matches!(state["status"].as_str(), Some("running" | "blocked")) {
            return Err(RepositoryError::AuthoringRunStale);
        }
        state["status"] = json!("paused");
        state["version"] = json!(prior.version + 1);
        self.update_generation_run_for_model_pause(task, request, &state.to_string())?;
        Ok(
            json!({"runId":request.run_id,"version":prior.version+1,"status":"paused","sourceTaskId":task.task_id,"authority":"committedGenerationControl"}),
        )
    }
}

/// Public generation model control capability; SQLite retains transaction ownership.
pub trait GenerationModelControlRepository {
fn pause_generation_run_for_model(
        &mut self,
        task: &TaskRecord,
        request: &ModelRunPauseRequest,
    ) -> Result<Value, RepositoryError>;
}

impl GenerationModelControlRepository for SqliteStoryRepository {
fn pause_generation_run_for_model(
        &mut self,
        task: &TaskRecord,
        request: &ModelRunPauseRequest,
    ) -> Result<Value, RepositoryError> {
        let result = self.pause_generation_run_for_model_inner(task, request);
        crate::recovery_operations::settle_prewrite_rejection(
            self,
            task,
            "pause_generation_run",
            &result,
        )?;
        result
    }
}

use crate::{AuthoringRunsRepository};
