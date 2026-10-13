use rusqlite::{Connection, OptionalExtension, params};

use crate::workflow::{apply_confirmed_facts, load_review, object_head};
use crate::{BranchId, CommitId, ProjectId, RepositoryError, ReviewRecord, SnapshotId};

#[path = "chapter_acceptance.rs"]
pub(crate) mod chapter;

pub(crate) struct ValidatedReview {
    pub review: ReviewRecord,
    pub previous_accepted_revision_id: Option<String>,
}

pub(crate) struct AcceptanceWrite<'a> {
    pub project_id: &'a ProjectId,
    pub branch_id: &'a BranchId,
    pub commit_id: &'a CommitId,
    pub base_snapshot_id: &'a SnapshotId,
    pub result_snapshot_id: &'a SnapshotId,
    pub base_revision: i64,
    pub selections_json: &'a str,
}

/// Validate one review against the same transaction that will accept it.
pub(crate) fn validate_review_for_accept(
    connection: &Connection,
    project_id: &ProjectId,
    review_id: &str,
    expected_project_revision: i64,
    selections_json: &str,
) -> Result<ValidatedReview, RepositoryError> {
    let review = load_review(connection, review_id)?;
    if review.project_id != *project_id {
        return Err(RepositoryError::CrossProjectReference);
    }
    if review.object_kind == "commitmentExtraction" {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    if review.project_revision != expected_project_revision || review.status != "ready" {
        return Err(RepositoryError::StaleProjectRevision {
            expected: expected_project_revision,
            actual: review.project_revision,
        });
    }
    let checks: serde_json::Value = serde_json::from_str(&review.checks_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    if checks.as_array().is_none_or(|checks| {
        checks.iter().any(|check| {
            check.get("blocking").and_then(serde_json::Value::as_bool) == Some(true)
                && check.get("passed").and_then(serde_json::Value::as_bool) != Some(true)
        })
    }) {
        return Err(RepositoryError::ReviewChecksFailed);
    }
    let head = object_head(connection, project_id, &review.object_id)?;
    if head.0 != review.object_kind
        || head.1.as_deref() != Some(review.revision_id.as_str())
        || head.3
    {
        return Err(if head.3 {
            RepositoryError::ObjectLocked
        } else {
            RepositoryError::StaleCandidate {
                expected: Some(review.revision_id),
                actual: head
                    .1
                    .map(crate::RevisionId::parse)
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptState)?,
            }
        });
    }
    if review.object_kind == "identity" {
        if review.proposals_json != "[]" || selections_json != "{}" {
            return Err(RepositoryError::InvalidContent(
                "identity review cannot change facts or selections".into(),
            ));
        }
        crate::identity_decisions::validate_identity_acceptance(
            connection,
            project_id,
            &review.object_id,
            &review.revision_id,
        )?;
    }
    if review.object_kind == "propertyRules" {
        if review.proposals_json != "[]" || selections_json != "{}" {
            return Err(RepositoryError::InvalidContent(
                "property rule review cannot change facts or selections".into(),
            ));
        }
        let content: String = connection.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",
            params![project_id.as_str(),review.object_id.as_str(),review.revision_id.as_str()], |row|row.get(0),
        ).map_err(storage_error)?;
        crate::property_rules::validate_write(&review.object_id, &content)?;
    }
    if review.object_kind == "commitment" {
        if review.proposals_json != "[]" || selections_json != "{}" {
            return Err(RepositoryError::InvalidContent(
                "commitment review cannot change Canon facts".into(),
            ));
        }
        let content: String = connection.query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",
            params![project_id.as_str(),review.object_id.as_str(),review.revision_id.as_str()], |row|row.get(0),
        ).map_err(storage_error)?;
        crate::narrative_commitments::validate_write(
            connection,
            project_id,
            &review.object_id,
            &content,
        )?;
    }
    if review.object_kind == "draft" {
        let choices: serde_json::Value = serde_json::from_str(selections_json)
            .map_err(|_| RepositoryError::InvalidContent("invalid review selections".into()))?;
        chapter::validate_choices(connection, &review, &choices)?;
        // Knowledge publication does not advance the workspace revision. Bind the
        // prepared check and any explicit submitted batch inside this transaction.
        let current_batch: Option<String> = connection.query_row(
            "SELECT b.batch_id FROM story_knowledge_heads_v2 h JOIN story_modeling_batches_v2 b ON b.project_id=h.project_id AND b.object_id=h.object_id AND b.batch_id=h.batch_id WHERE h.project_id=?1 AND h.object_id=?2 AND b.source_revision_id=?3 AND b.status='ready'",
            params![project_id.as_str(), review.object_id.as_str(), review.revision_id.as_str()],
            |row| row.get(0),
        ).optional().map_err(storage_error)?;
        let prepared: serde_json::Value = serde_json::from_str(&review.selections_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let prepared_batch = prepared
            .get("modelingBatchId")
            .map(|value| {
                value
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or(RepositoryError::CorruptState)
            })
            .transpose()?;
        if prepared_batch != current_batch.as_deref()
            || choices
                .get("modelingBatchId")
                .is_some_and(|batch| batch.as_str() != current_batch.as_deref())
        {
            return Err(RepositoryError::InvalidContent(
                "故事设定分析已变化，请重新准备发布检查".into(),
            ));
        }
    }
    crate::modeling_review::validate_confirmed_modeling_proposals(
        connection,
        project_id,
        &review.object_id,
        &review.revision_id,
        &review.proposals_json,
        selections_json,
    )?;
    Ok(ValidatedReview {
        review,
        previous_accepted_revision_id: head.2,
    })
}

