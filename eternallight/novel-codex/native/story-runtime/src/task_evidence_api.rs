use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;

use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    thread_id: String,
    expected_revision: i64,
    source_object_id: String,
    source_revision_id: String,
    quote: String,
}

pub(crate) async fn resolve(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    let Ok(project) = ProjectId::parse(project) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_project",
            "Invalid project",
        );
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(source) => return repository_error(source),
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project,
        Some(&task),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return error(
                StatusCode::FORBIDDEN,
                "task_evidence_only",
                "A task capability is required",
            );
        }
        Err(response) => return *response,
    };
    if headers.get("x-novel-tool-name").is_none() {
        return error(
            StatusCode::FORBIDDEN,
            "task_evidence_only",
            "Root tool identity is required",
        );
    }
    if let Err(source) =
        crate::recovery_binding::require_root(&headers, &task, "resolve_story_evidence")
    {
        return repository_error(source);
    }
    match repository.resolve_task_evidence(
        &task,
        &request.source_object_id,
        &request.source_revision_id,
        &request.quote,
        unix_now(),
    ) {
        Ok(report) => {
            if crate::context_model_item_budget::model_item_tokens(&report)
                > crate::context_model_item_budget::MODEL_ITEM_TOKEN_LIMIT
            {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "evidence_model_item_budget_exceeded",
                    "Exact evidence exceeds the model-item budget; use a shorter exact quote in the same source. No evidence was delivered.",
                );
            }
            Json(report).into_response()
        }
        Err(source) => repository_error(source),
    }
}
