use crate::server::{RuntimeState, bad_request, repository_error, unix_now};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BeginWorkInterlude, ProjectId, ReturnWorkInterlude, SqliteStoryRepository,
};
pub(crate) async fn begin(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<BeginWorkInterlude>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(v) => v,
        Err(e) => return bad_request("invalid_project_id", e),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(v) => v,
        Err(e) => return repository_error(e),
    };
    match repo.begin_work_interlude(&project, &work, request, unix_now()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn return_to_parent(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<ReturnWorkInterlude>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(v) => v,
        Err(e) => return bad_request("invalid_project_id", e),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(v) => v,
        Err(e) => return repository_error(e),
    };
    match repo.return_work_interlude(&project, &work, request) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
