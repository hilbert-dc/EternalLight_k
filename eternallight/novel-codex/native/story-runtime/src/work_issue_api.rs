//! Bound readers inspect only their actual Work's report references.
use crate::server::{Actor, RuntimeState, bad_request, candidate_task, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthorQuery {
    #[serde(default)]
    cursor: u64,
    #[serde(default = "default_limit")]
    limit: usize,
    issue_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TaskQuery {
    thread_id: String,
    expected_revision: i64,
    #[serde(default)]
    cursor: u64,
    #[serde(default = "default_limit")]
    limit: usize,
    issue_id: Option<String>,
}
fn default_limit() -> usize {
    8
}
pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, work)): Path<(String, String)>,
    Query(query): Query<AuthorQuery>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project", e),
    };
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(r) => r,
        Err(e) => return repository_error(e),
    };
    match repo.work_issues(
        &project,
        &work,
        query.issue_id.as_deref(),
        query.cursor,
        query.limit,
    ) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
pub(crate) async fn read_for_task(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(query): Query<TaskQuery>,
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
        Ok(None) => {
            return bad_request(
                "task_capability_required",
                "Actual task capability required",
            );
        }
        Err(e) => return *e,
    };
    match repo.task_work_issues(&task, query.issue_id.as_deref(), query.cursor, query.limit) {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}
