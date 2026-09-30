use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppendIntent {
    expected_intent_revision: i64,
    author_text: String,
}

pub(crate) async fn append(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<AppendIntent>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(id) => id,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(source) => return repository_error(source),
    };
    match repository.append_task_intent(
        &project_id,
        &task_id,
        request.expected_intent_revision,
        &request.author_text,
        unix_now(),
    ) {
        Ok(intent) => Json(json!(intent)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn deliver(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, intent_revision)): Path<(String, String, i64)>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(id) => id,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(source) => return repository_error(source),
    };
    match repository.deliver_task_intent(&project_id, &task_id, intent_revision, unix_now()) {
        Ok(intent) => Json(json!(intent)).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(id) => id,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(source) => return repository_error(source),
    };
    match repository.task_intents(&project_id, &task_id) {
        Ok(intents) => Json(json!({"intents": intents})).into_response(),
        Err(source) => repository_error(source),
    }
}
