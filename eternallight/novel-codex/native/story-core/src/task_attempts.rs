use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{CreateTaskRequest, RepositoryError};

pub(crate) fn validate_origin(target: &Value) -> Result<(), RepositoryError> {
    let action = target.get("requestedActionId").and_then(Value::as_str);
    let source = target.get("requestedByTaskId").and_then(Value::as_str);
    let chapter_source = target.get("sourceTaskId").and_then(Value::as_str);
    let run = target.get("runId").and_then(Value::as_str);
    let resume_step = target.get("resumeStepId").and_then(Value::as_str);
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
        || [
            action.is_some(),
            chapter_source.is_some(),
            resume_step.is_some(),
            run.is_some(),
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
    let chapter_source = target.get("sourceTaskId").and_then(Value::as_str);
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
    } else if let Some(source) = chapter_source {
        let parent: String = db.query_row(
            "SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id = a.step_id WHERE a.attempt_id = ?1",
            [source], |row| row.get(0),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let step = format!("knowledge-{source}");
        let source_task: (String, String) = db
            .query_row(
                "SELECT status, operation FROM story_tasks_v2 WHERE task_id = ?1",
                [source],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage_error)?;
        if source_task.0 != "completed"
            || source_task.1 != "generateChapter"
            || request.operation != "modelKnowledge"
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        db.execute(
            "INSERT OR IGNORE INTO story_work_steps_v1(id, work_task_id, ordinal, operation, target_json, input_revision, depends_on_step_id, status) VALUES (?1, ?2, (SELECT COALESCE(MAX(ordinal), 0) + 1 FROM story_work_steps_v1 WHERE work_task_id = ?2), 'modelKnowledge', ?3, ?4, (SELECT step_id FROM story_work_attempts_v1 WHERE attempt_id = ?5), 'queued')",
            params![step, parent, request.target_json, request.input_revision, source],
        ).map_err(storage_error)?;
        step
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
        let original: Value =
            serde_json::from_str(&original).map_err(|_| RepositoryError::CorruptState)?;
        if resumed != original {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        step.to_owned()
    } else if let Some(run) = run {
        let parent = format!("work-{run}");
        target
            .get("stepToken")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let plan_scope = (request.operation == "generatePlan")
            .then(|| target.get("planFromChapter").and_then(Value::as_i64))
            .flatten()
            .map(|chapter| format!("plan-chapter-{chapter}"));
        let scope = plan_scope.as_deref().unwrap_or_else(|| {
            target
                .get("chapterId")
                .and_then(Value::as_str)
                .unwrap_or("project")
        });
        let generation = target
            .get("intentGeneration")
            .and_then(Value::as_i64)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let step = format!("step-{run}-{}-{scope}-{generation}", request.operation);
        if db.query_row("SELECT 1 FROM story_work_tasks_v1 WHERE id = ?1 AND project_id = ?2 AND kind = 'generation'", params![parent, request.project_id.as_str()], |_| Ok(())).optional().map_err(storage_error)?.is_none() { return Err(RepositoryError::TaskBindingInvalid); }
        db.execute("INSERT OR IGNORE INTO story_work_steps_v1(id, work_task_id, ordinal, operation, target_json, input_revision, status) VALUES (?1, ?2, (SELECT COALESCE(MAX(ordinal), 0) + 1 FROM story_work_steps_v1 WHERE work_task_id = ?2), ?3, ?4, ?5, 'queued')", params![step, parent, request.operation, request.target_json, request.input_revision]).map_err(storage_error)?;
        db.execute("UPDATE story_work_steps_v1 SET target_json = ?1, input_revision = ?2 WHERE id = ?3 AND status = 'queued'", params![request.target_json, request.input_revision, step]).map_err(storage_error)?;
        step
    } else {
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
        db.execute(
            "INSERT INTO story_work_steps_v1(id, work_task_id, ordinal, operation, target_json, input_revision, status) VALUES (?1, ?2, 1, ?3, ?4, ?5, 'queued')",
            params![step, parent, request.operation, request.target_json, request.input_revision],
        ).map_err(storage_error)?;
        step
    };
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
    if action.is_some() || chapter_source.is_some() || resume_step.is_some() {
        let work_id: String = db
            .query_row(
                "SELECT work_task_id FROM story_work_steps_v1 WHERE id=?1",
                [&step_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        crate::work_tasks::assert_inputs_current(db, &work_id)?;
        crate::execution_scope_storage::assert_attempt_capacity(db, &request.project_id, &work_id)?;
    }
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_work_attempts_v1 WHERE step_id = ?1",
            [&step_id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    crate::modeling_dispatch::validate_claim(db, &request.project_id, &step_id, count, &target)?;
    db.execute("UPDATE story_work_steps_v1 SET status = 'running', current_attempt_id = ?1 WHERE id = ?2 AND status = 'queued'", params![request.task_id, step_id]).map_err(storage_error)?;
    db.execute("INSERT INTO story_work_attempts_v1(attempt_id, step_id, attempt_number) VALUES (?1, ?2, ?3)", params![request.task_id, step_id, count + 1]).map_err(storage_error)?;
    crate::scope_continuation::assert_current(
        db,
        &crate::tasks::load_task(db, &request.task_id)?,
        &target,
    )?;
    let (work_id, kind): (String,String) = db.query_row(
        "SELECT w.id,w.kind FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id WHERE s.id=?1",
        [&step_id], |row|Ok((row.get(0)?,row.get(1)?)),
    ).map_err(storage_error)?;
    if kind != "generation"
        && let Some(deadline) = crate::ordinary_work_budget::deadline(
            db,
            &request.project_id,
            &work_id,
            &request.task_id,
        )?
    {
        let now: i64 = db
            .query_row("SELECT unixepoch()", [], |row| row.get(0))
            .map_err(storage_error)?;
        if deadline <= now {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_provider_budget::reserve(
            db,
            &request.project_id,
            &work_id,
            &request.task_id,
            deadline,
        )?;
    }
    db.execute("UPDATE story_work_tasks_v1 SET version = version + 1,thread_id=CASE WHEN thread_id='' THEN ?2 ELSE thread_id END WHERE id = (SELECT work_task_id FROM story_work_steps_v1 WHERE id = ?1)", params![step_id,request.thread_id]).map_err(storage_error)?;
    if let Some(action) = action {
        db.execute("UPDATE story_task_action_requests_v1 SET status = 'running', result_task_id = ?1 WHERE action_id = ?2 AND status = 'queued'", params![request.task_id, action]).map_err(storage_error)?;
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
