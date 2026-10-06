//! Author schedules coordination; only a bound root agent chooses an admitted action.
use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    ProjectId, SqliteStoryRepository, WorkActionRequest, WorkDecisionRequest,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadBinding {
    thread_id: String,
    expected_revision: i64,
    cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionBinding {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    #[serde(flatten)]
    request: WorkActionRequest,
}

pub(crate) async fn prepare(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    Json(request): Json<WorkDecisionRequest>,
) -> Response {
    if actor != Actor::Author {
        return error(
            StatusCode::FORBIDDEN,
            "author_required",
            "Only Host Author transport schedules a Work decision",
        );
    }
    let project = match ProjectId::parse(project) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project", e),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(r) => r,
        Err(e) => return repository_error(e),
    };
    match repo.prepare_work_decision(&project, &work, &request) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(query): Query<ReadBinding>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project", e),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(r) => r,
        Err(e) => return repository_error(e),
    };
    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&id),
        Some(&query.thread_id),
        Some(query.expected_revision),
    ) {
        Ok(Some(t)) => t,
        Ok(_) => {
            return error(
                StatusCode::FORBIDDEN,
                "decision_task_required",
                "A bound decision task is required",
            );
        }
        Err(e) => return *e,
    };
    if let Err(e) = crate::recovery_binding::require_root(&headers, &task, "read_work_status") {
        return repository_error(e);
    }
    match repo.task_work_decision(&task, query.cursor.as_deref()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn select(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<ActionBinding>,
) -> Response {
    if request.task_id != id {
        return bad_request("invalid_task_binding", "Path and request task differ");
    }
    let project = match ProjectId::parse(project) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project", e),
    };
    let mut repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(r) => r,
        Err(e) => return repository_error(e),
    };
    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(t)) => t,
        Ok(_) => {
            return error(
                StatusCode::FORBIDDEN,
                "decision_task_required",
                "A bound decision task is required",
            );
        }
        Err(e) => return *e,
    };
    if let Err(e) = crate::recovery_binding::require_root(&headers, &task, "request_work_action") {
        return repository_error(e);
    }
    if let Err(e) = crate::recovery_binding::bind(
        &mut repo,
        &headers,
        Some(&task),
        "request_work_action",
        Some(&request.request.idempotency_key),
    ) {
        return repository_error(e);
    }
    match repo.select_work_action(&task, &request.request) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
