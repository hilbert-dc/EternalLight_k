//! Bounded domain evidence, without child prose or caller-supplied failure details.
use crate::{RepositoryError, authoring_runs::storage_error};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

pub(crate) fn settled_failure_without_writes(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t JOIN story_task_provider_budgets_v1 b ON b.task_id=t.task_id AND b.settled=1 WHERE t.task_id=?1 AND t.status='failed' AND t.output_refs_json='[]' AND NOT EXISTS(SELECT 1 FROM story_task_outputs_v2 WHERE task_id=t.task_id) AND NOT EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=t.task_id AND state!='rejected'))",[task],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    if !valid {
        return Ok(false);
    }
    settled_failure_usage(db, task)
}

/// A saved chapter may be ready for review even though its provider turn failed.
/// This admits reconciliation of the saved artifact, never replay of the writer.
pub(crate) fn recoverable_saved_chapter(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    let record = crate::tasks::load_task(db, task)?;
    if record.status != "failed" || record.operation != "generateChapter"
        || record.required_output_kind.as_deref() != Some("draft")
        || failure_code(record.error.as_deref()).is_none() {
        return Ok(false);
    }
    let work: Option<String> = db.query_row(
        "SELECT s.work_task_id FROM story_work_steps_v1 s JOIN story_work_tasks_v1 w ON w.id=s.work_task_id JOIN story_task_provider_budgets_v1 b ON b.task_id=s.current_attempt_id AND b.settled=1 WHERE s.current_attempt_id=?1 AND s.status='blocked' AND w.kind='generation' AND w.status IN ('running','blocked')",
        [task], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let Some(work) = work else { return Ok(false); };
    if crate::work_modeling_recovery::pending_results(db, &record.project_id, &work)?
        || !settled_failure_usage(db, task)?
        || !crate::tasks::recoverable_output(db, &record)? {
        return Ok(false);
    }
    let content: String = db.query_row(
        "SELECT r.content_json FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id AND r.project_id=?2 WHERE o.task_id=?1 ORDER BY o.ordinal DESC LIMIT 1",
        rusqlite::params![task, record.project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    crate::planned_scene_mapping::validate(db, &record, &content, Some("chapterComplete"))?;
    Ok(true)
}

fn settled_failure_usage(db: &Connection, task: &str) -> Result<bool, RepositoryError> {
    let work:String=db.query_row("SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1",[task],|r|r.get(/*idx*/0)).map_err(storage_error)?;
    let record = crate::tasks::load_task(db, task)?;
    let output_missing = failure_code(record.error.as_deref()) == Some("TaskOutputMissing");
    let budget = crate::work_budget_storage::load(db, &record.project_id, &work)?;
    if budget.continuation_block_reason().is_some() { return Ok(false); }
    Ok(
        budget.charges
            .iter()
            .any(|charge| {
                charge.task_id == task
                    // Provider completion records billed execution, not domain delivery.
                    && (charge.status == "failed"
                        || (charge.status == "completed" && output_missing))
                    && (charge.unknown_tokens == 0 || budget.max_tokens.is_none())
                    && !charge.overrun
            }),
    )
}
pub(crate) fn failure_code(error: Option<&str>) -> Option<&'static str> {
    let error = error?;
    if let Ok(value) = serde_json::from_str::<Value>(error) {
        if value["error"]["code"] == "task_provider_usage_unknown"
            && value["error"]["type"] == "budget_unknown"
            && value["error"]["retryable"] == false
        {
            // The same Engine request cannot retry. Native may admit a new
            // model decision only after checking settled costs and zero writes.
            return Some("task_provider_usage_unknown");
        }
        return None;
    }
    let code = error.split([';', ':', ' ']).next()?;
    match code {
        "unauthorized_tool_call" => Some("unauthorized_tool_call"),
        "invalid_tool_arguments" => Some("invalid_tool_arguments"),
        "tool_argument_correction_limit" => Some("tool_argument_correction_limit"),
        "provider_output_limit" => Some("provider_output_limit"),
        "TaskOutputMissing" => Some("TaskOutputMissing"),
        _ => None,
    }
}
pub(crate) fn tool_diagnostic(error: Option<&str>) -> Option<Value> {
    let error = error?;
    let message = error.strip_prefix("invalid_tool_arguments; toolValidation=")
        .or_else(|| error.strip_prefix("unauthorized_tool_call; toolValidation="))?;
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
            "required" | "type" | "enum" | "additionalProperties" | "constraint" | "not_in_current_request"
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
    if fields.get("requiredFields").is_some_and(|value| {
        value.as_array().is_none_or(|items| {
            items.is_empty()
                || items.len() > 8
                || items
                    .iter()
                    .any(|item| item.as_str().is_none_or(|field| !identifier(field, 32)))
        })
    }) {
        return None;
    }
    let mut value = value;
    if value["reason"] == "not_in_current_request" {
        value["instruction"] = serde_json::json!("该工具未在失败请求中开放，该调用没有执行。这不证明作者需要追加授权；请基于本轮真实可用工具和发现入口重新选择能力，不复用历史工具名或wire别名。权限仍由当前任务合同决定。");
    }
    Some(value)
}

#[cfg(test)]
mod recovery_diagnostic_tests {
    use super::*;
    #[test]
    fn preserves_request_visibility_diagnostic_without_accepting_arbitrary_prose() {
        let error = "unauthorized_tool_call; toolValidation={\"tool\":\"f__read_report\",\"reason\":\"not_in_current_request\",\"schemaPath\":\"/tools\"}";
        assert_eq!(failure_code(Some(error)), Some("unauthorized_tool_call"));
        assert_eq!(tool_diagnostic(Some(error)).unwrap()["tool"], "f__read_report");
        assert!(tool_diagnostic(Some("provider says unauthorized_tool_call")).is_none());
        assert!(failure_code(Some("unexpected status 403 Forbidden")).is_none());
    }
}
