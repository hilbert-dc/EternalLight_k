use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BranchId, CommitId, IdempotencyKey, NarrativeCommitment, ObjectId, ProjectId, RevisionId,
    SaveCandidateRequest, SnapshotId, TaskOutputBinding,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "CommitmentRevisionApiBinding", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "CommitmentRevisionApiSubmit", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Submit {
    task_id: String,
    thread_id: String,
    expected_revision: i64,
    expected_candidate_revision_id: Option<String>,
    idempotency_key: String,
    payload: NarrativeCommitment,
}

pub(crate) async fn read(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Query(binding): Query<Binding>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
        let project = match ProjectId::parse(project) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match candidate_task(
            &repo,
            actor,
            &headers,
            &project,
            Some(&binding.task_id),
            Some(&binding.thread_id),
            Some(binding.expected_revision),
        ) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "candidate_task_required",
                    "A bound task is required",
                );
            }
            Err(response) => return *response,
        }
        match repo.task_commitment(&binding.task_id, &id) {
            Ok(mut value) => {
                value
                    .as_object_mut()
                    .expect("commitment projection")
                    .remove("extractionOrigin");
                Json(value).into_response()
            }
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn submit(
    State(state): State<RuntimeState>,
    Path((project, id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<Submit>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repo| {
    let project = match ProjectId::parse(project) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };

    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&request.task_id),
        Some(&request.thread_id),
        Some(request.expected_revision),
    ) {
        Ok(Some(task)) if task.operation == "reviseCommitment" => task,
        Ok(_) => {
            return error(
                StatusCode::FORBIDDEN,
                "candidate_task_required",
                "A bound reviseCommitment task is required",
            );
        }
        Err(response) => return *response,
    };
    if request.payload.id != id {
        return bad_request("invalid_commitment_id", "Payload must match path");
    }
    let origin = match repo.commitment_revision_origin(&task.task_id, &id) {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
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
    let object = match ObjectId::parse(format!("commitment-{id}")) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_commitment_id", source),
    };
    if let Err(source) = crate::recovery_binding::bind(
        &mut repo,
        &headers,
        Some(&task),
        "submit_commitment_candidate",
        Some(key.as_str()),
    ) {
        return repository_error(source);
    }
    let content = json!({"payload":request.payload,"extractionOrigin":origin}).to_string();
    if content.len() > 32_000 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "commitment_too_large",
            "Commitment exceeds 32 KB",
        );
    }
    let branch = BranchId::parse("main").expect("static branch");
    let head = match repo.branch_head(&project, &branch) {
        Ok(Some(value)) => value,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found", "Project not found"),
        Err(source) => return repository_error(source),
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let digest = |value: &str| format!("{:x}", Sha256::digest(value.as_bytes()));
    let request_digest=digest(&json!({"taskId":task.task_id,"objectId":object.as_str(),"content":content,"expectedRevision":request.expected_revision,"expectedCandidateRevisionId":previous.as_ref().map(RevisionId::as_str)}).to_string());
    match repo.save_candidate(SaveCandidateRequest {
        project_id:project,branch_id:branch,object_id:object,object_kind:"commitment".into(),
        revision_id:RevisionId::parse(format!("commitment-revision-{suffix}")).expect("generated revision"),commit_id:CommitId::parse(format!("candidate-{suffix}")).expect("generated commit"),
        idempotency_key:key,request_digest,expected_project_revision:request.expected_revision,expected_snapshot_id:head.snapshot_id,next_snapshot_id:SnapshotId::parse(format!("snapshot-{suffix}")).expect("generated snapshot"),
        expected_candidate_revision_id:previous,content_digest:digest(&content),content_json:content,chapter_projection:None,task_output:Some(TaskOutputBinding {task_id:task.task_id,thread_id:task.thread_id,output_kind:"commitment".into(),completeness:None}),linked_impact_ids:Vec::new(),
    }) {
        Ok(receipt)=>(StatusCode::CREATED,Json(json!({"revisionId":receipt.revision_id.as_str(),"snapshotId":receipt.result_snapshot_id.as_str(),"confirmationState":"candidate"}))).into_response(),
        Err(source)=>repository_error(source),
    }

    }).await
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/commitments/{commitment_id}/task-input",
            axum::routing::get(crate::commitment_revision_api::read),
        )
        .route(
            "/api/projects/{project_id}/commitments/{commitment_id}/task-candidate",
            axum::routing::post(crate::commitment_revision_api::submit),
        )
}

use novel_codex_story_core::{CommitmentRevisionRepository, ProjectRevisionRepository};
