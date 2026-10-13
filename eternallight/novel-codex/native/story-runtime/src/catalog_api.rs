use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{CatalogRequest, ProjectId, RepositoryError};

use crate::server::{RuntimeState, error, repository_error};

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_project_id",
                    "作品标识无效。",
                );
            }
        };

        match repository.manuscript_catalog(&project) {
            Ok(value) => Json(value).into_response(),
            Err(source) => failure(source),
        }
    })
    .await
}

pub(crate) async fn update(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Json(request): Json<CatalogRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_project_id",
                    "作品标识无效。",
                );
            }
        };

        match repository.update_manuscript_catalog(&project, &request) {
            Ok(value) => Json(value).into_response(),
            Err(source) => failure(source),
        }
    })
    .await
}

pub(crate) fn failure(source: RepositoryError) -> Response {
    if let RepositoryError::InvalidContent(code) = &source {
        let (status, message) = match code.as_str() {
            "catalog_revision_changed" => {
                (StatusCode::CONFLICT, "目录已变化，请重新读取后再修改。")
            }
            "catalog_task_active" => (
                StatusCode::CONFLICT,
                "创作任务正在执行，请结束后再修改目录。",
            ),
            "catalog_source_changed" => {
                (StatusCode::CONFLICT, "卷纲来源已变化，请重新选择准确版本。")
            }
            "catalog_volume_not_found" => (StatusCode::BAD_REQUEST, "当前作品中找不到这卷。"),
            "catalog_chapter_not_found" => (StatusCode::BAD_REQUEST, "当前作品中找不到这章。"),
            "catalog_volume_limit" => (StatusCode::BAD_REQUEST, "本作品已达到100卷目录上限。"),
            "catalog_node_already_linked" => (
                StatusCode::CONFLICT,
                "这份卷纲已关联另一作品卷，请核对目录。",
            ),
            "invalid_volume_source" => (StatusCode::BAD_REQUEST, "所选来源不是唯一有效卷纲。"),
            "invalid_volume_text" => (StatusCode::BAD_REQUEST, "卷名须为1至160字。"),
            "invalid_catalog_key" | "invalid_catalog_chapter" | "invalid_catalog_volume" => {
                (StatusCode::BAD_REQUEST, "目录请求标识无效。")
            }
            _ => return repository_error(source),
        };
        return error(status, code, message);
    }
    repository_error(source)
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/catalog",
        axum::routing::get(crate::catalog_api::read).post(crate::catalog_api::update),
    )
}

use novel_codex_story_core::{ManuscriptCatalogRepository};
