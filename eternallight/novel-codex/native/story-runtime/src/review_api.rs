use axum::Extension;
use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::{ReviewAcceptance, ReviewSubmission, ReviewUndo};
use serde_json::Value;
use serde_json::json;

use crate::contracts::*;
use crate::server::{Actor, RuntimeState, bad_request, candidate_task, error, repository_error};
pub(crate) async fn create_review(
    State(state): State<RuntimeState>,
    Path(project_id): Path<String>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(request): Json<CreateReviewHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        if request.modeling_batch_id.is_some()
            && (actor != Actor::Author
                || request.artifact_kind != "draft"
                || !request.proposals.is_empty())
        {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_modeling_review",
                "Modeling reviews must be author-initiated and cannot contain caller proposals",
            );
        }
        if actor == Actor::Candidate && !request.proposals.is_empty() {
            return error(
                StatusCode::FORBIDDEN,
                "candidate_fact_proposals_disabled",
                "Candidate facts must be published through a source-bound modeling batch",
            );
        }
        if request.artifact_kind == "identity"
            && (actor != Actor::Author
                || request.modeling_batch_id.is_some()
                || !request.proposals.is_empty())
        {
            return error(
                StatusCode::FORBIDDEN,
                "identity_decision_author_only",
                "Only the author can review identity decisions",
            );
        }
        if request.artifact_kind == "propertyRules"
            && (actor != Actor::Author
                || request.modeling_batch_id.is_some()
                || !request.proposals.is_empty()
                || (!request.selections.is_null()
                    && request
                        .selections
                        .as_object()
                        .is_none_or(|fields| !fields.is_empty())))
        {
            return error(
                StatusCode::FORBIDDEN,
                "property_rules_author_only",
                "Only the author can review property rules without facts or selections",
            );
        }
        if request.proposals.len() > 200
            || request
                .proposals
                .iter()
                .any(|proposal| !proposal.is_object())
        {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_fact_proposals",
                "Fact proposals are invalid",
            );
        }
        if !matches!(
            request.artifact_kind.as_str(),
            "brief" | "outline" | "plan" | "draft" | "identity" | "propertyRules"
        ) {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_artifact_kind",
                "Artifact kind is invalid",
            );
        }
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let revision_id = match RevisionId::parse(request.revision_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_revision_id", error),
        };
        let idempotency_key = match IdempotencyKey::parse(request.idempotency_key) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_idempotency_key", error),
        };
        if let Err(response) = candidate_task(
            &repository,
            actor,
            &headers,
            &project_id,
            request.task_id.as_deref(),
            request.thread_id.as_deref(),
            request.expected_revision,
        ) {
            return *response;
        }
        match repository.review_store().create(ReviewSubmission {
            project_id: project_id.clone(),
            artifact_kind: request.artifact_kind,
            revision_id,
            idempotency_key,
            author: actor == Actor::Author,
            proposals: request.proposals,
            modeling_batch_id: request.modeling_batch_id,
            selections: request.selections,
        }) {
            Ok(created) => {
                let review = created.review;
                (
                StatusCode::CREATED,
                Json(json!({
                    "id": review.review_id,
                    "artifactKind": review.object_kind,
                    "artifactRevisionId": review.revision_id.as_str(),
                    "projectRevision": review.project_revision,
                    "status": review.status,
                    "checks": created.checks,
                    "factProposals": serde_json::from_str::<Value>(&review.proposals_json).unwrap_or(Value::Null),
                    "provisionalImpacts": []
                })),
            )
                .into_response()
            },
            Err(error) => repository_error(error),
        }

    })
    .await
}

pub(crate) async fn accept_review(
    State(state): State<RuntimeState>,
    Path((project_id, review_id)): Path<(String, String)>,
    Json(request): Json<AcceptReviewHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let key = match IdempotencyKey::parse(request.idempotency_key) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_idempotency_key", error),
        };
        if !request.selections.is_object() {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_selections",
                "Review selections must be an object",
            );
        }
        match repository.review_store().accept(ReviewAcceptance {
            project_id: project_id.clone(),
            review_id,
            idempotency_key: key,
            expected_revision: request.expected_revision,
            selections: request.selections,
        }) {
            Ok(receipt) => Json(json!({
                "commitId": receipt.commit_id.as_str(),
                "projectId": project_id.as_str(),
                "operation": "accept",
                "baseRevision": request.expected_revision,
                "resultRevision": receipt.result_revision,
                "confirmedFacts": [],
                "impacts": [],
                "resolvedImpactIds": [],
                "migratedImpactIds": [],
                "indexTargetRevision": receipt.result_revision
            }))
            .into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn reject_identity_review(
    State(state): State<RuntimeState>,
    Path((project_id, review_id)): Path<(String, String)>,
    Json(request): Json<RejectIdentityReviewHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };

        match repository.review_store().reject_identity(
            &project_id,
            &review_id,
            request.expected_revision,
        ) {
            Ok(()) => Json(json!({ "reviewId": review_id, "status": "rejected" })).into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

pub(crate) async fn undo_commit(
    State(state): State<RuntimeState>,
    Path((project_id, commit_id)): Path<(String, String)>,
    Json(request): Json<UndoHttpRequest>,
) -> Response {
    crate::repository_executor::with_repo(state.clone(), move |mut repository| {
        let project_id = match ProjectId::parse(project_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_project_id", error),
        };
        let target_commit_id = match CommitId::parse(commit_id) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_commit_id", error),
        };
        let key = match IdempotencyKey::parse(request.idempotency_key) {
            Ok(value) => value,
            Err(error) => return bad_request("invalid_idempotency_key", error),
        };
        match repository.review_store().undo(ReviewUndo {
            project_id: project_id.clone(),
            target_commit_id,
            idempotency_key: key,
            expected_revision: request.expected_revision,
        }) {
            Ok(receipt) => Json(json!({
                "commitId": receipt.commit_id.as_str(),
                "projectId": project_id.as_str(),
                "operation": "undo",
                "baseRevision": request.expected_revision,
                "resultRevision": receipt.result_revision,
                "indexTargetRevision": receipt.result_revision
            }))
            .into_response(),
            Err(error) => repository_error(error),
        }
    })
    .await
}

/// Author/Host only; authentication is applied by server composition.
pub(crate) fn routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new()
        .route(
            "/api/projects/{project_id}/reviews/{review_id}/accept",
            axum::routing::post(crate::review_api::accept_review),
        )
        .route(
            "/api/projects/{project_id}/reviews/{review_id}/reject-identity",
            axum::routing::post(crate::review_api::reject_identity_review),
        )
        .route(
            "/api/projects/{project_id}/commits/{commit_id}/undo",
            axum::routing::post(crate::review_api::undo_commit),
        )
}

/// Author or candidate; task capabilities remain checked by the handler.
pub(crate) fn writer_routes() -> axum::Router<crate::server::RuntimeState> {
    axum::Router::new().route(
        "/api/projects/{project_id}/reviews",
        axum::routing::post(crate::review_api::create_review),
    )
}

