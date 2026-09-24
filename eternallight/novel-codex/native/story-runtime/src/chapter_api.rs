use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::AssociateChapterPlanRequest;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::SqliteStoryRepository;
use serde::Deserialize;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

use crate::server::RuntimeState;
use crate::server::error;
use crate::server::repository_error;
use crate::server::unix_now;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaterializePlanRequest {
    chapter_plan_id: String,
    expected_revision: i64,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaterializeStandaloneRequest {
    #[serde(default)]
    title: String,
    expected_revision: i64,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssociatePlanRequest {
    chapter_id: String,
    chapter_plan_id: String,
    expected_revision: i64,
    idempotency_key: String,
}

pub(crate) async fn materialize_from_plan(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<MaterializePlanRequest>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let digest = digest(
        &json!({"chapterPlanId": request.chapter_plan_id, "expectedRevision": request.expected_revision}),
    );
    match repository.materialize_chapter_from_plan(&project_id, &request.chapter_plan_id, request.expected_revision, &request.idempotency_key, &digest, unix_now()) {
        Ok(value) => Json(json!({
            "commitId": format!("materialize-{}", value.chapter_id), "operation": "materialize-chapter",
            "projectId": project_id.as_str(), "chapterPlanId": value.chapter_plan_id,
            "chapterId": value.chapter_id, "baseRevision": request.expected_revision,
            "resultRevision": value.result_revision
        })).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn materialize_standalone(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<MaterializeStandaloneRequest>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let digest =
        digest(&json!({"title": request.title, "expectedRevision": request.expected_revision}));
    match repository.materialize_standalone_chapter(&project_id, &request.title, request.expected_revision, &request.idempotency_key, &digest, unix_now()) {
        Ok(value) => Json(json!({
            "commitId": format!("materialize-{}", value.chapter_id), "operation": "materialize-standalone",
            "projectId": project_id.as_str(), "chapterId": value.chapter_id,
            "chapterNumber": value.chapter_number, "baseRevision": request.expected_revision,
            "resultRevision": value.result_revision
        })).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn associate_plan(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<AssociatePlanRequest>,
) -> Response {
    let (mut repository, project_id) = match open(&state, project_id) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let digest = digest(
        &json!({"chapterId": request.chapter_id, "chapterPlanId": request.chapter_plan_id, "expectedRevision": request.expected_revision}),
    );
    match repository.associate_chapter_plan(AssociateChapterPlanRequest {
        project_id: &project_id,
        chapter_id: &request.chapter_id,
        chapter_plan_id: &request.chapter_plan_id,
        expected_revision: request.expected_revision,
        idempotency_key: &request.idempotency_key,
        request_digest: &digest,
        now: unix_now(),
    }) {
        Ok(value) => Json(json!({
            "commitId": format!("associate-{}", value.chapter_id), "operation": "associate-plan",
            "projectId": project_id.as_str(), "chapterPlanId": value.chapter_plan_id,
            "chapterId": value.chapter_id, "baseRevision": request.expected_revision,
            "resultRevision": value.result_revision
        }))
        .into_response(),
        Err(source) => repository_error(source),
    }
}

fn open(
    state: &RuntimeState,
    project_id: String,
) -> Result<(SqliteStoryRepository, ProjectId), Box<Response>> {
    let project_id = ProjectId::parse(project_id).map_err(|source| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            "invalid_project_id",
            &source.to_string(),
        ))
    })?;
    let repository = SqliteStoryRepository::open(&state.database_path)
        .map_err(|source| Box::new(repository_error(source)))?;
    Ok((repository, project_id))
}

fn digest(value: &serde_json::Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
