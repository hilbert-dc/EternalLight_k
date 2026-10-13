use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, TaskPlanningReviewRequest};
use serde_json::json;

pub(crate) async fn register(
    State(state): State<RuntimeState>,
    Path((project, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<TaskPlanningReviewRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
    let Ok(project) = ProjectId::parse(project) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_project",
            "Invalid project",
        );
    };

    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project,
        Some(&task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return error(
                StatusCode::FORBIDDEN,
                "task_required",
                "A task capability is required",
            );
        }
        Err(response) => return *response,
    };
    if headers.get("x-novel-tool-name").is_none() {
        return error(
            StatusCode::FORBIDDEN,
            "root_tool_required",
            "Root tool identity is required",
        );
    }
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&task),
        "request_planning_review",
        Some(&request.idempotency_key),
    ) {
        return repository_error(source);
    }
    match repository.request_task_planning_review(&project, &task_id, &request, unix_now()) {
        Ok(run) => {
            let receipt = json!({"id":run["id"],"projectId":run["projectId"],"workTaskId":run["workTaskId"],
            "sourceTaskId":run["sourceTaskId"],"sourceIntentRevision":run["sourceIntentRevision"],
            "artifactKind":run["artifactKind"],"revisionId":run["revisionId"],"targetNodeId":run["targetNodeId"],
            "roles":run["roles"],"status":run["status"],"reportCount":run["reports"].as_array().map_or(/*default*/0,Vec::len),"registrationOnly":true});
            if crate::context_model_item_budget::model_item_bytes(&receipt)
                > crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT
            {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "planning_registration_receipt_budget_exceeded",
                    "Registration may already be saved. Read this source task's exact registered runs; do not create another run or assume reports were delivered.",
                );
            }
            Json(receipt).into_response()
        }
        Err(source) => repository_error(source),
    }

    }).await
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path((project, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        if actor != Actor::Author {
            return error(
                StatusCode::FORBIDDEN,
                "author_required",
                "Author read access is required",
            );
        }
        let Ok(project) = ProjectId::parse(project) else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_project",
                "Invalid project",
            );
        };

        match repository.task_planning_reviews(&project, &task_id) {
            Ok(runs) => Json(json!({"runs":runs})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/planning-reviews",
        axum::routing::get(crate::task_planning_review_api::list)
            .post(crate::task_planning_review_api::register),
    )
}

use novel_codex_story_core::{TaskPlanningReviewRepository};
