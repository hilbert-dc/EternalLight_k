//! Author-only transport for persistent submissions; models cannot author commands.
use crate::server::{RuntimeState, bad_request};
fn repository_error(error: novel_codex_story_core::RepositoryError) -> Response {
    use novel_codex_story_core::RepositoryError::*;
    match error {
        TaskNotWritable => crate::server::error(
            axum::http::StatusCode::CONFLICT,
            "message_held",
            "消息触发条件尚未满足或交付仍待核对",
        ),
        TaskInputStale | WorkTaskStale => crate::server::error(
            axum::http::StatusCode::CONFLICT,
            "message_stale",
            "消息或原任务控制已变化，请重新读取",
        ),
        other => crate::server::repository_error(other),
    }
}
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    ChangeConversationMessage, ProjectId, SqliteStoryRepository, SubmitConversationMessage,
};
use serde_json::json;
fn open(
    state: &RuntimeState,
    project: String,
) -> Result<(SqliteStoryRepository, ProjectId), Response> {
    let project = ProjectId::parse(project).map_err(|e| bad_request("invalid_project_id", e))?;
    Ok((
        state.database.repository().map_err(repository_error)?,
        project,
    ))
}
pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path((project, thread)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repo, project) = match open(&state, project) {
        Ok(x) => x,
        Err(e) => return e,
    };
    match repo.conversation_store().conversation_messages(&project, &thread) {
        Ok(messages) => Json(json!({"messages":messages})).into_response(),
        Err(e) => repository_error(e),
    }

    }).await
}
pub(crate) async fn submit(
    State(state): State<RuntimeState>,
    Path((project, thread)): Path<(String, String)>,
    Json(request): Json<SubmitConversationMessage>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repo, project) = match open(&state, project) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    match repo.conversation_store().submit_conversation_message(&project, &thread, request, now) {
        Ok(message) => Json(message).into_response(),
        Err(e) => repository_error(e),
    }

    }).await
}
pub(crate) async fn change(
    State(state): State<RuntimeState>,
    Path((project, thread, message)): Path<(String, String, String)>,
    Json(request): Json<ChangeConversationMessage>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repo, project) = match open(&state, project) {
        Ok(x) => x,
        Err(e) => return e,
    };
    match repo.conversation_store().change_conversation_message(&project, &thread, &message, request) {
        Ok(message) => Json(message).into_response(),
        Err(e) => repository_error(e),
    }

    }).await
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, thread, message)): Path<(String, String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repo, project) = match open(&state, project) {
        Ok(x) => x,
        Err(e) => return e,
    };
    match repo.conversation_store().conversation_message(&project, &thread, &message) {
        Ok(message) => Json(message).into_response(),
        Err(e) => repository_error(e),
    }

    }).await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route("/api/projects/{project_id}/conversations/{thread_id}/messages", axum::routing::get(crate::conversation_message_api::list).post(crate::conversation_message_api::submit))
        .route("/api/projects/{project_id}/conversations/{thread_id}/messages/{message_id}", axum::routing::get(crate::conversation_message_api::read).post(crate::conversation_message_api::change))
}
