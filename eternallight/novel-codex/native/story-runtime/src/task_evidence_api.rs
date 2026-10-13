use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::ProjectId;
use serde::Deserialize;

use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskEvidenceApiRequest", optional_fields = nullable)]
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
    crate::repository_executor::with_repo(state.clone(), move |repository| {
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
            if crate::context_model_item_budget::model_item_bytes(&report)
                > crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT
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

    }).await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/source-evidence",
        axum::routing::post(crate::task_evidence_api::resolve),
    )
}

use novel_codex_story_core::{TaskEvidenceRepository};
