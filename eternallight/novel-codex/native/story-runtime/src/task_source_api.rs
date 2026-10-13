use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, RepositoryError, TaskRecord};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskSourceApiBinding", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    thread_id: String,
    expected_revision: i64,
    source_id: Option<String>,
    revision_id: Option<String>,
    cursor: Option<String>,
    view: Option<String>,
    chapter_number: Option<i64>,
    object_kind: Option<String>,
    query: Option<String>,
    quote: Option<String>,
    group_id: Option<String>,
    edge: Option<String>,
    characters: Option<usize>,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(binding): Query<Binding>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        if binding.edge.is_some()
            || binding.characters.is_some()
            || binding.quote.is_some()
            || binding.group_id.is_some()
            || binding.view.is_some()
            || binding.chapter_number.is_some()
            || binding.object_kind.is_some()
            || binding.revision_id.is_some()
            || binding.query.is_some()
        {
            return bad_request(
                "invalid_task_source_view",
                "Raw source reads do not accept a view",
            );
        }
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project", source),
        };
        if binding
            .source_id
            .as_deref()
            .is_some_and(|id| id.len() != 71 || !id.starts_with("source-"))
            || (binding.source_id.is_none()
                && !matches!(binding.view.as_deref(), Some("workspace" | "passages")))
            || binding
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.len() > 100)
        {
            return bad_request(
                "invalid_task_source",
                "Source ID or cursor exceeds its bound",
            );
        }

        let task = match candidate_task(
            &repo,
            actor,
            &headers,
            &project,
            Some(&id),
            Some(&binding.thread_id),
            Some(binding.expected_revision),
        ) {
            Ok(Some(task)) => task,
            Ok(None) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "task_source_task_required",
                    "An active bound task is required",
                );
            }
            Err(response) => return *response,
        };
        let reader = match source_reader(&headers, &task, "read_source_content") {
            Ok(reader) => reader,
            Err(source) => return repository_error(source),
        };
        match repo.task_source_page_for_reader(
            &task,
            binding.source_id.as_deref().unwrap_or_default(),
            binding.cursor.as_deref(),
            &reader,
        ) {
            Ok(value)
                if crate::context_model_item_budget::model_item_bytes(&value)
                    <= crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT =>
            {
                Json(value).into_response()
            }
            Ok(_) => error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "task_source_model_item_budget_exceeded",
                "Task source page exceeds the model-item budget",
            ),
            Err(source) => source_error(source, &task),
        }
    })
    .await
}

pub(crate) async fn read_content(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(binding): Query<Binding>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
    if (binding.view.as_deref() == Some("passages") &&
        (!matches!(binding.edge.as_deref(), Some("start" | "end")) || !binding.characters.is_some_and(|n| (1..=512).contains(&n))
            || binding.object_kind.as_deref().is_some_and(|k| k != "draft")))
        || (binding.view.as_deref() != Some("passages") && (binding.edge.is_some() || binding.characters.is_some()))
        || (binding.view.as_deref() == Some("locate") && binding.quote.is_none())
        || (binding.view.as_deref() != Some("locate") && binding.quote.is_some())
        || (!matches!(binding.view.as_deref(), Some("locate" | "scene")) && binding.group_id.is_some())
        || (binding.view.as_deref() == Some("scene") && binding.group_id.is_none())
        || binding
        .view
        .as_deref()
        .is_some_and(|view| !matches!(view, "index" | "workspace" | "locate" | "passages" | "scene"))
        || (!matches!(binding.view.as_deref(), Some("workspace" | "passages"))
            && (binding.chapter_number.is_some()
                || binding.object_kind.is_some()
                || binding.revision_id.is_some()
                || binding.query.is_some()))
    {
        return bad_request(
            "invalid_task_source_view",
            "Use workspace for admitted sources, passages with edge=start/end and characters=1..512 for partial frozen prose, index for records, or omit view for content",
        );
    }
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project", source),
    };
    if binding.source_id.as_deref().is_some_and(|id| id.len() != 71 || !id.starts_with("source-"))
        || (binding.source_id.is_none() && !matches!(binding.view.as_deref(), Some("workspace" | "passages")))
        || binding
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 100)
    {
        return bad_request(
            "invalid_task_source",
            "Source ID or cursor exceeds its bound",
        );
    }

    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&id),
        Some(&binding.thread_id),
        Some(binding.expected_revision),
    ) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return error(
                StatusCode::FORBIDDEN,
                "task_source_task_required",
                "An active bound task is required",
            );
        }
        Err(response) => return *response,
    };
    let reader = match source_reader(&headers, &task, "read_source_content") {
        Ok(reader) => reader,
        Err(source) => return repository_error(source),
    };
    let result = if binding.view.as_deref() == Some("scene") {
        repo.task_source_scene(&task, binding.source_id.as_deref().unwrap_or_default(), binding.group_id.as_deref().unwrap(), binding.cursor.as_deref(), &reader)
    } else if binding.view.as_deref() == Some("passages") {
        repo.task_workspace_passages_for_reader(&task, binding.source_id.as_deref(), binding.cursor.as_deref(),
            binding.chapter_number, binding.query.as_deref(), binding.revision_id.as_deref(),
            binding.edge.as_deref().unwrap(), binding.characters.unwrap(), &reader)
    } else if binding.view.as_deref() == Some("locate") {
        repo.task_source_locate_for_reader(&task, binding.source_id.as_deref().unwrap_or_default(), binding.quote.as_deref().unwrap(), binding.group_id.as_deref(), binding.cursor.as_deref(), &reader)
    } else if binding.view.as_deref() == Some("workspace") {
        repo.task_workspace_page(&task, binding.source_id.as_deref(), binding.cursor.as_deref(), binding.chapter_number, binding.object_kind.as_deref(), binding.query.as_deref(), binding.revision_id.as_deref())
    } else if binding.view.as_deref() == Some("index") {
        repo.task_source_index(&task, binding.source_id.as_deref().unwrap_or_default(), binding.cursor.as_deref())
    } else {
        repo.task_source_content_for_reader(
            &task,
            binding.source_id.as_deref().unwrap_or_default(),
            binding.cursor.as_deref(),
            &reader,
        )
    };
    match result {
        Ok(value)
            if crate::context_model_item_budget::model_item_bytes(&value)
                <= crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT =>
        {
            Json(value).into_response()
        }
        Ok(_) => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "task_source_model_item_budget_exceeded",
            "Task source page exceeds the model-item budget",
        ),
        Err(source) => source_error(source, &task),
    }

    }).await
}

