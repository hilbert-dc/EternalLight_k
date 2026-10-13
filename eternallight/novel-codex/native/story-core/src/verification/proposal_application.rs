use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::acceptance_shared::{
    AcceptanceWrite, ValidatedReview, record_review_acceptance, validate_review_for_accept,
};
use crate::proposal_sets::BoundOperation;
use crate::workflow::{branch_head, copy_snapshot, current_revision, object_head};
use crate::{BranchId, CommitId, ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

pub struct ApplyProposalSet<'a> {
    pub project_id: &'a ProjectId,
    pub proposal_set_id: &'a str,
    pub selected_operation_ids: &'a [String],
    pub selections_by_operation_id: &'a BTreeMap<String, Value>,
    pub idempotency_key: &'a str,
    pub now: i64,
}

pub struct UndoProposalSet<'a> {
    pub project_id: &'a ProjectId,
    pub application_id: &'a str,
    pub expected_project_revision: i64,
    pub idempotency_key: &'a str,
}



pub(crate) fn validate_selected_reviews(
    connection: &rusqlite::Connection,
    project: &ProjectId,
    operations: &[BoundOperation],
    selected: &[String],
    choices: &BTreeMap<String, Value>,
    revision: i64,
) -> Result<Vec<(BoundOperation, ValidatedReview, String)>, RepositoryError> {
    let selected = selected_operations(operations, selected)?;
    if selected.len() != choices.len() {
        return Err(RepositoryError::InvalidContent(
            "invalid proposal choice keys".into(),
        ));
    }
    let mut validated = Vec::with_capacity(selected.len());
    for operation in selected {
        let selections = choices
            .get(&operation.operation_id)
            .ok_or_else(|| RepositoryError::InvalidContent("missing proposal selections".into()))?;
        let serialized = selections.to_string();
        if serialized.len() > 16_000 {
            return Err(RepositoryError::InvalidContent(
                "proposal selections too large".into(),
            ));
        }
        let review = validate_review_for_accept(
            connection,
            project,
            &operation.review_id,
            revision,
            &serialized,
        )?;
        if review.review.object_id.as_str() != operation.object_id
            || review.review.object_kind != operation.object_kind
            || review.review.revision_id.as_str() != operation.candidate_revision_id
            || review.previous_accepted_revision_id != operation.expected_accepted_revision_id
        {
            return Err(RepositoryError::InvalidContent(
                "proposal operation is stale".into(),
            ));
        }
        crate::proposal_selection::validate_choices(connection, &review.review, selections)?;
        validated.push((operation, review, serialized));
    }
    validate_source_closure(connection, project, &validated)?;
    Ok(validated)
}

fn selected_operations(
    operations: &[BoundOperation],
    selected: &[String],
) -> Result<Vec<BoundOperation>, RepositoryError> {
    let by_id = operations
        .iter()
        .map(|op| (op.operation_id.as_str(), op))
        .collect::<BTreeMap<_, _>>();
    let selected_ids = selected.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if selected_ids.len() != selected.len() || selected_ids.iter().any(|id| !by_id.contains_key(id))
    {
        return Err(RepositoryError::InvalidContent(
            "invalid proposal selection".into(),
        ));
    }
    let mut ordered = Vec::with_capacity(selected.len());
    let mut visited = BTreeSet::new();
    for id in selected {
        visit_selected(id, &by_id, &selected_ids, &mut visited, &mut ordered)?;
    }
    Ok(ordered)
}

fn visit_selected(
    id: &str,
    by_id: &BTreeMap<&str, &BoundOperation>,
    selected: &BTreeSet<&str>,
    visited: &mut BTreeSet<String>,
    ordered: &mut Vec<BoundOperation>,
) -> Result<(), RepositoryError> {
    if visited.contains(id) {
        return Ok(());
    }
    let operation = by_id.get(id).ok_or(RepositoryError::CorruptState)?;
    for dependency in &operation.depends_on {
        if !selected.contains(dependency.as_str()) {
            return Err(RepositoryError::ReviewChecksFailed);
        }
        visit_selected(dependency, by_id, selected, visited, ordered)?;
    }
    visited.insert(id.to_owned());
    ordered.push((*operation).clone());
    Ok(())
}

