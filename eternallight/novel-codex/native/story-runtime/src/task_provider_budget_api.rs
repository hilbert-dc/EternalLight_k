use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return repository_error(error),
    };
    match repository.task_provider_budget(&project, &task_id, unix_now()) {
        Ok(budget) => Json(budget).into_response(),
        Err(error) => repository_error(error),
    }
}
