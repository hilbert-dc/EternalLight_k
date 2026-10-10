//! Host-only event delivery APIs. Never merge these into the candidate router.
use crate::server::{RuntimeState, bad_request, repository_error, unix_now};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    ProjectId, RepositoryError, SqliteStoryRepository, WorkExecutionAckRequest,
    WorkExecutionClaimRequest, WorkExecutionLeaseRequest, WorkExecutionOwnerRequest,
    WorkExecutionReleaseRequest,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListRequest {
    cursor: Option<String>,
    limit: Option<usize>,
}

fn run(
    state: RuntimeState,
    id: String,
    action: impl FnOnce(&mut SqliteStoryRepository, &ProjectId) -> Result<Value, RepositoryError>,
) -> Response {
    let project = match ProjectId::parse(id) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project_id", e),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(r) => r,
        Err(e) => return repository_error(e),
    };
    match action(&mut repo, &project) {
        Ok(value) => Json(value).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Query(request): Query<ListRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.work_execution_events(p, request.cursor.as_deref(), request.limit.unwrap_or(20))
    })
}
pub(crate) async fn claim(
    State(state): State<RuntimeState>,
    Path((project, event)): Path<(String, String)>,
    Json(request): Json<WorkExecutionClaimRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.claim_work_execution_event(p, &event, request, unix_now())
    })
}
pub(crate) async fn ack(
    State(state): State<RuntimeState>,
    Path((project, event)): Path<(String, String)>,
    Json(request): Json<WorkExecutionAckRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.ack_work_execution_event(p, &event, request, unix_now())
    })
}
pub(crate) async fn release(
    State(state): State<RuntimeState>,
    Path((project, event)): Path<(String, String)>,
    Json(request): Json<WorkExecutionReleaseRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.release_work_execution_event(p, &event, request)
    })
}
pub(crate) async fn acquire(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<WorkExecutionOwnerRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.acquire_work_execution_owner(p, &work, request, unix_now())
    })
}
pub(crate) async fn renew(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<WorkExecutionLeaseRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.renew_work_execution_owner(p, &work, request, unix_now())
    })
}
pub(crate) async fn release_owner(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<WorkExecutionLeaseRequest>,
) -> Response {
    run(state, project, |repo, p| {
        repo.release_work_execution_owner(p, &work, request)
    })
}
