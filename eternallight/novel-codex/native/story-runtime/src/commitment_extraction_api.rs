use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BranchId, CommitId, CommitmentExtractionOrigin, EstablishCommitment, IdempotencyKey, ObjectId,
    ProjectId, RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId,
    SqliteStoryRepository, TaskOutputBinding,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SubmitRequest {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    expected_candidate_revision_id: Option<String>,
    idempotency_key: String,
    payload: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EstablishRequest {
    report_revision_id: String,
    proposal_id: String,
    base_snapshot_id: String,
    expected_revision: i64,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadQuery {
    report_revision_id: Option<String>,
}

pub(crate) async fn establish(
    State(state): State<RuntimeState>,
    Path((project, chapter)): Path<(String, String)>,
    Json(request): Json<EstablishRequest>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let snapshot = match SnapshotId::parse(request.base_snapshot_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_snapshot_id", source),
    };
    let key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_idempotency_key", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    match repository.establish_commitment(EstablishCommitment {
        project_id:project, origin:CommitmentExtractionOrigin {chapter_id:chapter, report_revision_id:request.report_revision_id, proposal_id:request.proposal_id},
        expected_project_revision:request.expected_revision,base_snapshot_id:snapshot,idempotency_key:key,
    }) {
        Ok(value) => (StatusCode::CREATED, Json(json!({"commitmentId":value.commitment_id,
            "revisionId":value.receipt.revision_id.as_str(),"snapshotId":value.receipt.result_snapshot_id.as_str(),"confirmationState":"candidate"}))).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn get(
    State(state): State<RuntimeState>,
    Path((project, chapter)): Path<(String, String)>,
    Query(query): Query<ReadQuery>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let report_revision = match query.report_revision_id.map(RevisionId::parse).transpose() {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_report_revision_id", source),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    match repository.commitment_extraction(&project, &chapter, report_revision.as_ref()) {
        Ok(Some(value)) => Json(value).into_response(),
        Ok(None) => Json(Value::Null).into_response(),
        Err(source) => repository_error(source),
    }
}

pub(crate) async fn submit(
    State(state): State<RuntimeState>,
    Path((project, chapter)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SubmitRequest>,
) -> Response {
    if actor != Actor::Candidate {
        return error(
            StatusCode::FORBIDDEN,
            "candidate_task_required",
            "A bound extraction task is required",
        );
    }
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let object = match ObjectId::parse(format!("commitment-extraction-{chapter}")) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_chapter_id", source),
    };
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let task = match candidate_task(
        &repository,
        actor,
        &headers,
        &project,
        Some(&request.task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) if task.operation == "extractCommitments" => task,
        Ok(_) => return bad_request("invalid_task_operation", "Expected extractCommitments"),
        Err(response) => return *response,
    };
    if request.payload["chapterId"] != chapter {
        return bad_request("invalid_chapter_id", "Payload does not match chapter route");
    }
    let previous = match request
        .expected_candidate_revision_id
        .map(RevisionId::parse)
        .transpose()
    {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_revision_id", source),
    };
    let key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_idempotency_key", source),
    };
    let content = json!({"payload":request.payload}).to_string();
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&task),
        "submit_commitment_extraction",
        Some(key.as_str()),
    ) {
        return repository_error(source);
    }
    if content.len() > 40_000 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "extraction_too_large",
            "Extraction exceeds 40 KB",
        );
    }
    let branch = BranchId::parse("main").expect("static branch");
    let head = match repository.branch_head(&project, &branch) {
        Ok(Some(value)) => value,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found", "Project not found"),
        Err(source) => return repository_error(source),
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let digest = |value: &str| format!("{:x}", Sha256::digest(value.as_bytes()));
    let request_digest = digest(&json!({"taskId":task.task_id,"payload":request.payload,"expectedRevision":request.expected_revision,
        "expectedCandidateRevisionId":previous.as_ref().map(RevisionId::as_str)}).to_string());
    match repository.save_candidate(SaveCandidateRequest {
        project_id: project,
        branch_id: branch,
        object_id: object,
        object_kind: "commitmentExtraction".into(),
        revision_id: RevisionId::parse(format!("extraction-{suffix}")).expect("generated revision"),
        commit_id: CommitId::parse(format!("candidate-{suffix}")).expect("generated commit"),
        idempotency_key: key,
        request_digest,
        expected_project_revision: request.expected_revision,
        expected_snapshot_id: head.snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot"),
        expected_candidate_revision_id: previous,
        content_digest: digest(&content),
        content_json: content,
        chapter_projection: None,
        task_output: Some(TaskOutputBinding {
            task_id: task.task_id,
            thread_id: task.thread_id,
            output_kind: "commitmentExtraction".into(),
            completeness: None,
        }),
        linked_impact_ids: Vec::new(),
    }) {
        Ok(receipt) => {
            let saved = match repository.revision_content(&receipt.revision_id) {
                Ok(Some(content)) => serde_json::from_str::<Value>(&content).ok(),
                Ok(None) => None,
                Err(source) => return repository_error(source),
            };
            let Some(candidates) = saved
                .as_ref()
                .and_then(|report| report["payload"]["candidates"].as_array())
            else {
                return repository_error(RepositoryError::CorruptState);
            };
            (StatusCode::CREATED, Json(json!({"revisionId":receipt.revision_id.as_str(),
                "chapterId":chapter,"status":"candidateProposalBatch","candidateCount":candidates.len()}))).into_response()
        }
        Err(source) => repository_error(source),
    }
}
