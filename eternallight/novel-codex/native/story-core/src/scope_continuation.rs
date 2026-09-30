use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord, WorkTask};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Continuation {
    permit_id: String,
    source_task_id: String,
    trigger_action_id: String,
}

impl SqliteStoryRepository {
    /// Schedules a bounded read-only continuation from settled, actual outputs.
    /// The Author transport invokes this; candidate tools cannot mint work or budgets.
    pub fn continue_scoped_work(
        &mut self,
        project: &ProjectId,
        work: &str,
    ) -> Result<WorkTask, RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let current = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        let id: Option<String> = tx.query_row("SELECT permit_id FROM story_execution_scopes_v1 WHERE work_task_id=?1 AND status='active'", [work], |row| row.get(0)).optional().map_err(storage_error)?;
        if current.status != "completed" || id.is_none() {
            return Ok(current);
        }
        let id = id.ok_or(RepositoryError::CorruptState)?;
        let scope = crate::execution_scope_storage::by_id(&tx, project, &id)?;
        crate::execution_scope_storage::assert_live(&tx, &scope)?;
        let rounds: i64 = tx.query_row("SELECT COUNT(*) FROM story_work_steps_v1 WHERE work_task_id=?1 AND json_extract(target_json,'$.scopeContinuation.permitId')=?2", params![work,id], |row| row.get(0)).map_err(storage_error)?;
        if rounds >= i64::from(scope.scope.request.max_advisory_rounds) {
            return Ok(current);
        }
        crate::execution_scope_storage::assert_attempt_capacity(&tx, project, work)?;
        crate::work_tasks::assert_inputs_current(&tx, work)?;
        let unsettled: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0)", [work], |row| row.get(0)).map_err(storage_error)?;
        if unsettled || crate::work_budget_storage::load(&tx, project, work)?.exhausted() {
            return Err(RepositoryError::TaskNotWritable);
        }
        let trigger: Option<String> = tx.query_row("SELECT a.action_id FROM story_task_action_requests_v1 a JOIN story_action_permits_v1 p ON p.action_id=a.action_id JOIN story_tasks_v2 t ON t.task_id=a.result_task_id
            JOIN story_work_attempts_v1 attempt ON attempt.attempt_id=t.task_id AND attempt.step_id=a.action_id
            WHERE p.work_task_id=?1 AND p.scope_permit_id=?2 AND a.status='completed' AND t.status='completed' AND json_array_length(t.output_refs_json)>0
            AND EXISTS(SELECT 1 FROM story_task_outputs_v2 o WHERE o.task_id=t.task_id)
            AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 s WHERE s.work_task_id=?1 AND json_extract(s.target_json,'$.scopeContinuation.triggerActionId')=a.action_id)
            ORDER BY a.rowid DESC LIMIT 1", params![work,id], |row| row.get(0)).optional().map_err(storage_error)?;
        let Some(trigger) = trigger else {
            return Ok(current);
        };
        let (prior, revision): (String,i64) = tx.query_row("SELECT s.id,t.input_revision FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.attempt_id=s.current_attempt_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND s.status='completed' ORDER BY s.ordinal DESC LIMIT 1", [work], |row| Ok((row.get(0)?,row.get(1)?))).map_err(storage_error)?;
        let step = format!("followup-{trigger}");
        let target = json!({"type":"project","scopeContinuation":{"permitId":id,"sourceTaskId":scope.scope.source_task_id,"triggerActionId":trigger}});
        tx.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status)
            VALUES(?1,?2,(SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?2),'advisory',?3,?4,?5,'queued')", params![step,work,target.to_string(),revision,prior]).map_err(storage_error)?;
        validate_step(&tx, project, &step, &target)?;
        crate::work_tasks::refresh_work_task(&tx, work)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [work],
        )
        .map_err(storage_error)?;
        let result = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

pub(crate) fn validate_step(
    db: &Connection,
    project: &ProjectId,
    step: &str,
    target: &Value,
) -> Result<(String, i64), RepositoryError> {
    let binding: Continuation = serde_json::from_value(target["scopeContinuation"].clone())
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let scope = crate::execution_scope_storage::by_id(db, project, &binding.permit_id)?;
    let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
        JOIN story_work_steps_v1 predecessor ON predecessor.id=s.depends_on_step_id AND predecessor.work_task_id=s.work_task_id AND predecessor.ordinal<s.ordinal
        JOIN story_task_action_requests_v1 action ON action.action_id=?4 JOIN story_action_permits_v1 p ON p.action_id=action.action_id AND p.work_task_id=s.work_task_id AND p.scope_permit_id=?5
        JOIN story_work_steps_v1 trigger ON trigger.id=action.action_id AND trigger.work_task_id=s.work_task_id AND trigger.ordinal<s.ordinal
        JOIN story_tasks_v2 output ON output.task_id=action.result_task_id
        JOIN story_work_attempts_v1 attempt ON attempt.attempt_id=output.task_id AND attempt.step_id=trigger.id
        WHERE s.id=?1 AND s.work_task_id=?2 AND w.project_id=?3 AND s.operation='advisory' AND s.target_json=?6
        AND action.status='completed' AND output.status='completed' AND trigger.current_attempt_id=output.task_id AND json_array_length(output.output_refs_json)>0
        AND EXISTS(SELECT 1 FROM story_task_outputs_v2 o WHERE o.task_id=output.task_id))",
        params![step,scope.scope.work_task_id,project.as_str(),binding.trigger_action_id,binding.permit_id,target.to_string()], |row| row.get(0)).map_err(storage_error)?;
    if !valid
        || binding.source_task_id != scope.scope.source_task_id
        || scope.scope.request.max_advisory_rounds == 0
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok((
        binding.source_task_id,
        scope.scope.request.expected_intent_revision,
    ))
}

pub(crate) fn validate_source(
    db: &Connection,
    task: &TaskRecord,
    permit: &str,
) -> Result<(), RepositoryError> {
    let mut target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let step = target
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if target["scopeContinuation"]["permitId"] != permit {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let belongs: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 WHERE step_id=?1 AND attempt_id=?2)", params![step,task.task_id], |row| row.get(0)).map_err(storage_error)?;
    if !belongs {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (root, root_revision) = validate_step(db, &task.project_id, &step, &target)?;
    let prior: Option<String> = db.query_row(
        "SELECT prior.attempt_id FROM story_work_attempts_v1 current
         JOIN story_work_attempts_v1 prior ON prior.step_id=current.step_id AND prior.sequence<current.sequence
         WHERE current.attempt_id=?1 ORDER BY prior.sequence DESC LIMIT 1",
        [&task.task_id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let (source, revision) = match prior {
        Some(source) => {
            let revision: i64 = db.query_row(
                "SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1",
                [&source], |row| row.get(0),
            ).map_err(storage_error)?;
            (source, revision)
        }
        None => (root, root_revision),
    };
    let manifest: Value = serde_json::from_str(&task.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    if manifest["authorIntentBinding"]
        != json!({"sourceTaskId":source,"intentRevision":revision,"actionId":null})
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn assert_current(
    db: &Connection,
    task: &TaskRecord,
    target: &Value,
) -> Result<(), RepositoryError> {
    if let Some(binding) = target.get("scopeContinuation") {
        let id = binding["permitId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        validate_source(db, task, id)?;
        crate::scope_intent_adjustment::assert_analysis_authority(
            db,
            task,
            &crate::execution_scope_storage::by_id(db, &task.project_id, id)?,
        )?;
    }
    Ok(())
}
