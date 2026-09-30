use crate::server::{RuntimeState, bad_request, repository_error, unix_now};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, RecordTaskFinalReply, SqliteStoryRepository};

pub(crate) async fn record(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Json(binding): Json<RecordTaskFinalReply>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(error) => return repository_error(error),
    };
    match repo.record_task_final_reply(&project, &task, binding, unix_now()) {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(error) => return repository_error(error),
    };
    match repo.task_final_reply(&project, &task) {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => repository_error(error),
    }
}
