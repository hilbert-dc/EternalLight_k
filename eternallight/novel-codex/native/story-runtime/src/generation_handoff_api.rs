use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{GenerationHandoffRequest, ProjectId, SqliteStoryRepository};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    target_chapters: i64,
    instruction: String,
    expected_intent_revision: i64,
    idempotency_key: String,
}

pub(crate) async fn propose(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if task != request.task_id {
        return bad_request("invalid_task", "proposal task does not match route");
    }
    let project = match ProjectId::parse(project) {
        Ok(id) => id,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(source) => return repository_error(source),
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project,
        Some(&task),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return error(
                StatusCode::UNAUTHORIZED,
                "task_capability_required",
                "a live advisory task is required",
            );
        }
        Err(response) => return *response,
    };
    if !(1..=novel_codex_story_core::MAX_GENERATION_CHAPTERS).contains(&request.target_chapters)
        || request.instruction.trim().is_empty()
        || request.instruction.chars().count() > 1_000
        || request.idempotency_key.is_empty()
        || request.idempotency_key.len() > 160
        || request.idempotency_key.chars().any(char::is_control)
        || request.expected_intent_revision < 0
    {
        return bad_request("invalid_proposal", "invalid generation proposal");
    }
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&task),
        "request_generation_handoff",
        Some(&request.idempotency_key),
    ) {
        return repository_error(source);
    }
    match repository.propose_generation_handoff(
        &task,
        &GenerationHandoffRequest {
            target_chapters: request.target_chapters,
            instruction: request.instruction,
            expected_intent_revision: request.expected_intent_revision,
            idempotency_key: request.idempotency_key,
        },
    ) {
        Ok(proposal) => Json(json!({"proposal":proposal})).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, task)): Path<(String, String)>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(id) => id,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repo) => repo,
        Err(source) => return repository_error(source),
    };
    match repository.generation_handoff(&project, &task) {
        Ok(proposal) => Json(json!({"proposal":proposal})).into_response(),
        Err(source) => repository_error(source),
    }
}
