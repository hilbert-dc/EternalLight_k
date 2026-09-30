use crate::server::{RuntimeState, bad_request, internal_error, repository_error};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ModelingOutboxQuery, ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let now_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => match i64::try_from(duration.as_millis()) {
            Ok(value) => value,
            Err(error) => return internal_error(error),
        },
        Err(error) => return internal_error(error),
    };
    match repository.modeling_outbox_page(ModelingOutboxQuery {
        project_id: &project_id,
        cursor: query.cursor.as_deref(),
        limit: query.limit.unwrap_or(20),
        now_ms,
    }) {
        Ok(page) => Json(page).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn dispatch(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
) -> Response {
    let project = match ProjectId::parse(project_id) {
        Ok(project) => project,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(error) => return internal_error(error),
    };
    let now_ms = match SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
    {
        Some(now) => now,
        None => return internal_error("invalid system time"),
    };
    match repository.dispatch_modeling_intent(&project, now_ms) {
        Ok(action) => Json(serde_json::json!({"action":action})).into_response(),
        Err(error) => repository_error(error),
    }
}
