use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlRequest {
    command: String,
    expected_version: i64,
}

pub(crate) async fn start(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.start_fact_audit(&project_id) {
        Ok(job) => (StatusCode::CREATED, Json(json!(job))).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn latest(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.latest_fact_audit_job(&project_id) {
        Ok(job) => Json(json!({"job": job})).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, job_id)): Path<(String, String)>,
) -> Response {
    let (repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.fact_audit_job(&project_id, &job_id) {
        Ok(job) => Json(json!(job)).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn control(
    State(state): State<RuntimeState>,
    Path((project_id, job_id)): Path<(String, String)>,
    Json(request): Json<ControlRequest>,
) -> Response {
    if !matches!(request.command.as_str(), "pause" | "resume" | "cancel") {
        return bad_request("invalid_command", "Unknown fact audit control");
    }
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.control_fact_audit(
        &project_id,
        &job_id,
        &request.command,
        request.expected_version,
    ) {
        Ok(job) => Json(json!(job)).into_response(),
        Err(error) => repository_error(error),
    }
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id)
        .map_err(|error| Box::new(bad_request("invalid_project_id", error)))?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|error| Box::new(repository_error(error)))?;
    Ok((repository, project_id))
}
