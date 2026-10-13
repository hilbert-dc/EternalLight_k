use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ExecutionScopeRequest, ProjectId};
use serde::Deserialize;
use serde_json::json;

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ExecutionScopeApiConfirmScope", optional_fields = nullable)]
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
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.preview_execution_scope(&project, &source, &request, unix_now()) {
            Ok(value) => Json(json!(value)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn confirm(
    State(state): State<RuntimeState>,
    Path((project, source)): Path<(String, String)>,
    Json(request): Json<ConfirmScope>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
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
    })
    .await
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, source)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.execution_scope(&project, &source) {
            Ok(value) => Json(json!({"scope":value})).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/execution-scope/preview",
            axum::routing::post(crate::execution_scope_api::preview),
        )
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/execution-scope",
            axum::routing::get(crate::execution_scope_api::read)
                .post(crate::execution_scope_api::confirm),
        )
}

use novel_codex_story_core::{ExecutionScopeRepository};
