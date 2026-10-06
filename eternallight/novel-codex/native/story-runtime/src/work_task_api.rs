use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, error, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ControlRequest {
    expected_version: i64,
    command: String,
}

#[derive(Deserialize)]
pub(crate) struct ListRequest {
    cursor: Option<String>,
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(request): Query<ListRequest>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_tasks_page(&project, request.cursor.as_deref()) {
        Ok(page) => Json(json!(page)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, id)): Path<(String, String)>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_task(&project, &id) {
        Ok(task) => Json(json!(task)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn from_attempt(
    State(state): State<RuntimeState>,
    Path((project_id, id)): Path<(String, String)>,
) -> Response {
    let (repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.work_task_for_attempt(&project, &id) {
        Ok(task) => Json(json!(task)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn control(
    State(state): State<RuntimeState>,
    Path((project_id, id)): Path<(String, String)>,
    Json(request): Json<ControlRequest>,
) -> Response {
    if !matches!(
        request.command.as_str(),
        "pause" | "resume" | "cancel" | "recoverSavedResult"
    ) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_command",
            "Unknown task control",
        );
    }
    let (mut repo, project) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repo.control_work_task(
        &project,
        &id,
        &request.command,
        request.expected_version,
        unix_now(),
    ) {
        Ok(task) => Json(json!(task)).into_response(),
        Err(source) => repository_error(source),
    }
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project = ProjectId::parse(project_id)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repo = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repo, project))
}
