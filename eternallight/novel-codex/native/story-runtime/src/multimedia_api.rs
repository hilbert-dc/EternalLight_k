use crate::server::{Actor, RuntimeState, candidate_task, error, repository_error};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use novel_codex_story_core::{
    BeginMediaImport, CreateCreativeProjectRequest, CreateEditionRequest, CreateMediaBranch,
    MediaSourceRef, MediaTextField, MediaWorkspacePageRequest, PatchMultimediaDocument, ProjectId,
    ReadMediaText, RegisterMediaAsset, RepositoryError, SaveMultimediaDocument,
    SqliteStoryRepository,
};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn routes() -> Router<RuntimeState> {
    Router::new()
        .merge(super::multimedia_operations_api::author_routes())
        .merge(super::multimedia_context_api::routes())
        .route(
            "/api/projects/{project_id}/media-assets/{revision_id}",
            get(read_asset),
        )
        .route(
            "/api/projects/{project_id}/creative-project",
            get(read_project),
        )
        .route(
            "/api/projects/{project_id}/editions",
            get(list_editions).post(create_edition),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/workspace",
            get(read_workspace).post(save_workspace),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/patch",
            post(patch_workspace),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/text-page",
            post(read_text_page),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/branches",
            post(create_branch),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/imports",
            post(begin_import),
        )
        .route(
            "/api/projects/{project_id}/editions/{edition_id}/assets",
            post(register_asset),
        )
}
pub(crate) fn writer_routes() -> Router<RuntimeState> {
    Router::new()
        .merge(super::multimedia_operations_api::writer_routes())
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/media/workspace",
            post(task_workspace),
        )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MediaTaskRequest {
    thread_id: String,
    expected_revision: i64,
    action: MediaTaskAction,
    request: Option<MediaTaskMutation>,
    source: Option<MediaSourceRef>,
    version: Option<String>,
    section: Option<String>,
    page_id: Option<String>,
    panel_id: Option<String>,
    track_id: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    text_field: Option<MediaTextField>,
    char_offset: Option<usize>,
    char_limit: Option<usize>,
    entry_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum MediaTaskAction {
    Source,
    Read,
    Save,
    Patch,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum MediaTaskMutation {
    Save(SaveMultimediaDocument),
    Patch(PatchMultimediaDocument),
}
async fn task_workspace(
    State(s): State<RuntimeState>,
    Path((p, t)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(r): Json<MediaTaskRequest>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let task = match candidate_task(
        &db,
        actor,
        &headers,
        &p,
        Some(&t),
        Some(&r.thread_id),
        Some(r.expected_revision),
    ) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return error(
                StatusCode::FORBIDDEN,
                "media_task_capability_required",
                "媒体任务操作需要对应任务能力。",
            );
        }
        Err(response) => return *response,
    };
    let version = r.version.as_deref().unwrap_or("source");
    if !matches!(version, "source" | "working") {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_media_version",
            "请选择任务固定来源或本任务的工作版本。",
        );
    }
    let page = MediaWorkspacePageRequest {
        section: r.section,
        page_id: r.page_id,
        panel_id: r.panel_id,
        track_id: r.track_id,
        offset: r.offset,
        limit: r.limit,
        text_field: r.text_field,
        char_offset: r.char_offset,
        char_limit: r.char_limit,
        entry_id: r.entry_id,
    };
    if let Err(e) = page.validate_section_parameters() {
        return failure(e);
    }
    let result = match (r.action, r.request) {
        (MediaTaskAction::Read, None) => {
            if page.section.as_deref() == Some("canonSources") {
                return match db.media_task_canon_sources(&task, &page) {
                    Ok(v) => Json(v).into_response(),
                    Err(e) => failure(e),
                };
            }
            if matches!(page.text_field, Some(MediaTextField::CanonStatement { .. })) {
                return match db.media_task_canon_text(&task, &page) {
                    Ok(v) => Json(v).into_response(),
                    Err(e) => failure(e),
                };
            }
            if page.section.as_deref() == Some("canon") {
                return match db.media_task_canon(
                    &task,
                    page.offset.unwrap_or(0),
                    page.limit.unwrap_or(20),
                ) {
                    Ok(v) => Json(v).into_response(),
                    Err(e) => failure(e),
                };
            }
            let workspace = if version == "working" {
                db.media_task_working_workspace(&task)
            } else {
                db.media_task_workspace(&task)
            };
            workspace.and_then(|w| w.page(&page)).map(|mut v| {
                v["version"] = json!(version);
                v
            })
        }
        (MediaTaskAction::Source, None) => match r.source {
            Some(source) => db.read_task_media_source(&task, &source, &page),
            None => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "media_source_required",
                    "读取来源需要已授权的固定版本引用。",
                );
            }
        },
        (MediaTaskAction::Save, Some(MediaTaskMutation::Save(request))) => {
            if serde_json::to_vec(&request)
                .map(|v| v.len())
                .unwrap_or(usize::MAX)
                > 1_490_000
            {
                return error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "media_save_too_large",
                    "完整文档超过本次工具保存上限，请缩小编辑范围。",
                );
            }
            db.save_task_multimedia_document(&task, &request)
                .and_then(|w| w.page(&MediaWorkspacePageRequest::default()))
        }
        (MediaTaskAction::Patch, Some(MediaTaskMutation::Patch(request))) => {
            if serde_json::to_vec(&request)
                .map(|v| v.len())
                .unwrap_or(usize::MAX)
                > 256 * 1024
            {
                return error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "media_patch_too_large",
                    "局部修改超过本次操作上限，请分批调整。",
                );
            }
            db.patch_task_multimedia_document(&task, &request)
                .and_then(|w| w.page(&MediaWorkspacePageRequest::default()))
        }
        _ => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_media_task_request",
                "读取和保存参数不符。",
            );
        }
    };
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
pub(crate) fn guard_novel_route(state: &RuntimeState, path: &str) -> Result<(), Response> {
    let segments = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    if segments.len() < 4 || segments[0] != "api" || segments[1] != "projects" {
        return Ok(());
    }
    if matches!(
        segments[3],
        "creative-project"
            | "editions"
            | "conversations"
            | "tasks"
            | "work-tasks"
            | "context"
            | "media-operations"
            | "media-assets"
            | "canon"
    ) {
        return Ok(());
    }
    let (db, p) = open(state, segments[2].to_owned())?;
    db.require_novel_project(&p).map_err(failure)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BranchQuery {
    pub branch_id: Option<String>,
    pub revision_id: Option<String>,
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
pub(crate) fn open(
    state: &RuntimeState,
    p: String,
) -> Result<(SqliteStoryRepository, ProjectId), Response> {
    let p = ProjectId::parse(p).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_project_id",
            "作品标识无效。",
        )
    })?;
    let db = SqliteStoryRepository::open(&state.database_path).map_err(failure)?;
    Ok((db, p))
}
pub(crate) fn failure(e: RepositoryError) -> Response {
    match &e {
        RepositoryError::CrossProjectReference => error(
            StatusCode::FORBIDDEN,
            "media_scope_mismatch",
            "来源不属于当前作品。",
        ),
        RepositoryError::InvalidContent(code)
            if code.ends_with("_mismatch") || code == "novel_scope_required" =>
        {
            error(
                StatusCode::FORBIDDEN,
                code,
                "操作与当前作品或媒介范围不符。",
            )
        }
        RepositoryError::InvalidContent(code)
            if code == "media_revision_changed"
                || code == "media_source_changed"
                || code == "media_text_digest_changed"
                || code == "media_text_slice_changed"
                || code == "media_import_not_active" =>
        {
            error(
                StatusCode::CONFLICT,
                code,
                "内容或操作状态已变化，请重新读取后继续。",
            )
        }
        RepositoryError::InvalidContent(code) if code.ends_with("_not_found") => {
            error(StatusCode::NOT_FOUND, code, "当前范围内找不到请求的对象。")
        }
        _ => repository_error(e),
    }
}
pub(crate) async fn create_project(
    State(s): State<RuntimeState>,
    Json(r): Json<CreateCreativeProjectRequest>,
) -> Response {
    let mut db = match SqliteStoryRepository::open(&s.database_path) {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    match db.create_creative_project(&r, now()) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => failure(e),
    }
}
async fn read_project(State(s): State<RuntimeState>, Path(p): Path<String>) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.creative_project(&p) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn list_editions(State(s): State<RuntimeState>, Path(p): Path<String>) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.creative_editions(&p) {
        Ok(v) => Json(json!({"editions":v})).into_response(),
        Err(e) => failure(e),
    }
}
async fn create_edition(
    State(s): State<RuntimeState>,
    Path(p): Path<String>,
    Json(r): Json<CreateEditionRequest>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.create_creative_edition(&p, &r, now()) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => failure(e),
    }
}
async fn read_workspace(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Query(q): Query<BranchQuery>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let result = match q.revision_id {
        Some(revision) => {
            db.multimedia_revision_workspace(&p, &e, q.branch_id.as_deref(), &revision)
        }
        None => db.multimedia_workspace(&p, &e, q.branch_id.as_deref()),
    };
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn save_workspace(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Query(q): Query<BranchQuery>,
    Json(r): Json<SaveMultimediaDocument>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.save_multimedia_document(&p, &e, q.branch_id.as_deref(), &r) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn patch_workspace(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Query(q): Query<BranchQuery>,
    Json(r): Json<PatchMultimediaDocument>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.patch_multimedia_document(&p, &e, q.branch_id.as_deref(), &r) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn read_text_page(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Query(q): Query<BranchQuery>,
    Json(r): Json<ReadMediaText>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let request = MediaWorkspacePageRequest {
        section: Some("text".into()),
        text_field: Some(r.field),
        char_offset: r.char_offset,
        char_limit: r.char_limit,
        ..Default::default()
    };
    match db
        .multimedia_revision_workspace(&p, &e, q.branch_id.as_deref(), &r.revision_id)
        .and_then(|w| w.page(&request))
    {
        Ok(mut v) => {
            v["version"] = json!("source");
            Json(v).into_response()
        }
        Err(e) => failure(e),
    }
}
async fn create_branch(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Json(r): Json<CreateMediaBranch>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.create_media_branch(&p, &e, &r) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => failure(e),
    }
}
async fn begin_import(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Json(r): Json<BeginMediaImport>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.begin_media_import(&p, &e, &r, now()) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => failure(e),
    }
}
async fn register_asset(
    State(s): State<RuntimeState>,
    Path((p, e)): Path<(String, String)>,
    Json(r): Json<RegisterMediaAsset>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.register_media_asset(&p, &e, &r, now()) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => failure(e),
    }
}

async fn read_asset(
    State(s): State<RuntimeState>,
    Path((p, r)): Path<(String, String)>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.media_asset_revision(&p, &r) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
