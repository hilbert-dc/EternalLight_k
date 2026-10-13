//! Bound readers inspect only their actual Work's report references.
use crate::server::{Actor, RuntimeState, bad_request, candidate_task, repository_error};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::ProjectId;
use serde::Deserialize;
#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "WorkIssueApiAuthorQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthorQuery {
    #[serde(default)]
    #[ts(as = "Option<u64>", optional)]
    cursor: u64,
    #[serde(default = "default_limit")]
    #[ts(as = "Option<usize>", optional)]
    limit: usize,
    issue_id: Option<String>,
}
#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "WorkIssueApiTaskQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TaskQuery {
    thread_id: String,
    expected_revision: i64,
    #[serde(default)]
    #[ts(as = "Option<u64>", optional)]
    cursor: u64,
    #[serde(default = "default_limit")]
    #[ts(as = "Option<usize>", optional)]
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
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
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
    })
    .await
}
pub(crate) async fn read_for_task(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(query): Query<TaskQuery>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(p) => p,
            Err(e) => return bad_request("invalid_project", e),
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
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/work-tasks/{work_id}/issues",
        axum::routing::get(crate::work_issue_api::read),
    )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/work-issues",
        axum::routing::get(crate::work_issue_api::read_for_task),
    )
}

use novel_codex_story_core::{WorkIssuesRepository};
