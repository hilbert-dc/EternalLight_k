use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ExecutionScopeRequest, ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfirmScope {
    request: ExecutionScopeRequest,
    preview_digest: String,
}

pub(crate) async fn preview(
    State(state): State<RuntimeState>,
    Path((project, source)): Path<(String, String)>,
    Json(request): Json<ExecutionScopeRequest>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    match repo.preview_execution_scope(&project, &source, &request, unix_now()) {
        Ok(value) => Json(json!(value)).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn confirm(
    State(state): State<RuntimeState>,
    Path((project, source)): Path<(String, String)>,
    Json(request): Json<ConfirmScope>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    match repo.confirm_execution_scope(
        &project,
        &source,
        &request.request,
        &request.preview_digest,
        unix_now(),
    ) {
        Ok(value) => Json(value).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, source)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    match repo.execution_scope(&project, &source) {
        Ok(value) => Json(json!({"scope":value})).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn continue_work(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return repository_error(error),
    };
    match repo.continue_scoped_work(&project, &work) {
        Ok(value) => Json(json!(value)).into_response(),
        Err(error) => repository_error(error),
    }
}
