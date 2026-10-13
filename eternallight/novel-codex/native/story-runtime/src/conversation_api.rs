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

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ConversationApiListQuery", optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListQuery {
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    archived: bool,
    #[serde(rename="scopeId")]
    scope_id: Option<String>,
}
fn open(
    state: &RuntimeState,
    project: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project = ProjectId::parse(project)
        .map_err(|source| Box::new(bad_request("invalid_project_id", source)))?;
    let repository = state.database.repository()
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
    crate::repository_executor::run(state.clone(), move || {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let result=match query.scope_id.as_deref(){Some(scope)=>repository.conversation_store().scoped_conversations(&project,query.archived,scope),None=>repository.conversation_store().conversations(&project, query.archived)};
    match result {
        Ok(conversations) => Json(json!({"conversations": conversations})).into_response(),
        Err(source) => conversation_error(source),
    }

    }).await
}
pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, thread_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match repository.conversation_store().conversation(&project, &thread_id) {
        Ok(conversation) => Json(conversation).into_response(),
        Err(source) => conversation_error(source),
    }

    }).await
}
pub(crate) async fn create(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<CreateConversationRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let now = match timestamp() {
        Ok(value) => value,
        Err(response) => return response,
    };
    match repository.conversation_store().create_conversation(&project, request, now) {
        Ok(conversation) => Json(conversation).into_response(),
        Err(source) => conversation_error(source),
    }

    }).await
}
pub(crate) async fn update(
    State(state): State<RuntimeState>,
    Path((project, thread_id)): Path<(String, String)>,
    Json(request): Json<UpdateConversationRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repository, project) = match open(&state, project) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let now = match timestamp() {
        Ok(value) => value,
        Err(response) => return response,
    };
    match repository.conversation_store().update_conversation(&project, &thread_id, request, now) {
        Ok(conversation) => Json(conversation).into_response(),
        Err(source) => conversation_error(source),
    }

    }).await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/conversations",
            axum::routing::get(crate::conversation_api::list).post(crate::conversation_api::create),
        )
        .route(
            "/api/projects/{project_id}/conversations/{thread_id}",
            axum::routing::get(crate::conversation_api::read).patch(crate::conversation_api::update),
        )
}
