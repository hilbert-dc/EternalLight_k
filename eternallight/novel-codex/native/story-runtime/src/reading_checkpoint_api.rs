use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    CheckpointReadingRequest, ProjectId, SqliteStoryRepository, StoryQueryPlan,
};
use serde::Deserialize;

use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    thread_id: String,
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveRequest {
    thread_id: String,
    expected_revision: i64,
    plan: StoryQueryPlan,
    report_hash: String,
    expected_version: i64,
    summary: String,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(binding): Query<Binding>,
) -> Response {
    execute(state, project, task, actor, headers, binding, None)
}

pub(crate) async fn save(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SaveRequest>,
) -> Response {
    let binding = Binding {
        thread_id: request.thread_id,
        expected_revision: request.expected_revision,
    };
    let request = CheckpointReadingRequest {
        plan: request.plan,
        report_hash: request.report_hash,
        expected_version: request.expected_version,
        summary: request.summary,
    };
    execute(state, project, task, actor, headers, binding, Some(request))
}

fn execute(
    state: RuntimeState,
    project: String,
    task: String,
    actor: Actor,
    headers: HeaderMap,
    binding: Binding,
    request: Option<CheckpointReadingRequest>,
) -> Response {
    let Ok(project) = ProjectId::parse(project) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_project",
            "Invalid project",
        );
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(source) => return repository_error(source),
    };
    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&task),
        Some(&binding.thread_id),
        Some(binding.expected_revision),
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
    let result = if let Some(request) = request {
        if let Err(response) = crate::recovery_binding::bind(
            &mut repo,
            &headers,
            Some(&task),
            "checkpoint_reading",
            None,
        ) {
            return repository_error(response);
        }
        repo.checkpoint_reading(&task, &request, unix_now())
    } else {
        if let Err(source) =
            crate::recovery_binding::require_root(&headers, &task, "read_reading_progress")
        {
            return repository_error(source);
        }
        repo.reading_progress(&task, unix_now())
    };
    match result {
        Ok(value) => Json(value).into_response(),
        Err(source) => repository_error(source),
    }
}
