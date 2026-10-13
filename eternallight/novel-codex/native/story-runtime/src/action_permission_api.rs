use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::ProjectId;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error};

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, task_id, action_id)): Path<(String, String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project = match ProjectId::parse(project_id) {
            Ok(project) => project,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.task_action_permission(&project, &task_id, &action_id) {
            Ok(permission) => Json(json!({"permission":permission})).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/actions/{action_id}/permission",
        axum::routing::get(crate::action_permission_api::read),
    )
}

use novel_codex_story_core::{TaskActionPermitRepository};
