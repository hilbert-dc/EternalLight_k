use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SqliteStoryRepository;
use serde::Deserialize;
use serde_json::json;

use crate::server::RuntimeState;
use crate::server::error;
use crate::server::repository_error;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "SelectionApiSetBaselineRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetBaselineRequest {
    object_id: Option<String>,
    revision_id: String,
    expected_selection_revision: i64,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "SelectionApiClearBaselineRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClearBaselineRequest {
    object_id: Option<String>,
    expected_selection_revision: i64,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "SelectionApiContinuationRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContinuationRequest {
    target_id: Option<String>,
    working_revision_id: String,
    author_intent: String,
}

pub(crate) async fn set_baseline(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Json(request): Json<SetBaselineRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
    let (mut repository, project_id, object_id) =
        match open(&state, project_id, &kind, request.object_id) {
            Ok(value) => value,
            Err(response) => return *response,
        };
    let revision_id = match RevisionId::parse(request.revision_id) {
        Ok(value) => value,
        Err(source) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_revision_id",
                &source.to_string(),
            );
        }
    };
    match repository.set_working_baseline(
        &project_id, &kind, &object_id, &revision_id,
        request.expected_selection_revision, super::server::unix_now(),
    ) {
        Ok(value) => Json(json!({
            "objectKind": value.object_kind, "objectId": value.object_id,
            "revisionId": value.revision_id.as_ref().map(RevisionId::as_str),
            "revisionVersion": null, "revisionSource": "selected",
            "selectionRevision": value.selection_revision, "selectedAt": value.selected_at.to_string()
        })).into_response(),
        Err(source) => repository_error(source),
    }

    }).await
}

pub(crate) async fn clear_baseline(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Json(request): Json<ClearBaselineRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id, object_id) =
            match open(&state, project_id, &kind, request.object_id) {
                Ok(value) => value,
                Err(response) => return *response,
            };
        match repository.clear_working_baseline(
            &project_id,
            &kind,
            &object_id,
            request.expected_selection_revision,
            super::server::unix_now(),
        ) {
            Ok(value) => Json(json!({
                "objectKind": value.object_kind, "objectId": value.object_id,
                "revisionId": null, "selectionRevision": value.selection_revision, "cleared": true
            }))
            .into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn save_continuation(
    State(state): State<RuntimeState>,
    Path((project_id, kind)): Path<(String, String)>,
    Json(request): Json<ContinuationRequest>,
) -> Response {
    crate::repository_executor::run(state.clone(), move || {
        let (mut repository, project_id, object_id) =
            match open(&state, project_id, &kind, request.target_id) {
                Ok(value) => value,
                Err(response) => return *response,
            };
        let revision_id = match RevisionId::parse(request.working_revision_id) {
            Ok(value) => value,
            Err(source) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_revision_id",
                    &source.to_string(),
                );
            }
        };
        match repository.save_continuation_intent(
            &project_id,
            &kind,
            &object_id,
            &revision_id,
            &request.author_intent,
            super::server::unix_now(),
        ) {
            Ok(value) => Json(json!({
                "targetKind": value.target_kind, "targetId": value.target_id,
                "workingRevisionId": value.working_revision_id.as_str(),
                "authorIntent": value.author_intent, "status": value.status,
                "taskId": value.task_id, "updatedAt": value.updated_at.to_string()
            }))
            .into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

fn open(
    state: &RuntimeState,
    project_id: String,
    kind: &str,
    object_id: Option<String>,
) -> Result<(SqliteStoryRepository, ProjectId, String), Box<Response>> {
    if !matches!(kind, "brief" | "outline" | "plan" | "draft") {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            "invalid_working_baseline",
            "Working baseline kind is invalid",
        )));
    }
    let object_id = if kind == "draft" {
        object_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                Box::new(error(
                    StatusCode::BAD_REQUEST,
                    "invalid_working_baseline",
                    "Draft baseline requires a chapter",
                ))
            })?
    } else {
        if object_id.as_deref().is_some_and(|value| value != kind) {
            return Err(Box::new(error(
                StatusCode::BAD_REQUEST,
                "invalid_working_baseline",
                "Planning baseline object is invalid",
            )));
        }
        kind.to_owned()
    };
    let project_id = ProjectId::parse(project_id).map_err(|source| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            "invalid_project_id",
            &source.to_string(),
        ))
    })?;
    let repository = state
        .database
        .repository()
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id, object_id))
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/working-baselines/{kind}",
            axum::routing::put(crate::selection_api::set_baseline)
                .delete(crate::selection_api::clear_baseline),
        )
        .route(
            "/api/projects/{project_id}/continuations/{kind}",
            axum::routing::put(crate::selection_api::save_continuation),
        )
}

use novel_codex_story_core::{WorkspaceSelectionRepository};
