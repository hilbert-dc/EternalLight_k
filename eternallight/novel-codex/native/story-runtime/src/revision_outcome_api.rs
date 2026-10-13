use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use novel_codex_story_core::{
    BranchId, CommitId, IdempotencyKey, ObjectId, ProjectId, RepositoryError, RevisionId,
    RevisionOutcomeDecisionRequest, SaveCandidateRequest, SnapshotId,
    TaskOutputBinding,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "RevisionOutcomeApiSubmitRequest", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    Path((project_id, writer_task_id)): Path<(String, String)>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        let outcome = match repository.revision_outcome(&project_id, &writer_task_id) {
            Ok(Some(value)) => value,
            Ok(None) => return Json(serde_json::Value::Null).into_response(),
            Err(source) => return repository_error(source),
        };
        Json(json!({
            "revisionId": outcome.revision_id,
            "authority": "candidateAnalysis",
            "state": if outcome.current { "current" } else { "stale" },
            "assessment": outcome.content,
            "issues":outcome.issues,
            "decisions": outcome.decisions
        }))
        .into_response()
    })
    .await
}

pub(crate) async fn decide(
    State(state): State<RuntimeState>,
    Path((project_id, writer_task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    Json(request): Json<RevisionOutcomeDecisionRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        if actor != Actor::Author {
            return error(
                StatusCode::FORBIDDEN,
                "author_required",
                "Only the author can decide a revision outcome",
            );
        }
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(source) => return bad_request("invalid_project_id", source),
        };

        match repository.decide_revision_outcome(&project_id, &writer_task_id, request) {
            Ok(decision) => (StatusCode::CREATED, Json(json!(decision))).into_response(),
            Err(source) => repository_error(source),
        }
    })
    .await
}

pub(crate) async fn submit(
    State(state): State<RuntimeState>,
    Path((project_id, writer_task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<SubmitRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
    if actor != Actor::Candidate {
        return error(
            StatusCode::FORBIDDEN,
            "candidate_task_required",
            "A task-bound candidate is required for revision outcome assessment",
        );
    }
    let project_id = match ProjectId::parse(project_id) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_project_id", source),
    };
    let object_id = match ObjectId::parse(format!("revision-outcome-{writer_task_id}")) {
        Ok(value) => value,
        Err(source) => return bad_request("invalid_writer_task_id", source),
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
        Ok(Some(task)) if task.operation == "assessRevisionOutcome" => task,
        Ok(_) => return bad_request("invalid_task_operation", "Expected assessRevisionOutcome"),
        Err(response) => return *response,
    };
    let target: serde_json::Value = match serde_json::from_str(&task.target_json) {
        Ok(value) => value,
        Err(_) => return repository_error(RepositoryError::CorruptState),
    };
    if target["writerTaskId"] != writer_task_id || request.payload["writerTaskId"] != writer_task_id
    {
        return bad_request(
            "invalid_writer_task_id",
            "Payload and task writer must match route",
        );
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
    if let Err(source) = crate::recovery_binding::bind(
        &mut repository,
        &headers,
        Some(&task),
        "submit_revision_outcome",
        Some(idempotency_key.as_str()),
    ) {
        return repository_error(source);
    }
    let payload = match repository
        .prepare_revision_outcome_payload_with_recovery(&task, &request.payload)
    {
        Ok(value) => value,
        Err(source) => return repository_error(source),
    };
    let content_json = payload.to_string();
    if content_json.len() > 24_000 {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "revision_outcome_too_large",
            "Revision outcome exceeds 24,000 bytes",
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
        object_kind: "revisionOutcome".into(),
        revision_id: RevisionId::parse(format!("revision-outcome-{suffix}"))
            .expect("generated revision id"),
        commit_id: CommitId::parse(format!("candidate-{suffix}")).expect("generated commit id"),
        idempotency_key,
        request_digest,
        expected_project_revision: request.expected_revision,
        expected_snapshot_id: head.snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}"))
            .expect("generated snapshot id"),
        expected_candidate_revision_id,
        content_digest: digest(&payload),
        content_json,
        chapter_projection: None,
        task_output: Some(TaskOutputBinding {
            task_id: task.task_id,
            thread_id: task.thread_id,
            output_kind: "revisionOutcome".into(),
            completeness: None,
        }),
        linked_impact_ids: Vec::new(),
    });
    match result {
        Ok(receipt) => (
            StatusCode::CREATED,
            Json(json!({
                "revisionId": receipt.revision_id.as_str(),
                "writerTaskId": writer_task_id,
                "status": "candidate"
            })),
        )
            .into_response(),
        Err(source) => repository_error(source),
    }

    }).await
}

#[derive(Deserialize, ts_rs::TS)]
#[ts(export, export_to = "runtime/", rename = "RevisionOutcomeApiReadQuery", optional_fields = nullable)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadQuery {
    thread_id: String,
    expected_revision: i64,
    writer_task_id: String,
}
pub(crate) async fn read_for_task(
    State(state): State<RuntimeState>,
    Path((project_id, task_id)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ReadQuery>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |repo| {
    let project = match ProjectId::parse(project_id) {
        Ok(p) => p,
        Err(e) => return bad_request("invalid_project_id", e),
    };

    let task = match candidate_task(
        &repo,
        actor,
        &headers,
        &project,
        Some(&task_id),
        Some(&query.thread_id),
        Some(query.expected_revision),
    ) {
        Ok(Some(t)) => t,
        Ok(None) => return bad_request("task_capability_required", "Task bound reader required"),
        Err(e) => return *e,
    };
    match repo.task_revision_outcome(&task, &query.writer_task_id) {
        Ok(Some(outcome)) => {
            let mut context = json!({"issues":outcome.issues,"revisionId":outcome.revision_id,"authority":"candidateAnalysis","state":if outcome.current {"current"}else{"stale"},"writerTaskId":query.writer_task_id,"draftRevisionId":outcome.content["draftRevisionId"],"operationContext":{"revisionOutcomeReport":outcome.content},"coverage":{"complete":true,"missingScope":[]}});
            if let Err(e) = repo.project_task_sources(&task, &mut context) {
                return repository_error(e);
            }
            context["assessment"] = context["operationContext"]["revisionOutcomeReport"].clone();
            context["source"] = context["taskSources"]["entries"][0].clone();
            context.as_object_mut().unwrap().remove("operationContext");
            Json(context).into_response()
        }
        Ok(None) => Json(serde_json::Value::Null).into_response(),
        Err(e) => repository_error(e),
    }

    }).await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/revision-outcomes/{writer_task_id}",
            axum::routing::get(crate::revision_outcome_api::get),
        )
        .route(
            "/api/projects/{project_id}/revision-outcomes/{writer_task_id}/decisions",
            axum::routing::post(crate::revision_outcome_api::decide),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/tasks/{task_id}/revision-outcome",
            axum::routing::get(crate::revision_outcome_api::read_for_task),
        )
        .route(
            "/api/projects/{project_id}/revision-outcomes/{writer_task_id}",
            axum::routing::post(crate::revision_outcome_api::submit),
        )
}

use novel_codex_story_core::{ProjectRevisionRepository, RecoveryOperationsRepository, RevisionOutcomeDecisionRepository, RevisionOutcomeRepository, TaskSourcesRepository};
