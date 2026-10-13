use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ModelRunPauseRequest, ProjectId};
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "GenerationModelControlApiRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    run_id: String,
    expected_version: i64,
    expected_intent_revision: i64,
    instruction: String,
    idempotency_key: String,
}
pub(crate) async fn pause(
    State(state): State<RuntimeState>,
    Path((project, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
        if request.task_id != task_id {
            return bad_request("invalid_task", "control task does not match route");
        }
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project_id", e),
        };

        let task = match candidate_task(
            &repo,
            actor,
            &headers,
            &project,
            Some(&task_id),
            Some(&request.thread_id),
            Some(request.expected_revision),
        ) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "task_capability_required",
                    "live coordinator task required",
                );
            }
            Err(e) => return *e,
        };
        let control = ModelRunPauseRequest {
            run_id: request.run_id,
            expected_version: request.expected_version,
            expected_intent_revision: request.expected_intent_revision,
            instruction: request.instruction,
            idempotency_key: request.idempotency_key,
        };
        if let Err(e) = crate::recovery_binding::bind(
            &mut repo,
            &headers,
            Some(&task),
            "pause_generation_run",
            Some(&control.idempotency_key),
        ) {
            return repository_error(e);
        }
        match repo.pause_generation_run_for_model(&task, &control) {
            Ok(receipt) => Json(json!({"receipt":receipt})).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/generation-control",
        axum::routing::post(crate::generation_model_control_api::pause),
    )
}

use novel_codex_story_core::{GenerationModelControlRepository};
