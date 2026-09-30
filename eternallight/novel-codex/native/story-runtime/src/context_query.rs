use novel_codex_story_core::{RepositoryError, SqliteStoryRepository, StoryQueryPlan, TaskRecord};
use serde_json::Value;

pub(crate) struct ContextQueryBinding<'a> {
    pub snapshot_id: &'a str,
    pub byte_limit: usize,
    pub now: i64,
}

/// Re-execute the requested plan instead of trusting a model-supplied evidence report.
pub(crate) fn retrieve(
    repository: &SqliteStoryRepository,
    task: Option<&TaskRecord>,
    encoded_plan: Option<&str>,
    binding: ContextQueryBinding<'_>,
) -> Result<Option<Value>, RepositoryError> {
    let Some(encoded_plan) = encoded_plan else {
        return Ok(None);
    };
    let task = task.ok_or(RepositoryError::TaskOutputForbidden)?;
    if encoded_plan.len() > 9_000 {
        return Err(RepositoryError::InvalidContent(
            "query plan exceeds context request limit".into(),
        ));
    }
    let plan: StoryQueryPlan = serde_json::from_str(encoded_plan)
        .map_err(|_| RepositoryError::InvalidContent("invalid context query plan".into()))?;
    if plan.byte_budget > binding.byte_limit {
        return Err(RepositoryError::InvalidContent(
            "query budget exceeds context budget".into(),
        ));
    }
    let report = repository.query_task_plan(task, &plan, binding.now)?;
    let source_heads = repository.task_query_source_watermarks(task)?;
    if report["snapshotId"] != binding.snapshot_id || report["sourceWatermarks"] != source_heads {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(Some(report))
}
