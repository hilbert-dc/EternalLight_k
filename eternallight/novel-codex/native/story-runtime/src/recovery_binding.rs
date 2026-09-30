use axum::http::HeaderMap;
use novel_codex_story_core::{
    RecoveryOperationBinding, RepositoryError, SqliteStoryRepository, TaskRecord,
};
use sha2::{Digest, Sha256};

/// Domain calls without a Host tool identity are not fabricated as MCP calls.
/// Once a tool identity is present, all metadata is mandatory and checked.
pub(crate) fn bind(
    repo: &mut SqliteStoryRepository,
    headers: &HeaderMap,
    task: Option<&TaskRecord>,
    tool: &str,
    key: Option<&str>,
) -> Result<(), RepositoryError> {
    let Some(_) = headers.get("x-novel-tool-name") else {
        return Ok(());
    };
    let task = task.ok_or(RepositoryError::TaskBindingInvalid)?;
    let read = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .ok_or(RepositoryError::TaskBindingInvalid)
    };
    require_root(headers, task, tool)?;
    repo.bind_recovery_operation(RecoveryOperationBinding {
        project_id: task.project_id.clone(),
        task_id: task.task_id.clone(),
        thread_id: task.thread_id.clone(),
        turn_id: read("x-novel-turn")?.into(),
        call_id: read("x-novel-tool-call")?.into(),
        tool_name: tool.into(),
        request_hash: read("x-novel-request-hash")?.into(),
        idempotency_ref: key.map(|key| {
            format!(
                "sha256:{:x}",
                Sha256::digest(serde_json::json!(key).to_string().as_bytes())
            )
        }),
        now: crate::server::unix_now(),
    })
}

/// Root-owned tools must also reject delegated lineage at the public Runtime boundary.
pub(crate) fn require_root(
    headers: &HeaderMap,
    task: &TaskRecord,
    tool: &str,
) -> Result<(), RepositoryError> {
    if headers.get("x-novel-tool-name").is_none() {
        return Ok(());
    }
    if headers
        .get("x-novel-tool-name")
        .and_then(|value| value.to_str().ok())
        != Some(tool)
        || headers
            .get("x-novel-root-thread")
            .and_then(|value| value.to_str().ok())
            != Some(task.thread_id.as_str())
        || headers
            .get("x-novel-caller-thread")
            .and_then(|value| value.to_str().ok())
            != Some(task.thread_id.as_str())
        || headers
            .get("x-novel-parent-thread")
            .is_some_and(|value| !value.is_empty())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}
