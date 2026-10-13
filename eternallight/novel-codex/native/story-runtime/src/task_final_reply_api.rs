use crate::server::{RuntimeState, bad_request, repository_error, unix_now};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, RecordTaskFinalReply};

pub(crate) async fn record(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Json(binding): Json<RecordTaskFinalReply>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(project) => project,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.record_task_final_reply(&project, &task, binding, unix_now()) {
            Ok(receipt) => Json(receipt).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(project) => project,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repo.task_final_reply(&project, &task) {
            Ok(receipt) => Json(receipt).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/final-reply",
        axum::routing::get(crate::task_final_reply_api::read)
            .post(crate::task_final_reply_api::record),
    )
}

use novel_codex_story_core::{TaskFinalReplyRepository};
