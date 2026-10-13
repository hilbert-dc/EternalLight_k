use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, RecoveryOperationBinding};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::server::{Actor, RuntimeState, bad_request, error, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "RecoveryApiOperationLookup", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OperationLookup {
    task_id: String,
    thread_id: String,
    turn_id: String,
    call_id: String,
    tool_name: String,
    request_hash: String,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "RecoveryApiReconcileRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReconcileRequest {
    task_id: String,
    thread_id: String,
    turn_id: String,
    call_id: String,
    tool_name: String,
    request_hash: String,
    arguments: serde_json::Value,
}

/// Host/Author-only factual reconciliation, never a candidate write authority.
pub(crate) async fn reconcile_operation(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<ReconcileRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        let binding = RecoveryOperationBinding {
            project_id: project,
            task_id: request.task_id,
            thread_id: request.thread_id,
            turn_id: request.turn_id,
            call_id: request.call_id,
            tool_name: request.tool_name,
            request_hash: request.request_hash,
            idempotency_ref: None,
            now: unix_now(),
        };
        match repo.reconcile_candidate_operation(&binding, &request.arguments) {
            Ok(operation) => Json(json!({"operation":operation})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Read one original operation without acquiring a fence or settling anything.
pub(crate) async fn read_operation(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(request): Query<OperationLookup>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        let capability_hash = if actor == Actor::Candidate {
            let get = |key: &str| headers.get(key).and_then(|value| value.to_str().ok());
            if get("x-novel-root-thread") != Some(request.thread_id.as_str())
                || get("x-novel-caller-thread") != Some(request.thread_id.as_str())
                || get("x-novel-parent-thread").is_some_and(|v| !v.is_empty())
                || get("x-novel-turn") != Some(request.turn_id.as_str())
                || get("x-novel-tool-call") != Some(request.call_id.as_str())
                || get("x-novel-tool-name") != Some(request.tool_name.as_str())
                || get("x-novel-request-hash") != Some(request.request_hash.as_str())
            {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "recovery_identity_invalid",
                    "Receipt reads require the exact original root call identity",
                );
            }
            let Some(capability) = get("x-novel-task-capability").filter(|v| v.len() >= 24) else {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "task_capability_required",
                    "Receipt reads require their own task capability",
                );
            };
            Some(format!("{:x}", Sha256::digest(capability.as_bytes())))
        } else {
            None
        };
        let binding = RecoveryOperationBinding {
            project_id: project,
            task_id: request.task_id,
            thread_id: request.thread_id,
            turn_id: request.turn_id,
            call_id: request.call_id,
            tool_name: request.tool_name,
            request_hash: request.request_hash,
            idempotency_ref: None,
            now: unix_now(),
        };
        match repo.read_recovery_operation(&binding, capability_hash.as_deref()) {
            Ok(operation) => Json(json!({"operation":operation})).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "RecoveryApiLookup", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Lookup {
    task_ids: Vec<String>,
    fence: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<usize>", optional)]
    cursor: usize,
    #[serde(default = "default_limit")]
    #[ts(as = "Option<usize>", optional)]
    limit: usize,
}
fn default_limit() -> usize {
    500
}

pub(crate) async fn snapshot(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<Lookup>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repo.recovery_snapshot(
            &project,
            &request.task_ids,
            request.fence.as_deref(),
            request.cursor,
            request.limit,
            unix_now(),
        ) {
            Ok(value) => Json(value).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/tasks/recovery-operations",
            axum::routing::post(crate::recovery_api::snapshot),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-snapshot",
            axum::routing::post(crate::recovery_api::snapshot),
        )
        .route(
            "/api/projects/{project_id}/tasks/recovery-operations/reconcile",
            axum::routing::post(crate::recovery_api::reconcile_operation)
                .layer(axum::extract::DefaultBodyLimit::max(132 * 1024)),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/recovery-operations",
        axum::routing::get(crate::recovery_api::read_operation),
    )
}

use novel_codex_story_core::{RecoveryOperationReadRepository, RecoveryOperationReconcileRepository, RecoverySnapshotRepository};
