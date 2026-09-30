use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BranchId, CommitId, CommitmentExtractionOrigin, CreateReviewRequest, IdempotencyKey,
    NarrativeCommitment, ObjectId, ProjectId, RevisionId, SaveCandidateRequest, SnapshotId,
    SqliteStoryRepository,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::RuntimeState;
use crate::server::{bad_request, internal_error, repository_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveRequest {
    payload: NarrativeCommitment,
    extraction_origin: Option<CommitmentExtractionOrigin>,
    expected_revision: i64,
    base_snapshot_id: String,
    expected_candidate_revision_id: Option<String>,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadQuery {
    snapshot_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

pub(crate) async fn list(
    State(state): State<RuntimeState>,
    Path(project): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.narrative_commitments_page(
        &project,
        query.cursor.as_deref(),
        query.limit.unwrap_or(20),
    ) {
        Ok(value) => Json(value).into_response(),
        Err(error) => repository_error(error),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewRequest {
    revision_id: String,
    idempotency_key: String,
}

pub(crate) async fn save(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Json(request): Json<SaveRequest>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let object = match ObjectId::parse(format!("commitment-{id}")) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_commitment_id", error),
    };
    if request.payload.id != id {
        return bad_request("invalid_commitment_id", "payload must match path");
    }
    let snapshot = match SnapshotId::parse(request.base_snapshot_id.clone()) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_snapshot_id", error),
    };
    let previous = match request
        .expected_candidate_revision_id
        .clone()
        .map(RevisionId::parse)
        .transpose()
    {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_revision_id", error),
    };
    let key = match IdempotencyKey::parse(request.idempotency_key.clone()) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_idempotency_key", error),
    };
    let content =
        json!({"payload":request.payload,"extractionOrigin":request.extraction_origin}).to_string();
    let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
    let request_digest = format!("{:x}",Sha256::digest(json!({"objectId":object.as_str(),"content":content,"base":request.base_snapshot_id,"revision":request.expected_revision,"candidate":request.expected_candidate_revision_id}).to_string().as_bytes()));
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let token = Uuid::new_v4().simple().to_string();
    match repository.save_candidate(SaveCandidateRequest {
        project_id:project,branch_id:BranchId::parse("main").expect("static branch"),object_id:object,object_kind:"commitment".into(),
        revision_id:RevisionId::parse(format!("commitment-revision-{token}")).expect("generated id"),
        commit_id:CommitId::parse(format!("commitment-save-{token}")).expect("generated id"),
        idempotency_key:key,request_digest,expected_project_revision:request.expected_revision,
        expected_snapshot_id:snapshot,next_snapshot_id:SnapshotId::parse(format!("commitment-snapshot-{token}")).expect("generated id"),
        expected_candidate_revision_id:previous,content_digest:digest,content_json:content,chapter_projection:None,task_output:None,linked_impact_ids:Vec::new(),
    }) {
        Ok(receipt)=>(StatusCode::CREATED,Json(json!({"revisionId":receipt.revision_id.as_str(),"snapshotId":receipt.result_snapshot_id.as_str()}))).into_response(),
        Err(error)=>repository_error(error),
    }
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Query(query): Query<ReadQuery>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let snapshot = match query.snapshot_id.map(SnapshotId::parse).transpose() {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_snapshot_id", error),
    };
    let repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.narrative_commitment(&project, &id, snapshot.as_ref()) {
        Ok(value) => Json(value).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn review(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Json(request): Json<ReviewRequest>,
) -> Response {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_project_id", error),
    };
    let object = match ObjectId::parse(format!("commitment-{id}")) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_commitment_id", error),
    };
    let revision = match RevisionId::parse(request.revision_id) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_revision_id", error),
    };
    let key = match IdempotencyKey::parse(request.idempotency_key) {
        Ok(value) => value,
        Err(error) => return bad_request("invalid_idempotency_key", error),
    };
    let request_digest = format!(
        "{:x}",
        Sha256::digest(
            json!({"objectId":object.as_str(),"revisionId":revision.as_str()})
                .to_string()
                .as_bytes()
        )
    );
    let mut repository = match SqliteStoryRepository::open(&state.database_path) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    match repository.create_review(CreateReviewRequest {
        review_id:format!("review-{}",Uuid::new_v4().simple()),project_id:project,object_id:object,object_kind:"commitment".into(),revision_id:revision,
        idempotency_key:key,request_digest,checks_json:"[]".into(),proposals_json:"[]".into(),modeling_batch_id:None,selections_json:"{}".into(),
    }) {
        Ok(review)=>(StatusCode::CREATED,Json(json!({"id":review.review_id,"artifactKind":review.object_kind,"artifactRevisionId":review.revision_id.as_str(),"projectRevision":review.project_revision,"status":review.status}))).into_response(),
        Err(error)=>repository_error(error),
    }
}