fn validate_source_closure(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    selected: &[(BoundOperation, ValidatedReview, String)],
) -> Result<(), RepositoryError> {
    let selected_revisions = selected
        .iter()
        .map(|(_, review, _)| review.review.revision_id.as_str())
        .collect::<BTreeSet<_>>();
    for (_, review, selections_json) in selected {
        let proposals: Vec<Value> = serde_json::from_str(&review.review.proposals_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let selections: Value =
            serde_json::from_str(selections_json).map_err(|_| RepositoryError::CorruptState)?;
        for proposal in proposals {
            let Some(id) = proposal["id"].as_str() else {
                return Err(RepositoryError::CorruptState);
            };
            if selections["factDecisions"][id] != "confirmed" {
                continue;
            }
            let source_revision = proposal["sourceRevisionId"]
                .as_str()
                .unwrap_or(review.review.revision_id.as_str());
            let (object_id, content) = connection.query_row(
                "SELECT object_id, content_json FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2",
                params![project_id.as_str(), source_revision],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ).optional().map_err(storage_error)?.ok_or(RepositoryError::ReviewChecksFailed)?;
            let accepted_source = object_head(
                connection,
                project_id,
                &crate::ObjectId::parse(object_id).map_err(|_| RepositoryError::CorruptState)?,
            )?
            .2;
            if !selected_revisions.contains(source_revision)
                && accepted_source.as_deref() != Some(source_revision)
            {
                return Err(RepositoryError::ReviewChecksFailed);
            }
            let body = serde_json::from_str::<Value>(&content)
                .map_err(|_| RepositoryError::CorruptState)?;
            let quote = proposal["evidence"]
                .as_str()
                .filter(|quote| !quote.trim().is_empty())
                .ok_or(RepositoryError::ReviewChecksFailed)?;
            if !body["body"]
                .as_str()
                .is_some_and(|text| text.contains(quote))
            {
                return Err(RepositoryError::ReviewChecksFailed);
            }
        }
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "proposal_application_tests.rs"]
mod tests;

/// Public proposal application capability; SQLite retains transaction ownership.
pub trait ProposalApplicationRepository {
    /// Accepts selected reviewed candidates in one transaction and one story snapshot.
    fn apply_proposal_set(
        &mut self,
        request: ApplyProposalSet<'_>,
    ) -> Result<Value, RepositoryError>;
    /// Compensates every member of the latest group in one transaction and one new snapshot.
    fn undo_proposal_set(&mut self, request: UndoProposalSet<'_>)
    -> Result<Value, RepositoryError>;
}

impl ProposalApplicationRepository for SqliteStoryRepository {
    /// Accepts selected reviewed candidates in one transaction and one story snapshot.
    fn apply_proposal_set(
        &mut self,
        request: ApplyProposalSet<'_>,
    ) -> Result<Value, RepositoryError> {
        if !request.proposal_set_id.starts_with("proposal-set-")
            || request.proposal_set_id.len() > 80
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
            || request.selected_operation_ids.is_empty()
            || request.selected_operation_ids.len() > 20
            || request.selections_by_operation_id.len() != request.selected_operation_ids.len()
        {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal application request".into(),
            ));
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(
                json!({
                    "proposalSetId": request.proposal_set_id,
                    "selectedOperationIds": request.selected_operation_ids,
                    "selectionsByOperationId": request.selections_by_operation_id
                })
                .to_string()
                .as_bytes()
            )
        );
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = transaction.query_row(
            "SELECT application_id, request_digest, result_revision, result_snapshot_id FROM story_proposal_applications_v1 WHERE project_id = ?1 AND idempotency_key = ?2",
            params![request.project_id.as_str(), request.idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
        ).optional().map_err(storage_error)?;
        if let Some((id, prior_digest, revision, snapshot)) = prior {
            if prior_digest != digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return Ok(
                json!({"applicationId":id,"resultRevision":revision,"resultSnapshotId":snapshot,"replayed":true}),
            );
        }
        let row = transaction.query_row(
            "SELECT base_snapshot_id, project_revision, operations_json FROM story_proposal_sets_v1 WHERE project_id = ?1 AND proposal_set_id = ?2 AND branch_id = 'main'",
            params![request.project_id.as_str(), request.proposal_set_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::CandidateNotFound)?;
        let branch = BranchId::parse("main").expect("main branch id");
        let base_snapshot = SnapshotId::parse(row.0).map_err(|_| RepositoryError::CorruptState)?;
        let current_revision = current_revision(&transaction, request.project_id)?;
        if current_revision != row.1 {
            return Err(RepositoryError::StaleProjectRevision {
                expected: row.1,
                actual: current_revision,
            });
        }
        let active_snapshot = branch_head(&transaction, request.project_id, &branch)?;
        if active_snapshot != base_snapshot {
            return Err(RepositoryError::StaleSnapshot {
                expected: base_snapshot,
                actual: active_snapshot,
            });
        }
        let operations: Vec<BoundOperation> =
            serde_json::from_str(&row.2).map_err(|_| RepositoryError::CorruptState)?;
        let validated = validate_selected_reviews(
            &transaction,
            request.project_id,
            &operations,
            request.selected_operation_ids,
            request.selections_by_operation_id,
            row.1,
        )?;
        let application_id = format!("proposal-application-{}", uuid::Uuid::new_v4().simple());
        let next_snapshot =
            SnapshotId::parse(format!("snapshot-{}", uuid::Uuid::new_v4().simple()))
                .map_err(|_| RepositoryError::CorruptState)?;
        copy_snapshot(
            &transaction,
            request.project_id,
            &branch,
            &base_snapshot,
            &next_snapshot,
        )?;
        let result_revision = current_revision + 1;
        transaction.execute(
            "INSERT INTO story_proposal_applications_v1(application_id, project_id, proposal_set_id, base_snapshot_id, result_snapshot_id, base_revision, result_revision, status, idempotency_key, request_digest, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'applied', ?8, ?9, ?10)",
            params![application_id, request.project_id.as_str(), request.proposal_set_id, base_snapshot.as_str(), next_snapshot.as_str(), current_revision, result_revision, request.idempotency_key, digest, request.now],
        ).map_err(storage_error)?;
        for (operation, review, selections_json) in &validated {
            let member_commit =
                CommitId::parse(format!("accept-{}", uuid::Uuid::new_v4().simple()))
                    .map_err(|_| RepositoryError::CorruptState)?;
            record_review_acceptance(
                &transaction,
                review,
                AcceptanceWrite {
                    project_id: request.project_id,
                    branch_id: &branch,
                    commit_id: &member_commit,
                    base_snapshot_id: &base_snapshot,
                    result_snapshot_id: &next_snapshot,
                    base_revision: current_revision,
                    selections_json,
                },
            )?;
            transaction.execute(
                "INSERT INTO story_proposal_application_members_v1(application_id, operation_id, acceptance_commit_id) VALUES (?1, ?2, ?3)",
                params![application_id, operation.operation_id, member_commit.as_str()],
            ).map_err(storage_error)?;
        }
        let updated = transaction.execute(
            "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2 AND revision = ?3",
            params![result_revision, request.project_id.as_str(), current_revision],
        ).map_err(storage_error)?;
        if updated != 1 {
            return Err(RepositoryError::CorruptState);
        }
        transaction.commit().map_err(storage_error)?;
        Ok(
            json!({"applicationId":application_id,"resultRevision":result_revision,
            "resultSnapshotId":next_snapshot.as_str(),"replayed":false}),
        )
    }

    /// Compensates every member of the latest group in one transaction and one new snapshot.
    fn undo_proposal_set(
        &mut self,
        request: UndoProposalSet<'_>,
    ) -> Result<Value, RepositoryError> {
        if !request.application_id.starts_with("proposal-application-")
            || request.application_id.len() > 96
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
        {
            return Err(RepositoryError::InvalidContent(
                "invalid proposal undo request".into(),
            ));
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(
                json!({
                    "applicationId": request.application_id,
                    "expectedProjectRevision": request.expected_project_revision
                })
                .to_string()
                .as_bytes()
            )
        );
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = transaction.query_row(
            "SELECT request_digest, undo_application_id, result_revision, result_snapshot_id FROM story_proposal_undo_receipts_v1 WHERE project_id = ?1 AND idempotency_key = ?2",
            params![request.project_id.as_str(), request.idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?)),
        ).optional().map_err(storage_error)?;
        if let Some((prior_digest, id, revision, snapshot)) = prior {
            if prior_digest != digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return Ok(
                json!({"undoApplicationId":id,"resultRevision":revision,"resultSnapshotId":snapshot,"replayed":true}),
            );
        }
        let group = transaction.query_row(
            "SELECT result_snapshot_id, result_revision, status, undone_by_application_id FROM story_proposal_applications_v1 WHERE project_id = ?1 AND application_id = ?2",
            params![request.project_id.as_str(), request.application_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::CommitNotFound)?;
        let current = current_revision(&transaction, request.project_id)?;
        if current != request.expected_project_revision
            || group.1 != current
            || group.2 != "applied"
            || group.3.is_some()
        {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        let branch = BranchId::parse("main").expect("main branch id");
        let base_snapshot =
            SnapshotId::parse(group.0).map_err(|_| RepositoryError::CorruptState)?;
        if branch_head(&transaction, request.project_id, &branch)? != base_snapshot {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        let mut statement = transaction.prepare(
            "SELECT a.commit_id, a.object_id, a.object_kind, a.accepted_revision_id, a.previous_accepted_revision_id, a.undone_by_commit_id FROM story_proposal_application_members_v1 m JOIN story_acceptances_v2 a ON a.commit_id = m.acceptance_commit_id WHERE m.application_id = ?1 AND a.project_id = ?2 AND a.operation = 'accept' ORDER BY m.rowid DESC",
        ).map_err(storage_error)?;
        let members = statement
            .query_map(
                params![request.application_id, request.project_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        drop(statement);
        if members.is_empty() || members.iter().any(|member| member.5.is_some()) {
            return Err(RepositoryError::CorruptState);
        }
        let next_snapshot =
            SnapshotId::parse(format!("snapshot-{}", uuid::Uuid::new_v4().simple()))
                .map_err(|_| RepositoryError::CorruptState)?;
        copy_snapshot(
            &transaction,
            request.project_id,
            &branch,
            &base_snapshot,
            &next_snapshot,
        )?;
        let result_revision = current + 1;
        for (member_commit, object_id, object_kind, accepted_revision, previous_revision, _) in
            members
        {
            let undo_commit = CommitId::parse(format!("undo-{}", uuid::Uuid::new_v4().simple()))
                .map_err(|_| RepositoryError::CorruptState)?;
            let updated = transaction.execute(
                "UPDATE story_object_heads_v2 SET accepted_revision_id = ?1 WHERE project_id = ?2 AND object_id = ?3 AND accepted_revision_id = ?4",
                params![previous_revision, request.project_id.as_str(), object_id, accepted_revision],
            ).map_err(storage_error)?;
            if updated != 1 {
                return Err(RepositoryError::UndoRequiresLatestCommit);
            }
            let updated = transaction.execute(
                "UPDATE story_acceptances_v2 SET undone_by_commit_id = ?1 WHERE project_id = ?2 AND commit_id = ?3 AND undone_by_commit_id IS NULL",
                params![undo_commit.as_str(), request.project_id.as_str(), member_commit],
            ).map_err(storage_error)?;
            if updated != 1 {
                return Err(RepositoryError::CorruptState);
            }
            transaction.execute(
                "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, object_id, object_kind, accepted_revision_id, previous_accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation, target_commit_id) VALUES (?1, ?2, 'main', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'undo', ?11)",
                params![undo_commit.as_str(), request.project_id.as_str(), object_id, object_kind, previous_revision.as_deref().unwrap_or(&accepted_revision), accepted_revision, current, result_revision, base_snapshot.as_str(), next_snapshot.as_str(), member_commit],
            ).map_err(storage_error)?;
            transaction.execute(
                "UPDATE story_facts_v2 SET reverted_by_commit_id = ?1 WHERE project_id = ?2 AND accepted_by_commit_id = ?3 AND reverted_by_commit_id IS NULL",
                params![undo_commit.as_str(), request.project_id.as_str(), member_commit],
            ).map_err(storage_error)?;
            transaction.execute(
                "UPDATE story_planning_impacts_v2 SET reverted_by_commit_id = ?1 WHERE project_id = ?2 AND source_commit_id = ?3 AND reverted_by_commit_id IS NULL",
                params![undo_commit.as_str(), request.project_id.as_str(), member_commit],
            ).map_err(storage_error)?;
        }
        let updated = transaction.execute(
            "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2 AND revision = ?3",
            params![result_revision, request.project_id.as_str(), current],
        ).map_err(storage_error)?;
        if updated != 1 {
            return Err(RepositoryError::CorruptState);
        }
        let undo_id = format!("proposal-undo-{}", uuid::Uuid::new_v4().simple());
        transaction.execute(
            "UPDATE story_proposal_applications_v1 SET status = 'undone', undone_by_application_id = ?1 WHERE project_id = ?2 AND application_id = ?3 AND status = 'applied'",
            params![undo_id, request.project_id.as_str(), request.application_id],
        ).map_err(storage_error)?;
        transaction.execute(
            "INSERT INTO story_proposal_undo_receipts_v1(project_id, idempotency_key, request_digest, application_id, undo_application_id, result_revision, result_snapshot_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![request.project_id.as_str(), request.idempotency_key, digest, request.application_id, undo_id, result_revision, next_snapshot.as_str()],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(
            json!({"undoApplicationId":undo_id,"resultRevision":result_revision,
            "resultSnapshotId":next_snapshot.as_str(),"replayed":false}),
        )
    }
}
