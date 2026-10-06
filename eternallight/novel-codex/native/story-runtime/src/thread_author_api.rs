use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;

use crate::server::{RuntimeState, bad_request, repository_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadRequest {
    turn_ids: Vec<String>,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, thread_id)): Path<(String, String)>,
    Json(request): Json<ReadRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    match repository.thread_author_messages(&project_id, &thread_id, &request.turn_ids) {
        Ok(messages) => Json(messages).into_response(),
        Err(source) => repository_error(source),
    }
}
