use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, StoryQueryPlan};
use serde::Deserialize;

use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "QueryPlanApiQueryPlanRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QueryPlanRequest {
    thread_id: String,
    expected_revision: i64,
    plan: StoryQueryPlan,
}

pub(crate) async fn execute(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<QueryPlanRequest>,
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
                    "task_query_only",
                    "A task capability is required",
                );
            }
            Err(response) => return *response,
        };
        match repository.query_task_plan(&task, &request.plan, unix_now()) {
            Ok(report) => Json(report).into_response(),
            Err(novel_codex_story_core::RepositoryError::TaskOutputForbidden) => error(
                StatusCode::FORBIDDEN,
                "query_scope_forbidden",
                "Query attempts to override its bound task scope",
            ),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/query",
        axum::routing::post(crate::query_plan_api::execute),
    )
}

use novel_codex_story_core::{QueryPlanRepository};
