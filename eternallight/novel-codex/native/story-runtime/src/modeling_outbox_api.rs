use crate::server::{RuntimeState, bad_request, repository_error};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ModelingOutboxQuery, ProjectId};
use serde::Deserialize;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ModelingOutboxApiPageQuery", optional_fields = nullable)]
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
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project_id,
            cursor: query.cursor.as_deref(),
            limit: query.limit.unwrap_or(20),
        }) {
            Ok(page) => Json(page).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/modeling-outbox",
        axum::routing::get(crate::modeling_outbox_api::list),
    )
}

use novel_codex_story_core::{ModelingOutboxRepository};