// Only the trusted MCP transport supplies lineage. A child keeps the exact root
// capability/visible source, while its returned pages never certify root reading.
fn source_reader(
    headers: &HeaderMap,
    task: &TaskRecord,
    tool: &str,
) -> Result<String, RepositoryError> {
    if headers.get("x-novel-tool-name").is_none() {
        return Ok(task.thread_id.clone());
    }
    let get = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty() && v.len() <= 160 && !v.chars().any(char::is_control))
    };
    if get("x-novel-tool-name") != Some(tool)
        || get("x-novel-root-thread") != Some(task.thread_id.as_str())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let caller = get("x-novel-caller-thread").ok_or(RepositoryError::TaskBindingInvalid)?;
    let parent = get("x-novel-parent-thread");
    if (caller == task.thread_id && parent.is_some())
        || (caller != task.thread_id && parent != Some(task.thread_id.as_str()))
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(caller.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task() -> TaskRecord {
        TaskRecord {
            task_id: "task".into(),
            project_id: ProjectId::parse("project").unwrap(),
            thread_id: "root".into(),
            operation: "advisory".into(),
            task_kind: "advisory".into(),
            purpose: "planner".into(),
            required_output_kind: None,
            target_json: "{}".into(),
            input_manifest_json: "{}".into(),
            input_revision: 0,
            model: None,
            turn_id: None,
            status: "running".into(),
            stage: "running".into(),
            delivery_status: "pending".into(),
            output_refs_json: "[]".into(),
            result_summary: "".into(),
            error: None,
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
            capability_revoked_at: None,
        }
    }
    #[test]
    fn source_reader_requires_exact_root_or_trusted_direct_child_lineage() {
        let task = task();
        let mut headers = HeaderMap::new();
        assert_eq!(
            source_reader(&headers, &task, "read_source_content").unwrap(),
            "root"
        );
        headers.insert("x-novel-tool-name", "read_source_content".parse().unwrap());
        headers.insert("x-novel-root-thread", "root".parse().unwrap());
        headers.insert("x-novel-caller-thread", "root".parse().unwrap());
        assert_eq!(
            source_reader(&headers, &task, "read_source_content").unwrap(),
            "root"
        );
        headers.insert("x-novel-caller-thread", "child".parse().unwrap());
        assert!(source_reader(&headers, &task, "read_source_content").is_err());
        headers.insert("x-novel-parent-thread", "root".parse().unwrap());
        assert_eq!(
            source_reader(&headers, &task, "read_source_content").unwrap(),
            "child"
        );
        assert!(source_reader(&headers, &task, "read_context").is_err());
        headers.insert("x-novel-parent-thread", "other-root".parse().unwrap());
        assert!(source_reader(&headers, &task, "read_source_content").is_err());
        headers.insert("x-novel-parent-thread", "root".parse().unwrap());
        headers.insert("x-novel-root-thread", "foreign".parse().unwrap());
        assert!(source_reader(&headers, &task, "read_source_content").is_err());
    }
}

fn source_error(source: RepositoryError, task: &TaskRecord) -> Response {
    let (status, code, message) = match source {
        RepositoryError::TaskSourceCatalogRequired => (
            StatusCode::CONFLICT,
            "task_source_catalog_required",
            "本轮尚无来源目录。请先调用 read_context 获取本轮来源目录，再使用其中的 sourceId 读取正文或资料。",
        ),
        RepositoryError::TaskSourceNotInCatalog => (
            StatusCode::CONFLICT,
            "task_source_not_in_catalog",
            "该 sourceId 不在本轮来源目录中。请先调用 read_context 获取本轮来源目录；不要复用其他任务的 sourceId 或 cursor。",
        ),
        RepositoryError::TaskSourceCursorInvalid => (
            StatusCode::BAD_REQUEST,
            "task_source_cursor_invalid",
            "这个 cursor 与本轮来源或读取方式不匹配。请先调用 read_context 获取本轮来源目录，再从首段或目录返回的 read 请求 重新读取。",
        ),
        _ => return repository_error(source),
    };
    (
        status,
        Json(json!({"error":{
            "code":code,"message":message,
            "details":{"taskId":task.task_id,"retryable":true,"sourceIsolation":"currentTaskOnly"},
            "recovery":{"tool":"read_context","arguments":{
                "expectedRevision":task.input_revision
            },"discardPreviousSourceIdAndCursor":true}
        }})),
    )
        .into_response()
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/task-source-content",
            axum::routing::get(crate::task_source_api::read_content),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/task-source",
            axum::routing::get(crate::task_source_api::read),
        )
}

use novel_codex_story_core::{TaskSourcesRepository, TaskWorkspaceRepository};
