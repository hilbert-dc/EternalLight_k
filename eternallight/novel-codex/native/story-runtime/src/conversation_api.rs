//! UI-host conversation ownership and metadata. These routes are author-only, not MCP tools.
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    CreateConversationRequest, ProjectId, RepositoryError, SqliteStoryRepository,
    UpdateConversationRequest,
};
use serde::Deserialize;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::server::{RuntimeState, bad_request, error, repository_error};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListQuery {
    #[serde(default)]
    archived: bool,
}
fn open(
    state: &RuntimeState,
    project: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project = ProjectId::parse(project)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project))
}
fn timestamp() -> Result<i64, Response> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|source| crate::server::internal_error(source))?
        .as_millis();
    i64::try_from(now).map_err(|source| crate::server::internal_error(source))
}
fn conversation_error(source: RepositoryError) -> Response {
    if source == RepositoryError::CrossProjectReference {
        error(
            StatusCode::FORBIDDEN,
            "conversation_owner_mismatch",
            "The conversation belongs to another project",
        )
    } else {
        repository_error(source)
    }
}
pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    let (repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.conversations(&project, query.archived) {
        Ok(conversations) => Json(json!({"conversations": conversations})).into_response(),
        Err(source) => conversation_error(source),
    }
}
pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, thread_id)): Path<(String, String)>,
) -> Response {
    let (repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.conversation(&project, &thread_id) {
        Ok(conversation) => Json(conversation).into_response(),
        Err(source) => conversation_error(source),
    }
}
pub(crate) async fn create(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<CreateConversationRequest>,
) -> Response {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let now = match timestamp() {
        Ok(value) => value,
        Err(response) => return response,
    };
    match repository.create_conversation(&project, request, now) {
        Ok(conversation) => Json(conversation).into_response(),
        Err(source) => conversation_error(source),
    }
}
pub(crate) async fn update(
    State(state): State<RuntimeState>,
    Path((project, thread_id)): Path<(String, String)>,
    Json(request): Json<UpdateConversationRequest>,
) -> Response {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let now = match timestamp() {
        Ok(value) => value,
        Err(response) => return response,
    };
    match repository.update_conversation(&project, &thread_id, request, now) {
        Ok(conversation) => Json(conversation).into_response(),
        Err(source) => conversation_error(source),
    }
}
