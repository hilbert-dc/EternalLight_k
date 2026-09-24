use axum::Json;
use axum::extract::Extension;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::CreatePlanningReview;
use novel_codex_story_core::CreateTaskRequest;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::SavePlanningReviewReport;
use novel_codex_story_core::SqliteStoryRepository;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use uuid::Uuid;

use crate::RuntimeState;
use crate::server::Actor;
use crate::server::bad_request;
use crate::server::candidate_task;
use crate::server::internal_error;
use crate::server::repository_error;
use crate::server::task_json;
use crate::server::unix_now;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartRequest {
    artifact_kind: String,
    revision_id: String,
    target_node_id: Option<String>,
    author_intent: String,
    roles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeatTaskRequest {
    thread_id: String,
    model: Option<String>,
    capability_hash: String,
    capability_expires_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportRequest {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    report: Value,
}

pub(crate) async fn start(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Json(request): Json<StartRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    if request.author_intent.trim().is_empty() || request.author_intent.chars().count() > 4_000 {
        return bad_request("invalid_planning_review", "author intent is invalid");
    }
    let packet_identity = json!({
        "artifactKind": request.artifact_kind,
        "revisionId": request.revision_id,
        "targetNodeId": request.target_node_id,
        "authorIntent": request.author_intent
    });
    let source_packet_hash = format!(
        "{:x}",
        sha2::Sha256::digest(packet_identity.to_string().as_bytes())
    );
    let run_id = format!("planning-review-{}", Uuid::new_v4().simple());
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.create_planning_review(CreatePlanningReview {
        run_id: &run_id,
        project_id: &project_id,
        artifact_kind: &request.artifact_kind,
        revision_id: &request.revision_id,
        target_node_id: request.target_node_id.as_deref(),
        author_intent: &request.author_intent,
        roles: &request.roles,
        source_packet_hash: &source_packet_hash,
        now: unix_now(),
    }) {
        Ok(run) => Json(run).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn create_seat_task(
    State(state): State<RuntimeState>,
    Path((project_id, run_id, role)): Path<(String, String, String)>,
    Json(request): Json<SeatTaskRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let run = match repository.planning_reviews(&project_id) {
        Ok(runs) => runs.into_iter().find(|run| run["id"] == run_id),
        Err(error) => return repository_error(error),
    };
    let Some(run) = run else {
        return repository_error(novel_codex_story_core::RepositoryError::ReviewNotFound);
    };
    let task_id = format!("task-{}", Uuid::new_v4().simple());
    let target = json!({
        "type": "artifact", "artifactKind": run["artifactKind"], "revisionId": run["revisionId"],
        "reviewRunId": run_id, "role": role
    });
    let manifest = json!({
        "operation": "reviewPlanningSeat", "projectId": project_id.as_str(),
        "projectRevision": run["inputRevision"], "target": target,
        "authorIntent": run["authorIntent"], "sourcePacketHash": run["sourcePacketHash"]
    });
    match repository.create_planning_review_seat_task(
        &project_id,
        &run_id,
        &role,
        CreateTaskRequest {
            task_id,
            project_id: project_id.clone(),
            thread_id: request.thread_id,
            operation: "reviewPlanningSeat".into(),
            input_revision: run["inputRevision"].as_i64().unwrap_or(-1),
            model: request.model,
            target_json: target.to_string(),
            input_manifest_json: manifest.to_string(),
            capability_hash: request.capability_hash,
            capability_expires_at: request.capability_expires_at,
        },
    ) {
        Ok(task) => Json(task_json(&task)).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn submit_report(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<ReportRequest>,
) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if let Err(response) = candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        Some(&request.task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        return *response;
    }
    match repository.save_planning_review_report(SavePlanningReviewReport {
        project_id: &project_id,
        run_id: &run_id,
        task_id: &request.task_id,
        thread_id: &request.thread_id,
        expected_revision: request.expected_revision,
        report: &request.report,
        now: unix_now(),
    }) {
        Ok(report) => Json(report).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn finalize(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    finish(state, project_id, run_id, false)
}

pub(crate) async fn cancel(
    State(state): State<RuntimeState>,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Response {
    finish(state, project_id, run_id, true)
}

fn finish(state: RuntimeState, project_id: String, run_id: String, cancel: bool) -> Response {
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.finish_planning_review(&project_id, &run_id, cancel, unix_now()) {
        Ok(run) => Json(run).into_response(),
        Err(error) => repository_error(error),
    }
}
