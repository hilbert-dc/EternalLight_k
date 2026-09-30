use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use novel_codex_story_core::{
    ConditionEvaluationControl, ConditionEvaluationTarget, ProjectId, SqliteStoryRepository,
};
use serde::Deserialize;

use crate::server::{RuntimeState, bad_request, repository_error};

// Merged into the author router before its authorization middleware is applied.
pub(crate) fn routes() -> Router<RuntimeState> {
    Router::new()
        .route(
            "/api/projects/{project_id}/condition-evaluations",
            post(start),
        )
        .route(
            "/api/projects/{project_id}/condition-evaluations/{job_id}",
            get(read),
        )
        .route(
            "/api/projects/{project_id}/condition-evaluations/{job_id}/control",
            post(control),
        )
        .route(
            "/api/projects/{project_id}/condition-evaluations/{job_id}/explanations",
            get(explanations),
        )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExplanationQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

async fn explanations(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Query(query): Query<ExplanationQuery>,
) -> Response {
    let (repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.condition_explanations(
        &project,
        &id,
        query.cursor.as_deref(),
        query.limit.unwrap_or(16),
    ) {
        Ok(page) => Json(page).into_response(),
        Err(error) => repository_error(error),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ControlRequest {
    command: Command,
    expected_version: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command {
    Pause,
    Resume,
    Cancel,
}

async fn start(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(target): Json<ConditionEvaluationTarget>,
) -> Response {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.start_condition_evaluation(&project, target) {
        Ok(job) => (StatusCode::CREATED, Json(job)).into_response(),
        Err(error) => repository_error(error),
    }
}

async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
) -> Response {
    let (repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.condition_evaluation(&project, &id) {
        Ok(job) => Json(job).into_response(),
        Err(error) => repository_error(error),
    }
}

async fn control(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Json(request): Json<ControlRequest>,
) -> Response {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let command = match request.command {
        Command::Pause => ConditionEvaluationControl::Pause,
        Command::Resume => ConditionEvaluationControl::Resume,
        Command::Cancel => ConditionEvaluationControl::Cancel,
    };
    match repository.control_condition_evaluation(&project, &id, request.expected_version, command)
    {
        Ok(job) => Json(job).into_response(),
        Err(error) => repository_error(error),
    }
}

fn open(
    state: &RuntimeState,
    project: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project = ProjectId::parse(project)
        .map_err(|error| Box::new(bad_request("invalid_project_id", error)))?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|error| Box::new(repository_error(error)))?;
    Ok((repository, project))
}
