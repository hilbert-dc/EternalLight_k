//! Host-only event delivery APIs. Never merge these into the candidate router.
use crate::server::{RuntimeState, bad_request, repository_error, unix_now};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    ProjectId, RepositoryError, WorkExecutionAckRequest, WorkExecutionClaimRequest,
    WorkExecutionLeaseRequest, WorkExecutionOwnerRequest, WorkExecutionReleaseRequest,
    WorkExecutionStore,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "WorkExecutionEventApiListRequest", optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListRequest {
    cursor: Option<String>,
    limit: Option<usize>,
}

fn run(
    state: RuntimeState,
    id: String,
    action: impl FnOnce(&mut WorkExecutionStore<'_>, &ProjectId) -> Result<Value, RepositoryError>,
) -> Response {
    let project = match ProjectId::parse(id) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project_id", e),
    };
    let mut repo = match state.database.repository() {
        Ok(r) => r,
        Err(e) => return repository_error(e),
    };
    match action(&mut repo.work_execution(), &project) {
        Ok(value) => Json(value).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Query(request): Query<ListRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| {
            repo.list(p, request.cursor.as_deref(), request.limit.unwrap_or(20))
        })
    })
    .await
}
pub(crate) async fn claim(
    State(state): State<RuntimeState>,
    Path((project, event)): Path<(String, String)>,
    Json(request): Json<WorkExecutionClaimRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| {
            repo.claim(p, &event, request, unix_now())
        })
    })
    .await
}
pub(crate) async fn ack(
    State(state): State<RuntimeState>,
    Path((project, event)): Path<(String, String)>,
    Json(request): Json<WorkExecutionAckRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| {
            repo.acknowledge(p, &event, request, unix_now())
        })
    })
    .await
}
pub(crate) async fn release(
    State(state): State<RuntimeState>,
    Path((project, event)): Path<(String, String)>,
    Json(request): Json<WorkExecutionReleaseRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| repo.release(p, &event, request))
    })
    .await
}
pub(crate) async fn acquire(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<WorkExecutionOwnerRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| {
            repo.acquire_owner(p, &work, request, unix_now())
        })
    })
    .await
}
pub(crate) async fn renew(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<WorkExecutionLeaseRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| {
            repo.renew_owner(p, &work, request, unix_now())
        })
    })
    .await
}
pub(crate) async fn release_owner(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Json(request): Json<WorkExecutionLeaseRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        run(state, project, |repo, p| {
            repo.release_owner(p, &work, request)
        })
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/work-execution-events",
            axum::routing::get(crate::work_execution_event_api::list),
        )
        .route(
            "/api/projects/{project_id}/work-execution-events/{id}/claim",
            axum::routing::post(crate::work_execution_event_api::claim),
        )
        .route(
            "/api/projects/{project_id}/work-execution-events/{id}/ack",
            axum::routing::post(crate::work_execution_event_api::ack),
        )
        .route(
            "/api/projects/{project_id}/work-execution-events/{id}/release",
            axum::routing::post(crate::work_execution_event_api::release),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{id}/execution-owner",
            axum::routing::post(crate::work_execution_event_api::acquire),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{id}/execution-owner/renew",
            axum::routing::post(crate::work_execution_event_api::renew),
        )
        .route(
            "/api/projects/{project_id}/work-tasks/{id}/execution-owner/release",
            axum::routing::post(crate::work_execution_event_api::release_owner),
        )
}
