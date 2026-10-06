use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    DeleteChapterRequest, ProjectId, RepositoryError, SqliteStoryRepository,
};

use crate::server::{RuntimeState, error, repository_error, unix_now};

pub(crate) async fn delete(
    State(state): State<RuntimeState>,
    Path((project, chapter)): Path<(String, String)>,
    Json(request): Json<DeleteChapterRequest>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(project) => project,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_project_id",
                "作品标识无效。",
            );
        }
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(repository) => repository,
        Err(source) => return repository_error(source),
    };
    match repository.delete_chapter(&project, &chapter, &request, unix_now()) {
        Ok(receipt) => Json(receipt).into_response(),
        Err(source) => {
            if let RepositoryError::InvalidContent(code) = &source {
                let (status, message) = match code.as_str() {
                    "invalid_chapter_delete" => (StatusCode::BAD_REQUEST, "删除章节请求无效。"),
                    "chapter_delete_not_found" | "chapter_deleted" => (
                        StatusCode::CONFLICT,
                        "当前作品中已不存在这章，请重新读取目录。",
                    ),
                    "catalog_task_active" => (
                        StatusCode::CONFLICT,
                        "创作任务正在执行，请结束后再删除章节。",
                    ),
                    _ => return repository_error(source),
                };
                return error(status, code, message);
            }
            if matches!(source, RepositoryError::ObjectLocked) {
                return error(
                    StatusCode::CONFLICT,
                    "chapter_delete_locked",
                    "这章或场景已受保护，请先解除保护再删除。",
                );
            }
            repository_error(source)
        }
    }
}
