use rusqlite::OptionalExtension;
use rusqlite::params;

use crate::BranchId;
use crate::CommitId;
use crate::IdempotencyKey;
use crate::ObjectId;
use crate::ProjectId;
use crate::RepositoryError;
use crate::RevisionId;
use crate::SnapshotId;
use crate::SqliteStoryRepository;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateReviewRequest {
    pub review_id: String,
    pub project_id: ProjectId,
    pub object_id: ObjectId,
    pub object_kind: String,
    pub revision_id: RevisionId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub checks_json: String,
    pub proposals_json: String,
    pub selections_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRecord {
    pub review_id: String,
    pub project_id: ProjectId,
    pub object_id: ObjectId,
    pub object_kind: String,
    pub revision_id: RevisionId,
    pub project_revision: i64,
    pub checks_json: String,
    pub proposals_json: String,
    pub selections_json: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptReviewRequest {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub review_id: String,
    pub commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub expected_project_revision: i64,
    pub next_snapshot_id: SnapshotId,
    pub selections_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoAcceptanceRequest {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub target_commit_id: CommitId,
    pub commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub expected_project_revision: i64,
    pub next_snapshot_id: SnapshotId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptanceReceipt {
    pub commit_id: CommitId,
    pub result_revision: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptanceRecord {
    pub commit_id: CommitId,
    pub project_id: ProjectId,
    pub operation: String,
    pub object_kind: String,
    pub accepted_revision_id: RevisionId,
    pub previous_accepted_revision_id: Option<RevisionId>,
    pub base_revision: i64,
    pub result_revision: i64,
    pub target_commit_id: Option<CommitId>,
}

impl SqliteStoryRepository {
    pub fn create_review(
        &mut self,
        request: CreateReviewRequest,
    ) -> Result<ReviewRecord, RepositoryError> {
        validate_json(&request.checks_json)?;
        validate_json(&request.proposals_json)?;
        validate_json(&request.selections_json)?;
        let transaction = self.connection.transaction().map_err(storage_error)?;
        if let Some((stored_digest, review_id)) = transaction
            .query_row(
                "SELECT request_digest, review_id FROM story_review_receipts_v2 WHERE project_id = ?1 AND idempotency_key = ?2",
                params![request.project_id.as_str(), request.idempotency_key.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage_error)?
        {
            if stored_digest != request.request_digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return load_review(&transaction, &review_id);
        }
        let revision = transaction
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [request.project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        let candidate = transaction
            .query_row(
                "SELECT object_kind, candidate_revision_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![request.project_id.as_str(), request.object_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CandidateNotFound)?;
        if candidate.0 != request.object_kind
            || candidate.1.as_deref() != Some(request.revision_id.as_str())
        {
            return Err(RepositoryError::StaleCandidate {
                expected: Some(request.revision_id),
                actual: candidate
                    .1
                    .map(RevisionId::parse)
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptState)?,
            });
        }
        transaction
            .execute(
                "INSERT INTO story_reviews_v2(review_id, project_id, object_id, object_kind, revision_id, project_revision, checks_json, proposals_json, selections_json, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'ready')",
                params![request.review_id, request.project_id.as_str(), request.object_id.as_str(), request.object_kind, request.revision_id.as_str(), revision, request.checks_json, request.proposals_json, request.selections_json],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_review_receipts_v2(project_id, idempotency_key, request_digest, review_id) VALUES (?1, ?2, ?3, ?4)",
                params![request.project_id.as_str(), request.idempotency_key.as_str(), request.request_digest, request.review_id],
            )
            .map_err(storage_error)?;
        let review = load_review(&transaction, &request.review_id)?;
        transaction.commit().map_err(storage_error)?;
        Ok(review)
    }

    pub fn review(
        &self,
        project_id: &ProjectId,
        review_id: &str,
    ) -> Result<Option<ReviewRecord>, RepositoryError> {
        let review = self
            .connection
            .query_row(
                "SELECT review_id FROM story_reviews_v2 WHERE project_id = ?1 AND review_id = ?2",
                params![project_id.as_str(), review_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?;
        review
            .map(|review_id| load_review(&self.connection, &review_id))
            .transpose()
    }

    pub fn reviews(&self, project_id: &ProjectId) -> Result<Vec<ReviewRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT review_id FROM story_reviews_v2 WHERE project_id = ?1 ORDER BY rowid DESC LIMIT 30")
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|row| load_review(&self.connection, &row.map_err(storage_error)?))
            .collect()
    }

    pub fn facts(&self, project_id: &ProjectId) -> Result<Vec<serde_json::Value>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json, project_revision FROM story_facts_v2 WHERE project_id = ?1 AND reverted_by_commit_id IS NULL ORDER BY project_revision, rowid")
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(storage_error)?
            .map(|row| {
                let (payload, revision) = row.map_err(storage_error)?;
                let mut payload: serde_json::Value =
                    serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
                payload["projectRevision"] = serde_json::json!(revision);
                Ok(payload)
            })
            .collect()
    }

    pub fn acceptances(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<AcceptanceRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT commit_id, project_id, operation, object_kind, accepted_revision_id, previous_accepted_revision_id, base_revision, result_revision, target_commit_id FROM story_acceptances_v2 WHERE project_id = ?1 ORDER BY result_revision DESC LIMIT 30",
            )
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            })
            .map_err(storage_error)?
            .map(|row| {
                let row = row.map_err(storage_error)?;
                Ok(AcceptanceRecord {
                    commit_id: CommitId::parse(row.0).map_err(|_| RepositoryError::CorruptState)?,
                    project_id: ProjectId::parse(row.1)
                        .map_err(|_| RepositoryError::CorruptState)?,
                    operation: row.2,
                    object_kind: row.3,
                    accepted_revision_id: RevisionId::parse(row.4)
                        .map_err(|_| RepositoryError::CorruptState)?,
                    previous_accepted_revision_id: row
                        .5
                        .map(RevisionId::parse)
                        .transpose()
                        .map_err(|_| RepositoryError::CorruptState)?,
                    base_revision: row.6,
                    result_revision: row.7,
                    target_commit_id: row
                        .8
                        .map(CommitId::parse)
                        .transpose()
                        .map_err(|_| RepositoryError::CorruptState)?,
                })
            })
            .collect()
    }

    pub fn accept_review(
        &mut self,
        request: AcceptReviewRequest,
    ) -> Result<AcceptanceReceipt, RepositoryError> {
        validate_json(&request.selections_json)?;
        let transaction = self.connection.transaction().map_err(storage_error)?;
        if let Some(receipt) = replay_acceptance(
            &transaction,
            &request.project_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        let review = load_review(&transaction, &request.review_id)?;
        let current_revision = current_revision(&transaction, &request.project_id)?;
        if current_revision != request.expected_project_revision
            || review.project_revision != request.expected_project_revision
            || review.status != "ready"
        {
            return Err(RepositoryError::StaleProjectRevision {
                expected: request.expected_project_revision,
                actual: current_revision,
            });
        }
        let checks: serde_json::Value = serde_json::from_str(&review.checks_json)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        if checks.as_array().is_some_and(|checks| {
            checks.iter().any(|check| {
                check.get("blocking").and_then(serde_json::Value::as_bool) == Some(true)
                    && check.get("passed").and_then(serde_json::Value::as_bool) != Some(true)
            })
        }) {
            return Err(RepositoryError::ReviewChecksFailed);
        }
        let head = object_head(&transaction, &request.project_id, &review.object_id)?;
        if head.1.as_deref() != Some(review.revision_id.as_str()) || head.3 {
            return Err(if head.3 {
                RepositoryError::ObjectLocked
            } else {
                RepositoryError::StaleCandidate {
                    expected: Some(review.revision_id),
                    actual: head
                        .1
                        .map(RevisionId::parse)
                        .transpose()
                        .map_err(|_| RepositoryError::CorruptState)?,
                }
            });
        }
        let base_snapshot_id = branch_head(&transaction, &request.project_id, &request.branch_id)?;
        copy_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &base_snapshot_id,
            &request.next_snapshot_id,
        )?;
        let result_revision = current_revision + 1;
        transaction
            .execute(
                "UPDATE story_object_heads_v2 SET accepted_revision_id = candidate_revision_id WHERE project_id = ?1 AND object_id = ?2",
                params![request.project_id.as_str(), review.object_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2 AND revision = ?3",
                params![result_revision, request.project_id.as_str(), current_revision],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_reviews_v2 SET status = 'accepted', selections_json = ?1 WHERE review_id = ?2",
                params![request.selections_json, review.review_id],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, review_id, object_id, object_kind, accepted_revision_id, previous_accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'accept')",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), review.review_id, review.object_id.as_str(), review.object_kind, review.revision_id.as_str(), head.2, current_revision, result_revision, base_snapshot_id.as_str(), request.next_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        crate::impacts::resolve_linked_impacts_on_accept(
            &transaction,
            &request.project_id,
            &review.object_kind,
            &review.revision_id,
            result_revision,
        )?;
        apply_confirmed_facts(
            &transaction,
            &request.project_id,
            &request.commit_id,
            result_revision,
            &review.proposals_json,
            &request.selections_json,
        )?;
        store_acceptance_receipt(
            &transaction,
            &request.project_id,
            &request.idempotency_key,
            &request.request_digest,
            &request.commit_id,
            result_revision,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(AcceptanceReceipt {
            commit_id: request.commit_id,
            result_revision,
        })
    }

    pub fn undo_acceptance(
        &mut self,
        request: UndoAcceptanceRequest,
    ) -> Result<AcceptanceReceipt, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        if let Some(receipt) = replay_acceptance(
            &transaction,
            &request.project_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        let current = current_revision(&transaction, &request.project_id)?;
        let target = transaction
            .query_row(
                "SELECT branch_id, object_id, object_kind, accepted_revision_id, previous_accepted_revision_id, result_revision, result_snapshot_id, undone_by_commit_id FROM story_acceptances_v2 WHERE project_id = ?1 AND commit_id = ?2 AND operation = 'accept'",
                params![request.project_id.as_str(), request.target_commit_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CommitNotFound)?;
        if current != request.expected_project_revision
            || target.5 != current
            || target.0 != request.branch_id.as_str()
            || target.7.is_some()
        {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        let base_snapshot_id =
            SnapshotId::parse(target.6).map_err(|_| RepositoryError::CorruptState)?;
        let active_head = branch_head(&transaction, &request.project_id, &request.branch_id)?;
        if active_head != base_snapshot_id {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        copy_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &base_snapshot_id,
            &request.next_snapshot_id,
        )?;
        let result_revision = current + 1;
        transaction
            .execute(
                "UPDATE story_object_heads_v2 SET accepted_revision_id = ?1 WHERE project_id = ?2 AND object_id = ?3 AND accepted_revision_id = ?4",
                params![target.4, request.project_id.as_str(), target.1, target.3],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2 AND revision = ?3",
                params![result_revision, request.project_id.as_str(), current],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_acceptances_v2 SET undone_by_commit_id = ?1 WHERE commit_id = ?2",
                params![
                    request.commit_id.as_str(),
                    request.target_commit_id.as_str()
                ],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, object_id, object_kind, accepted_revision_id, previous_accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'undo', ?12)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), target.1, target.2, target.4.as_deref().unwrap_or(&target.3), target.3, current, result_revision, base_snapshot_id.as_str(), request.next_snapshot_id.as_str(), request.target_commit_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_facts_v2 SET reverted_by_commit_id = ?1 WHERE project_id = ?2 AND accepted_by_commit_id = ?3 AND reverted_by_commit_id IS NULL",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.target_commit_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_planning_impacts_v2 SET reverted_by_commit_id = ?1 WHERE project_id = ?2 AND source_commit_id = ?3 AND reverted_by_commit_id IS NULL",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.target_commit_id.as_str()],
            )
            .map_err(storage_error)?;
        store_acceptance_receipt(
            &transaction,
            &request.project_id,
            &request.idempotency_key,
            &request.request_digest,
            &request.commit_id,
            result_revision,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(AcceptanceReceipt {
            commit_id: request.commit_id,
            result_revision,
        })
    }
}

fn apply_confirmed_facts(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    commit_id: &CommitId,
    project_revision: i64,
    proposals_json: &str,
    selections_json: &str,
) -> Result<(), RepositoryError> {
    let proposals: Vec<serde_json::Value> = serde_json::from_str(proposals_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    let selections: serde_json::Value = serde_json::from_str(selections_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    let decisions = selections
        .get("factDecisions")
        .and_then(serde_json::Value::as_object);
    for proposal in proposals {
        let Some(fact_id) = proposal
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        else {
            return Err(RepositoryError::InvalidContent(
                "fact proposal id is missing".into(),
            ));
        };
        if decisions
            .and_then(|values| values.get(&fact_id))
            .and_then(serde_json::Value::as_str)
            != Some("confirmed")
        {
            continue;
        }
        let mut accepted = proposal;
        accepted["reviewDecision"] = serde_json::json!("confirmed");
        connection
            .execute(
                "INSERT INTO story_facts_v2(fact_id, project_id, payload_json, accepted_by_commit_id, project_revision) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![fact_id, project_id.as_str(), accepted.to_string(), commit_id.as_str(), project_revision],
            )
            .map_err(storage_error)?;
        crate::impacts::record_impacts_for_fact(
            connection,
            project_id,
            commit_id.as_str(),
            &accepted,
            project_revision,
        )?;
    }
    Ok(())
}

fn validate_json(value: &str) -> Result<(), RepositoryError> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn current_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)
}

fn load_review(
    connection: &rusqlite::Connection,
    review_id: &str,
) -> Result<ReviewRecord, RepositoryError> {
    connection
        .query_row(
            "SELECT review_id, project_id, object_id, object_kind, revision_id, project_revision, checks_json, proposals_json, selections_json, status FROM story_reviews_v2 WHERE review_id = ?1",
            [review_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?, row.get::<_, String>(9)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ReviewNotFound)
        .and_then(|row| {
            Ok(ReviewRecord {
                review_id: row.0,
                project_id: ProjectId::parse(row.1).map_err(|_| RepositoryError::CorruptState)?,
                object_id: ObjectId::parse(row.2).map_err(|_| RepositoryError::CorruptState)?,
                object_kind: row.3,
                revision_id: RevisionId::parse(row.4).map_err(|_| RepositoryError::CorruptState)?,
                project_revision: row.5,
                checks_json: row.6,
                proposals_json: row.7,
                selections_json: row.8,
                status: row.9,
            })
        })
}

fn object_head(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    object_id: &ObjectId,
) -> Result<(String, Option<String>, Option<String>, bool), RepositoryError> {
    connection
        .query_row(
            "SELECT object_kind, candidate_revision_id, accepted_revision_id, locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
            params![project_id.as_str(), object_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::CandidateNotFound)
}

fn branch_head(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
) -> Result<SnapshotId, RepositoryError> {
    let value = connection
        .query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
            params![project_id.as_str(), branch_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::BranchNotFound)?;
    SnapshotId::parse(value).map_err(|_| RepositoryError::CorruptState)
}

fn copy_snapshot(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    base: &SnapshotId,
    next: &SnapshotId,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, ?4)",
            params![next.as_str(), project_id.as_str(), branch_id.as_str(), base.as_str()],
        )
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) SELECT ?1, object_id, revision_id FROM story_snapshot_members_v2 WHERE snapshot_id = ?2",
            params![next.as_str(), base.as_str()],
        )
        .map_err(storage_error)?;
    let changed = connection
        .execute(
            "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
            params![next.as_str(), project_id.as_str(), branch_id.as_str(), base.as_str()],
        )
        .map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::StaleSnapshot {
            expected: base.clone(),
            actual: branch_head(connection, project_id, branch_id)?,
        });
    }
    Ok(())
}

fn replay_acceptance(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    key: &IdempotencyKey,
    digest: &str,
) -> Result<Option<AcceptanceReceipt>, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT request_digest, commit_id, result_revision FROM story_acceptance_receipts_v2 WHERE project_id = ?1 AND idempotency_key = ?2",
            params![project_id.as_str(), key.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((stored_digest, commit_id, result_revision)) = row else {
        return Ok(None);
    };
    if stored_digest != digest {
        return Err(RepositoryError::IdempotencyConflict);
    }
    Ok(Some(AcceptanceReceipt {
        commit_id: CommitId::parse(commit_id).map_err(|_| RepositoryError::CorruptState)?,
        result_revision,
    }))
}

fn store_acceptance_receipt(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    key: &IdempotencyKey,
    digest: &str,
    commit_id: &CommitId,
    result_revision: i64,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO story_acceptance_receipts_v2(project_id, idempotency_key, request_digest, commit_id, result_revision) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id.as_str(), key.as_str(), digest, commit_id.as_str(), result_revision],
        )
        .map_err(storage_error)?;
    Ok(())
}
