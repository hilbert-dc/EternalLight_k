use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::ProjectId;
use serde::Deserialize;

use crate::server::{RuntimeState, bad_request, repository_error};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ThreadAuthorApiReadRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadRequest {
    turn_ids: Vec<String>,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, thread_id)): Path<(String, String)>,
    Json(request): Json<ReadRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.conversation_store().thread_author_messages(
            &project_id,
            &thread_id,
            &request.turn_ids,
        ) {
            Ok(messages) => Json(messages).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/threads/{thread_id}/author-messages",
        axum::routing::post(crate::thread_author_api::read),
    )
}
