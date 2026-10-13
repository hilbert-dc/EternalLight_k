use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BranchId, CommitId, IdempotencyKey, ObjectId, ProjectId, RevisionId, SaveCandidateRequest,
    SnapshotId, TaskOutputBinding,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "SceneAssessmentApiSubmitRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmitRequest {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    expected_candidate_revision_id: Option<String>,
    idempotency_key: String,
    payload: serde_json::Value,
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project_id, chapter_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        let assessment = match repository.scene_assessment(&project_id, &chapter_id) {
            Ok(Some(value)) => value,
            Ok(None) => return error(StatusCode::NOT_FOUND, "not_found", "No scene assessment"),
            Err(source) => return repository_error(source),
        };
        Json(json!({
            "revisionId": assessment.revision_id,
            "authority": "candidateAnalysis",
            "state": if assessment.current { "current" } else { "stale" },
            "assessment": assessment.content
        }))
        .into_response()
    })
    .await
}

pub(crate) async fn submit(
    State(state): State<RuntimeState>,
    Path((project_id, chapter_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SubmitRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
    if actor != Actor::Candidate {
        return error(
            StatusCode::FORBIDDEN,
            "candidate_task_required",
            "A task-bound candidate is required for scene assessment",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let object_id = match ObjectId::parse(format!("scene-assessment-{chapter_id}")) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_chapter_id", source),
    };

    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project_id,
        Some(&request.task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) if task.operation == "assessSceneOutcome" => task,
        Ok(_) => return bad_request("invalid_task_operation", "Expected assessSceneOutcome"),
        Err(response) => return *response,
    };
    if request.payload["chapterId"] != chapter_id {
        return bad_request("invalid_chapter_id", "Payload chapter does not match route");
    }
    let expected_candidate_revision_id = match request
        .expected_candidate_revision_id
        .map(RevisionId::parse)
        .transpose()
    {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_revision_id", source),
    };
    let idempotency_key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_idempotency_key", source),
    };
    let content_json = request.payload.to_string();
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&task),
        "submit_scene_assessment",
        Some(idempotency_key.as_str()),
    ) {
        return repository_error(source);
    }
    if content_json.len() > 48 * 1024 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "assessment_too_large",
            "Scene assessment exceeds 48 KiB",
        );
    }
    let branch_id = BranchId::parse("main").expect("static branch id");
    let head = match repository.branch_head(&project_id, &branch_id) {
        Ok(Some(value)) => value,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found", "Project not found"),
        Err(source) => return repository_error(source),
    };
    let digest =
        |value: &serde_json::Value| format!("{:x}", Sha256::digest(value.to_string().as_bytes()));
    let request_digest = digest(&json!({
        "payload": request.payload,
        "expectedRevision": request.expected_revision,
        "expectedCandidateRevisionId": expected_candidate_revision_id.as_ref().map(RevisionId::as_str)
    }));
    let suffix = Uuid::new_v4().simple().to_string();
    let result = repository.save_candidate(SaveCandidateRequest {
        project_id,
        branch_id,
        object_id,
        object_kind: "sceneAssessment".into(),
        revision_id: RevisionId::parse(format!("scene-assessment-{suffix}"))
            .expect("generated revision id"),
        commit_id: CommitId::parse(format!("candidate-{suffix}")).expect("generated commit id"),
        idempotency_key,
        request_digest,
        expected_project_revision: request.expected_revision,
        expected_snapshot_id: head.snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot id"),
        expected_candidate_revision_id,
        content_digest: digest(&request.payload),
        content_json,
        chapter_projection: None,
        task_output: Some(TaskOutputBinding {
            task_id: task.task_id,
            thread_id: task.thread_id,
            output_kind: "sceneAssessment".into(),
            completeness: None,
        }),
        linked_impact_ids: Vec::new(),
    });
    match result {
        Ok(receipt) => (
            StatusCode::CREATED,
            Json(json!({
                "revisionId": receipt.revision_id.as_str(),
                "chapterId": chapter_id,
                "status": "candidate"
            })),
        )
            .into_response(),
        Err(source) => repository_error(source),
    }

    }).await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments",
        axum::routing::get(crate::scene_assessment_api::get),
    )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments",
        axum::routing::post(crate::scene_assessment_api::submit),
    )
}

use novel_codex_story_core::{ProjectRevisionRepository, SceneAssessmentRepository};
