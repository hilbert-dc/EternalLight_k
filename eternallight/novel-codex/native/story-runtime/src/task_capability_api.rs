use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, RenewTaskCapability};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::{RuntimeState, bad_request, repository_error, unix_now};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "TaskCapabilityApiRenewRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RenewRequest {
    thread_id: String,
    capability_hash: String,
    expected_expires_at: i64,
    operation: String,
    target: Value,
    model: Option<String>,
}

pub(crate) async fn renew(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Json(request): Json<RenewRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.renew_task_capability(RenewTaskCapability {
            project_id: &project_id,
            task_id: &task_id,
            thread_id: &request.thread_id,
            capability_hash: &request.capability_hash,
            expected_expires_at: request.expected_expires_at,
            operation: &request.operation,
            target: &request.target,
            model: request.model.as_deref(),
            now: unix_now(),
        }) {
            Ok(task) => Json(json!({"taskId":task.task_id,
            "expiresAt":task.capability_expires_at}))
            .into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/capability/renew",
        axum::routing::post(crate::task_capability_api::renew),
    )
}

use novel_codex_story_core::{TaskCapabilityRenewalRepository};