/// Record one member of a formal acceptance; caller owns the snapshot and workspace CAS.
pub(crate) fn record_review_acceptance(
    connection: &Connection,
    validated: &ValidatedReview,
    write: AcceptanceWrite<'_>,
) -> Result<(), RepositoryError> {
    let review = &validated.review;
    if review.object_kind == "propertyRules" && write.branch_id.as_str() != "main" {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let result_revision = write.base_revision + 1;
    let accepted_revision = if review.object_kind == "draft" {
        let choices = serde_json::from_str(write.selections_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        chapter::materialize(
            connection,
            review,
            validated.previous_accepted_revision_id.as_deref(),
            &choices,
        )?
    } else {
        review.revision_id.clone()
    };
    let updated = connection.execute(
        "UPDATE story_object_heads_v2 SET accepted_revision_id = ?4 WHERE project_id = ?1 AND object_id = ?2 AND candidate_revision_id = ?3 AND locked = 0",
        params![write.project_id.as_str(), review.object_id.as_str(), review.revision_id.as_str(), accepted_revision.as_str()],
    ).map_err(storage_error)?;
    if updated != 1 {
        return Err(RepositoryError::CorruptState);
    }
    let updated = connection.execute(
        "UPDATE story_reviews_v2 SET status = 'accepted', selections_json = ?1 WHERE project_id = ?2 AND review_id = ?3 AND status = 'ready'",
        params![write.selections_json, write.project_id.as_str(), review.review_id],
    ).map_err(storage_error)?;
    if updated != 1 {
        return Err(RepositoryError::CorruptState);
    }
    connection.execute(
        "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, review_id, object_id, object_kind, accepted_revision_id, previous_accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'accept')",
        params![write.commit_id.as_str(), write.project_id.as_str(), write.branch_id.as_str(), review.review_id, review.object_id.as_str(), review.object_kind, accepted_revision.as_str(), validated.previous_accepted_revision_id, write.base_revision, result_revision, write.base_snapshot_id.as_str(), write.result_snapshot_id.as_str()],
    ).map_err(storage_error)?;
    crate::impacts::resolve_linked_impacts_on_accept(
        connection,
        write.project_id,
        &review.object_kind,
        &review.revision_id,
        result_revision,
    )?;
    apply_confirmed_facts(
        connection,
        write.project_id,
        write.commit_id,
        result_revision,
        &review.proposals_json,
        write.selections_json,
    )?;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
