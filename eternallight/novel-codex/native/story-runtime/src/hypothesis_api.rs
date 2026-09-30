use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use novel_codex_story_core::{
    CreateHypothesis, HypothesisAssumption, ProjectId, SnapshotId, SqliteStoryRepository,
};
use serde::Deserialize;

use crate::RuntimeState;
use crate::server::{bad_request, internal_error, repository_error, unix_now};

#[derive(Deserialize)]
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
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let snapshot_id = match SnapshotId::parse(request.base_snapshot_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_snapshot_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
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
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project_id, hypothesis_id)): Path<(String, String)>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.read_hypothesis(&project_id, &hypothesis_id) {
        Ok(value) => Json(value).into_response(),
        Err(error) => repository_error(error),
    }
}
