use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error};

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, action_id)): Path<(String, String, String)>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return repository_error(error),
    };
    match repository.task_action_permission(&project, &task_id, &action_id) {
        Ok(permission) => Json(json!({"permission":permission})).into_response(),
        Err(error) => repository_error(error),
    }
}
