//! Review application boundary: identities and envelopes belong to Story Core.
//! Persistence methods retain their existing transactions, CAS and receipt checks.
use crate::{
    AcceptReviewRequest, AcceptanceReceipt, BranchId, CommitId, CreateReviewRequest,
    IdempotencyKey, ObjectId, ProjectId, RepositoryError, ReviewRecord, RevisionId, SnapshotId,
    SqliteStoryRepository, UndoAcceptanceRequest,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct ReviewSubmission {
    pub project_id: ProjectId,
    pub artifact_kind: String,
    pub revision_id: RevisionId,
    pub idempotency_key: IdempotencyKey,
    pub author: bool,
    pub proposals: Vec<Value>,
    pub modeling_batch_id: Option<String>,
    pub selections: Value,
}

pub struct ReviewCreation {
    pub review: ReviewRecord,
    pub checks: Value,
}

pub struct ReviewAcceptance {
    pub project_id: ProjectId,
    pub review_id: String,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: i64,
    pub selections: Value,
}

pub struct ReviewUndo {
    pub project_id: ProjectId,
    pub target_commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: i64,
}

/// Borrows the repository; it does not introduce another transaction or fact authority.
pub struct ReviewStore<'a> {
    repository: &'a mut SqliteStoryRepository,
}

impl SqliteStoryRepository {
    pub fn review_store(&mut self) -> ReviewStore<'_> {
        ReviewStore { repository: self }
    }
}

impl ReviewStore<'_> {
    pub fn create(
        &mut self,
        submission: ReviewSubmission,
    ) -> Result<ReviewCreation, RepositoryError> {
        let ReviewSubmission {
            project_id,
            artifact_kind,
            revision_id,
            idempotency_key,
            author,
            proposals,
            modeling_batch_id,
            selections,
        } = submission;
        if proposals.len() > 200 || proposals.iter().any(|proposal| !proposal.is_object()) {
            return Err(RepositoryError::InvalidContent(
                "Fact proposals are invalid".into(),
            ));
        }
        if !matches!(
            artifact_kind.as_str(),
            "brief" | "outline" | "plan" | "draft" | "identity" | "propertyRules"
        ) {
            return Err(RepositoryError::InvalidContent(
                "Artifact kind is invalid".into(),
            ));
        }
        let review_id = format!("review-{}", Uuid::new_v4().simple());
        let proposals = proposals
            .into_iter()
            .map(|mut proposal| {
                proposal["id"] = json!(format!("fact-{}", Uuid::new_v4().simple()));
                proposal["source"] = json!(if author { "author" } else { "agent" });
                proposal["reviewDecision"] = json!("candidate");
                proposal
            })
            .collect::<Vec<_>>();
        let checks = json!([{
            "id": "current-candidate", "passed": true, "blocking": true,
            "message": "候选版本是当前待审版本"
        }]);
        let selections = if selections.is_object() {
            selections
        } else {
            json!({})
        };
        // Keep the existing receipt digest contract, including enriched fact proposals.
        let request_digest = digest(&json!({
            "operation": "createReview", "artifactKind": artifact_kind,
            "revisionId": revision_id.as_str(), "modelingBatchId": modeling_batch_id,
            "proposals": proposals, "selections": selections
        }));
        let object_id = if artifact_kind == "draft" {
            self.repository
                .object_heads(&project_id)?
                .into_iter()
                .find(|head| {
                    head.object_kind == "draft"
                        && head.candidate_revision_id.as_ref() == Some(&revision_id)
                })
                .map(|head| head.object_id)
                .ok_or(RepositoryError::CandidateNotFound)?
        } else {
            ObjectId::parse(format!("artifact-{artifact_kind}"))
                .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?
        };
        let review = self.repository.create_review(CreateReviewRequest {
            review_id,
            project_id,
            object_id,
            object_kind: artifact_kind,
            revision_id,
            idempotency_key,
            request_digest,
            checks_json: checks.to_string(),
            proposals_json: Value::Array(proposals).to_string(),
            modeling_batch_id,
            selections_json: selections.to_string(),
        })?;
        Ok(ReviewCreation { review, checks })
    }

    pub fn accept(
        &mut self,
        submission: ReviewAcceptance,
    ) -> Result<AcceptanceReceipt, RepositoryError> {
        let ReviewAcceptance {
            project_id,
            review_id,
            idempotency_key,
            expected_revision,
            selections,
        } = submission;
        if !selections.is_object() {
            return Err(RepositoryError::InvalidContent(
                "Review selections must be an object".into(),
            ));
        }
        let request_digest = digest(&json!({
            "reviewId": review_id, "expectedRevision": expected_revision, "selections": selections
        }));
        let suffix = Uuid::new_v4().simple().to_string();
        self.repository.accept_review(AcceptReviewRequest {
            project_id,
            branch_id: BranchId::main(),
            review_id,
            commit_id: CommitId::parse(format!("commit-{suffix}"))
                .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?,
            idempotency_key,
            request_digest,
            expected_project_revision: expected_revision,
            next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}"))
                .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?,
            selections_json: selections.to_string(),
        })
    }

    pub fn reject_identity(
        &mut self,
        project_id: &ProjectId,
        review_id: &str,
        expected_revision: i64,
    ) -> Result<(), RepositoryError> {
        self.repository
            .reject_identity_review(project_id, review_id, expected_revision)
    }

    pub fn undo(&mut self, submission: ReviewUndo) -> Result<AcceptanceReceipt, RepositoryError> {
        let ReviewUndo {
            project_id,
            target_commit_id,
            idempotency_key,
            expected_revision,
        } = submission;
        let request_digest = digest(&json!({
            "commitId": target_commit_id.as_str(), "expectedRevision": expected_revision
        }));
        let suffix = Uuid::new_v4().simple().to_string();
        self.repository.undo_acceptance(UndoAcceptanceRequest {
            project_id,
            branch_id: BranchId::main(),
            target_commit_id,
            commit_id: CommitId::parse(format!("undo-{suffix}"))
                .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?,
            idempotency_key,
            request_digest,
            expected_project_revision: expected_revision,
            next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}"))
                .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?,
        })
    }
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

use crate::{ProjectRevisionRepository, WorkflowRepository};
