use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error, unix_now};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{ChapterStructureSourceRequest, ProjectId};
use serde::Deserialize;

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "ChapterStructureApiListQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    assessment_state: Option<String>,
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let Ok(project) = ProjectId::parse(project) else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_project",
                "Invalid project",
            );
        };

        match repository.chapter_structure_changes(
            &project,
            query.cursor.as_deref(),
            query.limit.unwrap_or(10),
            query.assessment_state.as_deref(),
        ) {
            Ok(value) => Json(value).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let Ok(project) = ProjectId::parse(project) else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_project",
                "Invalid project",
            );
        };

        match repository.chapter_structure_change(&project, &id) {
            Ok(value) => Json(value).into_response(),
            Err(e) => repository_error(e),
        }
    })
    .await
}
pub(crate) async fn source(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<ChapterStructureSourceRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let Ok(project) = ProjectId::parse(project) else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_project",
                "Invalid project",
            );
        };

        let task = match candidate_task(
            &repository,
            actor,
            &headers,
            &project,
            Some(&id),
            Some(&request.thread_id),
            Some(request.expected_revision),
        ) {
            Ok(Some(task)) => task,
            Ok(None) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "structure_task_only",
                    "Task capability required",
                );
            }
            Err(response) => return *response,
        };
        if headers.get("x-novel-tool-name").is_none() {
            return error(
                StatusCode::FORBIDDEN,
                "structure_task_only",
                "Root tool identity required",
            );
        }
        if let Err(e) =
            crate::recovery_binding::require_root(&headers, &task, "read_chapter_structure_source")
        {
            return repository_error(e);
        }
        match repository.read_chapter_structure_source(&task, &request, unix_now()) {
            Ok(value) => {
                if !crate::context_model_item_budget::fits(&value) {
                    return error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "structure_source_budget_exceeded",
                        "No page delivered; reduce limit in the same source",
                    );
                }
                Json(value).into_response()
            }
            Err(e) => repository_error(e),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/chapter-structure-changes",
            axum::routing::get(crate::chapter_structure_api::list),
        )
        .route(
            "/api/projects/{project_id}/chapter-structure-changes/{change_id}",
            axum::routing::get(crate::chapter_structure_api::get),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/chapter-structure-source",
        axum::routing::post(crate::chapter_structure_api::source),
    )
}

use novel_codex_story_core::{ChapterStructureRepository, TaskChapterStructureRepository};
