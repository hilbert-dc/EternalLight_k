//! Capability-derived transport identity. Never accepts a model-selected task.
use crate::server::{Actor, RuntimeState, repository_error, unix_now};
use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::RepositoryError;
use serde_json::json;
use sha2::{Digest, Sha256};

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
    let denied = |reason: &str, field: &str| {
        (StatusCode::UNAUTHORIZED, Json(json!({"error":{"code":"task_binding_invalid",
            "message":"An active task capability and its bound root thread are required",
            "details":{"origin":"storyRuntime","phase":"binding","reason":reason,"instancePath":field,"nextAction":"verify_execution_binding"},"recovery":null}}))).into_response()
    };
    if actor == Actor::Author {
        return denied("candidate_actor_required", "/actor");
    }
    let Some(capability) = headers
        .get("x-novel-task-capability")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() >= 24)
    else {
        return denied("task_capability_required", "/taskCapability");
    };

    let task = match repo.resolve_tool_binding(
        &format!("{:x}", Sha256::digest(capability.as_bytes())),
        unix_now(),
    ) {
        Ok(task) => task,
        Err(
            RepositoryError::TaskCapabilityInvalid
            | RepositoryError::TaskBindingInvalid
            | RepositoryError::TaskNotFound
            | RepositoryError::TaskNotWritable,
        ) => return denied("task_capability_invalid", "/taskCapability"),
        Err(error) => return repository_error(error),
    };
    if headers
        .get("x-novel-root-thread")
        .and_then(|v| v.to_str().ok())
        != Some(task.thread_id.as_str())
    {
        return denied("root_thread_mismatch", "/rootThreadId");
    }
    let output = match repo.task_candidate_expectation(&task.task_id) {
        Ok(Some((object, revision))) => json!({"objectId":object.as_str(),
            "expectedCandidateRevisionId":revision.as_ref().map(|id| id.as_str())}),
        Ok(None) => serde_json::Value::Null,
        Err(error) => return repository_error(error),
    };
    Json(json!({"output":output,"projectId":task.project_id.as_str(),"taskId":task.task_id,"threadId":task.thread_id,"operation":task.operation,"inputRevision":task.input_revision})).into_response()

    }).await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/tool-binding",
        axum::routing::get(crate::tool_binding_api::read),
    )
}

use novel_codex_story_core::{TasksRepository};
