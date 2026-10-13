use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::ProjectId;
use serde_json::Value;

use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};

pub(crate) async fn run(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(mut request): Json<Value>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
    if request.to_string().len() > 8_192 {
        return error(
            StatusCode::BAD_REQUEST,
            "hypothesis_input_budget_exceeded",
            "Complete hypothesis input must fit 8KiB",
        );
    }
    let Ok(project) = ProjectId::parse(project) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_project",
            "Invalid project",
        );
    };
    let Some(thread) = request["threadId"].as_str().map(str::to_owned) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_thread",
            "threadId is required",
        );
    };
    let Some(revision) = request["expectedRevision"].as_i64().filter(|n| *n >= 0) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_revision",
            "expectedRevision is required",
        );
    };

    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project,
        Some(&task),
        Some(&thread),
        Some(revision),
    ) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return error(
                StatusCode::FORBIDDEN,
                "task_hypothesis_only",
                "A task capability is required",
            );
        }
        Err(response) => return *response,
    };
    if headers.get("x-novel-tool-name").is_none() {
        return error(
            StatusCode::FORBIDDEN,
            "task_hypothesis_only",
            "Root tool identity is required",
        );
    }
    if let Err(source) =
        crate::recovery_binding::require_root(&headers, &task, "simulate_story_hypothesis")
    {
        return repository_error(source);
    }
    let Some(fields) = request.as_object_mut() else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_hypothesis",
            "Expected an action object",
        );
    };
    fields.remove("threadId");
    fields.remove("expectedRevision");
    match repository.task_hypothesis(&task, &request, unix_now()) {
        Ok(report) => {
            if crate::context_model_item_budget::model_item_bytes(&report)
                > crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT
            {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "hypothesis_model_item_budget_exceeded",
                    "No complete report was delivered. A simulate record may already be saved; retry the identical logicalKey/intent/assumptions with a smaller findings limit to read it without creating another record. For readBaseline use a smaller limit or exact entityId. Unknown/unshown consequences do not prove no impact.",
                );
            }
            Json(report).into_response()
        }
        Err(source) => repository_error(source),
    }

    }).await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/hypothesis",
        axum::routing::post(crate::task_hypothesis_api::run),
    )
}

use novel_codex_story_core::{TaskHypothesisRepository};
