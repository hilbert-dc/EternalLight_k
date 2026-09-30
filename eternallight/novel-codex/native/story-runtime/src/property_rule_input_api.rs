use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use serde::Deserialize;

use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    thread_id: String,
    expected_revision: i64,
    cursor: Option<String>,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(binding): Query<Binding>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project", source),
    };
    if binding
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.len() > 100)
    {
        return bad_request("invalid_cursor", "Rule cursor exceeds its bound");
    }
    let repo = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&id),
        Some(&binding.thread_id),
        Some(binding.expected_revision),
    ) {
        Ok(Some(task)) if task.operation == "generatePropertyRules" => task,
        Ok(_) => {
            return error(
                StatusCode::FORBIDDEN,
                "rule_task_required",
                "A bound rule generation task is required",
            );
        }
        Err(response) => return *response,
    };
    if let Err(source) =
        crate::recovery_binding::require_root(&headers, &task, "read_property_rules")
    {
        return repository_error(source);
    }
    match repo.task_property_rule_page(&task, binding.cursor.as_deref()) {
        Ok(value) => Json(value).into_response(),
        Err(source) => repository_error(source),
    }
}
