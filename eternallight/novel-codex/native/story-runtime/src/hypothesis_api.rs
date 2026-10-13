use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use novel_codex_story_core::{
    CreateHypothesis, HypothesisAssumption, ProjectId, SnapshotId,
};
use serde::Deserialize;

use crate::RuntimeState;
use crate::server::{bad_request, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "HypothesisApiCreateHypothesisRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateHypothesisRequest {
    base_snapshot_id: String,
    intent: String,
    assumptions: Vec<HypothesisAssumption>,
    idempotency_key: String,
}

pub(crate) async fn create(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateHypothesisRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let snapshot_id = match SnapshotId::parse(request.base_snapshot_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_snapshot_id", error),
        };

        match repository.create_hypothesis(CreateHypothesis {
            project_id: &project_id,
            base_snapshot_id: &snapshot_id,
            intent: &request.intent,
            assumptions: &request.assumptions,
            idempotency_key: &request.idempotency_key,
            now: unix_now(),
        }) {
            Ok(value) => (StatusCode::CREATED, Json(value)).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, hypothesis_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.read_hypothesis(&project_id, &hypothesis_id) {
            Ok(value) => Json(value).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/hypotheses",
            axum::routing::post(crate::hypothesis_api::create),
        )
        .route(
            "/api/projects/{project_id}/hypotheses/{hypothesis_id}",
            axum::routing::get(crate::hypothesis_api::read),
        )
}

use novel_codex_story_core::{HypothesesRepository};
