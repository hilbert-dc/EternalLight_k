use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository, StoryQueryPlan};
use serde::Deserialize;

use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};

#[derive(Deserialize)]
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
}
