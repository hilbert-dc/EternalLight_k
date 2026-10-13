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
#[ts(export, export_to = "runtime/", rename = "TaskImpactApiRequest", optional_fields = nullable)]
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
            if crate::context_model_item_budget::model_item_bytes(&report)
                > crate::context_model_item_budget::MODEL_ITEM_BYTE_LIMIT
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

    }).await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/impact-analysis",
        axum::routing::post(crate::task_impact_api::analyze),
    )
}

use novel_codex_story_core::{TaskImpactRepository};
