use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{CreateTaskRequest, RepositoryError};

pub(crate) fn validate_origin(target: &Value) -> Result<(), RepositoryError> {
    let action = target.get("requestedActionId").and_then(Value::as_str);
    let source = target.get("requestedByTaskId").and_then(Value::as_str);
    let run = target.get("runId").and_then(Value::as_str);
    let resume_step = target.get("resumeStepId").and_then(Value::as_str);
    let decision =
        target.get("workDecision").is_some() || target.get("workDecisionAction").is_some();
    if [
        "requestedActionId",
        "requestedByTaskId",
        "sourceTaskId",
        "resumeStepId",
        "runId",
    ]
    .iter()
    .any(|field| {
        target.get(field).is_some_and(|value| {
            value
                .as_str()
                .is_none_or(|id| id.is_empty() || id.len() > 160)
        })
    }) || action.is_some() != source.is_some()
        || target.get("sourceTaskId").is_some()
        || (run.is_some() && (!decision || resume_step.is_none()))
        || [
            action.is_some(),
            resume_step.is_some(),
            run.is_some() && !decision,
        ]
        .into_iter()
        .filter(|bound| *bound)
        .count()
            > 1
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn attach_attempt(
    db: &Connection,
    request: &CreateTaskRequest,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&request.target_json).map_err(|_| RepositoryError::CorruptState)?;
    validate_origin(&target)?;
    let action = target.get("requestedActionId").and_then(Value::as_str);
    let source = target.get("requestedByTaskId").and_then(Value::as_str);
    let run = target.get("runId").and_then(Value::as_str);
    let resume_step = target.get("resumeStepId").and_then(Value::as_str);
    let step_id = if let (Some(action), Some(source)) = (action, source) {
        crate::task_action_binding::validate_action_binding(
            db,
            &request.project_id,
            source,
            action,
            &request.operation,
            request.input_revision,
            &target,
        )?;
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_task_action_requests_v1 WHERE project_id = ?1 AND source_task_id = ?2 AND action_id = ?3 AND operation = ?4 AND input_revision = ?5 AND status = 'queued')",
            params![request.project_id.as_str(), source, action, request.operation, request.input_revision], |row| row.get(0),
        ).map_err(storage_error)?;
        if !valid {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        action.to_owned()
    } else if let Some(step) = resume_step {
        let original: String = db
            .query_row(
                "SELECT target_json FROM story_work_steps_v1 WHERE id = ?1",
                [step],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let mut resumed = target.clone();
        resumed
            .as_object_mut()
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .remove("resumeStepId");
        resumed.as_object_mut().unwrap().remove("messageDispatch");
        let mut original: Value =
            serde_json::from_str(&original).map_err(|_| RepositoryError::CorruptState)?;
        original.as_object_mut().ok_or(RepositoryError::TaskBindingInvalid)?.remove("messageDispatch");
        if resumed != original {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        step.to_owned()
    } else {
        let held:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_interludes_v1 i JOIN story_work_tasks_v1 w ON w.id=i.work_id WHERE w.project_id=?1 AND w.thread_id=?2 AND i.state='open')",params![request.project_id.as_str(),request.thread_id],|r|r.get(0)).map_err(storage_error)?;
        if held {return Err(RepositoryError::TaskNotWritable);}
        let parent = format!("work-{}", request.task_id);
        let kind = if request.operation == "advisory" {
            "conversation"
        } else {
            "direct"
        };
        let goal: Value = serde_json::from_str(&request.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let goal = goal
            .get("authorIntent")
            .and_then(Value::as_str)
            .unwrap_or(&request.operation);
        db.execute(
            "INSERT INTO story_work_tasks_v1(id, project_id, thread_id, goal, kind, status, input_revision) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
            params![parent, request.project_id.as_str(), request.thread_id, goal, kind, request.input_revision],
        ).map_err(storage_error)?;
        let step = format!("step-{}", request.task_id);
        let mut step_target=target.clone();
        step_target.as_object_mut().ok_or(RepositoryError::TaskBindingInvalid)?.remove("messageDispatch");
        db.execute(
            "INSERT INTO story_work_steps_v1(id, work_task_id, ordinal, operation, target_json, input_revision, status) VALUES (?1, ?2, 1, ?3, ?4, ?5, 'queued')",
            params![step, parent, request.operation, step_target.to_string(), request.input_revision],
        ).map_err(storage_error)?;
        step
    };
    let work: String = db.query_row("SELECT work_task_id FROM story_work_steps_v1 WHERE id=?1", [&step_id], |r| r.get(0)).map_err(storage_error)?;
    if target.get("interludeId").is_some() {
        if crate::work_pause_barrier::read(db,&work)?.is_some(){return Err(RepositoryError::TaskNotWritable);}
        crate::work_interludes::assert_schedule(db,&work,Some(&step_id))?;
    } else { crate::work_pause_barrier::assert_can_schedule(db, &work)?; }
    let ready: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_tasks_v1 t ON t.id = s.work_task_id WHERE s.id = ?1 AND t.project_id = ?2 AND t.status = 'running' AND s.operation = ?3 AND s.status = 'queued' AND s.input_revision = ?4 AND (s.depends_on_step_id IS NULL OR EXISTS(SELECT 1 FROM story_work_steps_v1 d WHERE d.id = s.depends_on_step_id AND d.status = 'completed')))",
        params![step_id, request.project_id.as_str(), request.operation, request.input_revision], |row| row.get(0),
    ).map_err(storage_error)?;
    if !ready {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if let Some(run_id) = run {
        crate::authoring_runs::claim_generation_run_for_task(
            db,
            &request.project_id,
            run_id,
            &request.task_id,
            &request.operation,
            &target,
        )?;
    }
    if action.is_some() || resume_step.is_some() {
        let work_id: String = db
            .query_row(
                "SELECT work_task_id FROM story_work_steps_v1 WHERE id=?1",
                [&step_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !decision_target(&target) && target.get("interludeId").is_none() {
            crate::work_tasks::assert_inputs_current(db, &work_id)?;
        }
        crate::execution_scope_storage::assert_attempt_capacity(db, &request.project_id, &work_id)?;
    }
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_work_attempts_v1 WHERE step_id = ?1",
            [&step_id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if let Some(diagnostic) = previous_validation(db, &step_id, request.project_id.as_str())? {
        db.execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=json_set(input_manifest_json,'$.previousToolValidation',json(?1)) WHERE task_id=?2 AND project_id=?3",
            params![diagnostic.to_string(), request.task_id, request.project_id.as_str()],
        ).map_err(storage_error)?;
    }
    db.execute("UPDATE story_work_steps_v1 SET status = 'running', current_attempt_id = ?1 WHERE id = ?2 AND status = 'queued'", params![request.task_id, step_id]).map_err(storage_error)?;
    db.execute("INSERT INTO story_work_attempts_v1(attempt_id, step_id, attempt_number) VALUES (?1, ?2, ?3)", params![request.task_id, step_id, count + 1]).map_err(storage_error)?;
    let (work_id, kind): (String,String) = db.query_row(
        "SELECT w.id,w.kind FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id WHERE s.id=?1",
        [&step_id], |row|Ok((row.get(0)?,row.get(1)?)),
    ).map_err(storage_error)?;
    if target.get("interludeId").is_some() {
        let deadline=crate::work_interludes::deadline(db,&request.project_id,&work_id,&request.task_id)?;
        crate::task_provider_budget::reserve(db,&request.project_id,&work_id,&request.task_id,deadline)?;
    } else if kind != "generation"
        && let Some(authorization) = crate::ordinary_work_budget::authorization(
            db,
            &request.project_id,
            &work_id,
            &request.task_id,
        )?
    {
        let now: i64 = db
            .query_row("SELECT unixepoch()", [], |row| row.get(0))
            .map_err(storage_error)?;
        if authorization
            .deadline_at
            .is_some_and(|deadline| deadline <= now)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_provider_budget::reserve(
            db,
            &request.project_id,
            &work_id,
            &request.task_id,
            authorization.deadline_at,
        )?;
    }
    db.execute("UPDATE story_work_tasks_v1 SET version = version + 1,thread_id=CASE WHEN thread_id='' THEN ?2 ELSE thread_id END WHERE id = (SELECT work_task_id FROM story_work_steps_v1 WHERE id = ?1)", params![step_id,request.thread_id]).map_err(storage_error)?;
    if action.is_some() || resume_step.is_some() {
        db.execute("UPDATE story_task_action_requests_v1 SET status = 'running', result_task_id = ?1 WHERE action_id = ?2 AND project_id=?3 AND status = 'queued'", params![request.task_id, step_id, request.project_id.as_str()]).map_err(storage_error)?;
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn previous_validation(db: &Connection, step: &str, project: &str) -> Result<Option<Value>, RepositoryError> {
    let error: Option<String> = db.query_row(
        "SELECT t.error FROM story_work_attempts_v1 a JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE a.step_id=?1 AND t.project_id=?2 AND t.status='failed' ORDER BY a.attempt_number DESC LIMIT 1",
        params![step,project], |row| row.get(0),
    ).optional().map_err(storage_error)?.flatten();
    Ok(crate::conversation_receipts::tool_diagnostic(error.as_deref()))
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn retry_feedback_is_bound_to_the_failed_step_and_project_and_sanitized() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_tasks_v2(task_id TEXT,project_id TEXT,status TEXT,error TEXT); CREATE TABLE story_work_attempts_v1(attempt_id TEXT,step_id TEXT,attempt_number INTEGER);").unwrap();
        let error = "invalid_tool_arguments; toolValidation={\"tool\":\"publish_knowledge_batch\",\"reason\":\"required\",\"schemaPath\":\"/required\",\"missingField\":\"value\"}";
        db.execute("INSERT INTO story_tasks_v2 VALUES('failed','project','failed',?1)",[error]).unwrap();
        db.execute_batch("INSERT INTO story_work_attempts_v1 VALUES('failed','step',1)").unwrap();
        assert_eq!(previous_validation(&db,"step","project").unwrap().unwrap()["missingField"],"value");
        assert!(previous_validation(&db,"step","other").unwrap().is_none());
        assert!(previous_validation(&db,"other","project").unwrap().is_none());
        db.execute("UPDATE story_tasks_v2 SET error='private unstructured failure'",[]).unwrap();
        assert!(previous_validation(&db,"step","project").unwrap().is_none());
    }
}

fn decision_target(target: &Value) -> bool {
    target.get("workDecision").is_some() || target.get("workDecisionAction").is_some()
}
