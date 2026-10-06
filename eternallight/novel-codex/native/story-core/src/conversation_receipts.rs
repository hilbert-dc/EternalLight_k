//! Bounded domain evidence, without child prose or caller-supplied failure details.
use crate::{RepositoryError, authoring_runs::storage_error};
use rusqlite::Connection;
use serde_json::Value;

pub(crate) fn known_zero_failure(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t JOIN story_task_provider_budgets_v1 b ON b.task_id=t.task_id AND b.settled=1 WHERE t.task_id=?1 AND t.status='failed' AND t.output_refs_json='[]' AND NOT EXISTS(SELECT 1 FROM story_task_outputs_v2 WHERE task_id=t.task_id) AND NOT EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=t.task_id AND state!='rejected'))",[task],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    if !valid {
        return Ok(false);
    }
    let work:String=db.query_row("SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1",[task],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    let record = crate::tasks::load_task(db, task)?;
    Ok(
        crate::work_budget_storage::load(db, &record.project_id, &work)?
            .charges
            .iter()
            .any(|charge| {
                charge.task_id == task
                    && charge.status == "failed"
                    && charge.unknown_tokens == 0
                    && !charge.overrun
            }),
    )
}
pub(crate) fn failure_code(error: Option<&str>) -> Option<&'static str> {
    let code = error?.split([';', ':', ' ']).next()?;
    match code {
        "invalid_tool_arguments" => Some("invalid_tool_arguments"),
        "TaskOutputMissing" => Some("TaskOutputMissing"),
        _ => None,
    }
}
pub(crate) fn tool_diagnostic(error: Option<&str>) -> Option<Value> {
    let message = error?.strip_prefix("invalid_tool_arguments; toolValidation=")?;
    if message.len() > 400 {
        return None;
    }
    let value: Value = serde_json::from_str(message).ok()?;
    let fields = value.as_object()?;
    if fields.keys().any(|key| {
        !matches!(
            key.as_str(),
            "tool" | "reason" | "schemaPath" | "missingField" | "requiredFields"
        )
    }) {
        return None;
    }
    let identifier = |text: &str, max: usize| {
        !text.is_empty()
            && text.len() <= max
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    };
    if !identifier(value["tool"].as_str()?, /*max*/ 80)
        || !matches!(
            value["reason"].as_str()?,
            "required" | "type" | "enum" | "additionalProperties" | "constraint"
        )
    {
        return None;
    }
    let path = value["schemaPath"].as_str()?;
    if path.len() > 128
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-/$~".contains(&byte))
    {
        return None;
    }
    if fields.get("missingField").is_some_and(|field| {
        field
            .as_str()
            .is_none_or(|field| !identifier(field, /*max*/ 48))
    }) {
        return None;
    }
    if fields.get("requiredFields").is_some_and(|value| value.as_array().is_none_or(|items|
        items.is_empty() || items.len() > 8 || items.iter().any(|item|
            item.as_str().is_none_or(|field| !identifier(field, 32))))) {
        return None;
    }
    Some(value)
}
