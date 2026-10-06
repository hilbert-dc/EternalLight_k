use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    thread_id: String,
    expected_revision: i64,
    source_id: String,
    cursor: Option<String>,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(binding): Query<Binding>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project", source),
    };
    if binding.source_id.len() != 71
        || !binding.source_id.starts_with("source-")
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
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
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
    if let Err(source) = crate::recovery_binding::require_root(&headers, &task, "read_task_source")
    {
        return repository_error(source);
    }
    match repo.task_source_page(&task, &binding.source_id, binding.cursor.as_deref()) {
        Ok(value)
            if crate::context_model_item_budget::model_item_tokens(&value)
                <= crate::context_model_item_budget::MODEL_ITEM_TOKEN_LIMIT =>
        {
            Json(value).into_response()
        }
        Ok(_) => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "task_source_model_item_budget_exceeded",
            "Task source page exceeds the model-item budget",
        ),
        Err(source) => repository_error(source),
    }
}
