use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Lookup {
    task_ids: Vec<String>,
    fence: Option<String>,
    #[serde(default)]
    cursor: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    500
}

pub(crate) async fn snapshot(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<Lookup>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    match repo.recovery_snapshot(
        &project,
        &request.task_ids,
        request.fence.as_deref(),
        request.cursor,
        request.limit,
        unix_now(),
    ) {
        Ok(value) => Json(value).into_response(),
        Err(source) => repository_error(source),
    }
}
