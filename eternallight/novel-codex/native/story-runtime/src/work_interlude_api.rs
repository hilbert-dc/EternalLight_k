use crate::server::{RuntimeState, bad_request, repository_error, unix_now};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BeginWorkInterlude, ProjectId, ReturnWorkInterlude,
};
pub(crate) async fn begin(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<BeginWorkInterlude>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(v) => v,
            Err(e) => return bad_request("invalid_project_id", e),
        };

        match repo.begin_work_interlude(&project, &work, request, unix_now()) {
            Ok(v) => Json(v).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
pub(crate) async fn return_to_parent(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<ReturnWorkInterlude>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(v) => v,
            Err(e) => return bad_request("invalid_project_id", e),
        };

        match repo.return_work_interlude(&project, &work, request) {
            Ok(v) => Json(v).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/work-tasks/{id}/interludes",
            axum::routing::post(crate::work_interlude_api::begin),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{id}/interludes/return",
            axum::routing::post(crate::work_interlude_api::return_to_parent),
        )
}

use novel_codex_story_core::{WorkInterludesRepository};
