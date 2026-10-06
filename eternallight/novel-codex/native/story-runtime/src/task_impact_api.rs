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
    entity_id: String,
    max_hops: Option<usize>,
    limit: Option<usize>,
}

pub(crate) async fn analyze(
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
                "task_impact_only",
                "A task capability is required",
            );
        }
        Err(response) => return *response,
    };
    if headers.get("x-novel-tool-name").is_none() {
        return error(
            StatusCode::FORBIDDEN,
            "task_impact_only",
            "Root tool identity is required",
        );
    }
    if let Err(source) =
        crate::recovery_binding::require_root(&headers, &task, "analyze_story_impact")
    {
        return repository_error(source);
    }
    match repository.analyze_task_impact(
        &task,
        &request.entity_id,
        request.max_hops.unwrap_or(2),
        request.limit.unwrap_or(10),
        unix_now(),
    ) {
        Ok(report) => {
            if crate::context_model_item_budget::model_item_tokens(&report)
                > crate::context_model_item_budget::MODEL_ITEM_TOKEN_LIMIT
            {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "impact_model_item_budget_exceeded",
                    "Impact report exceeds the model-item budget; reduce limit/maxHops in the same task. No report was delivered; incomplete evidence does not prove no impact.",
                );
            }
            Json(report).into_response()
        }
        Err(source) => repository_error(source),
    }
}
