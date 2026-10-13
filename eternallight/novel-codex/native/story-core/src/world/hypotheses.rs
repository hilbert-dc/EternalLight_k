use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::identity_decisions::identity_resolution_at_snapshot;
use crate::knowledge_conflicts::detect_conflicts;
use crate::{
    KnowledgePerspective, KnowledgeStateQuery, ProjectId, RepositoryError, SnapshotId,
    SqliteStoryRepository,
};

const MAX_ASSUMPTIONS: usize = 25;
const MAX_ASSUMPTION_BYTES: usize = 64_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum HypothesisAssumption {
    SuppressClaim {
        modeling_batch_id: String,
        source_revision_id: String,
        assertion_index: usize,
    },
    AssumeClaim {
        subject_id: String,
        predicate: String,
        value: Value,
        story_time: Option<i64>,
    },
}

pub struct CreateHypothesis<'a> {
    pub project_id: &'a ProjectId,
    pub base_snapshot_id: &'a SnapshotId,
    pub intent: &'a str,
    pub assumptions: &'a [HypothesisAssumption],
    pub idempotency_key: &'a str,
    pub now: i64,
}

impl SqliteStoryRepository {
    

    

    pub(crate) fn read_hypothesis_in_snapshot(
        &self,
        project_id: &ProjectId,
        hypothesis_id: &str,
    ) -> Result<Value, RepositoryError> {
        if !hypothesis_id.starts_with("hypothesis-") || hypothesis_id.len() > 80 {
            return Err(RepositoryError::InvalidContent(
                "invalid hypothesis id".into(),
            ));
        }
        let row = self.connection.query_row(
            "SELECT base_snapshot_id, intent, assumptions_json, created_at FROM story_hypotheses_v1 WHERE project_id = ?1 AND hypothesis_id = ?2",
            params![project_id.as_str(), hypothesis_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::CandidateNotFound)?;
        let snapshot_id = SnapshotId::parse(row.0).map_err(|_| RepositoryError::CorruptState)?;
        let assumptions: Vec<HypothesisAssumption> =
            serde_json::from_str(&row.2).map_err(|_| RepositoryError::CorruptState)?;
        let baseline = self.knowledge_state_in_snapshot(KnowledgeStateQuery {
            project_id,
            snapshot_id: Some(&snapshot_id),
            perspective: KnowledgePerspective::Author,
            world_time: None,
            epistemic_time: None,
            reading_chapter: None,
            owner_id: None,
            entity_id: None,
            predicate: None,
        })?;
        let identity = identity_resolution_at_snapshot(&self.connection, project_id, &snapshot_id)?;
        let mut entries = baseline["entries"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .clone();
        let mut suppressed = BTreeSet::new();
        for assumption in &assumptions {
            if let HypothesisAssumption::SuppressClaim {
                modeling_batch_id,
                source_revision_id,
                assertion_index,
            } = assumption
            {
                suppressed.insert((
                    modeling_batch_id.clone(),
                    source_revision_id.clone(),
                    *assertion_index,
                ));
            }
        }
        let mut found_suppressions = BTreeSet::new();
        entries.retain(|entry| {
            let key = (
                entry["modelingBatchId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                entry["sourceRevisionId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                entry["assertionIndex"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(usize::MAX),
            );
            if suppressed.contains(&key) {
                found_suppressions.insert(key);
                false
            } else {
                true
            }
        });
        for (index, assumption) in assumptions.iter().enumerate() {
            if let HypothesisAssumption::AssumeClaim {
                subject_id,
                predicate,
                value,
                story_time,
            } = assumption
            {
                entries.push(json!({
                    "sourceObjectId": null, "sourceRevisionId": null,
                    "modelingBatchId": null, "assertionIndex": null,
                    "hypothesisId": hypothesis_id, "assumptionIndex": index,
                    "canonicalSubjectId": identity.canonical_id(subject_id),
                    "confirmationState": "assumed",
                    "assertion": {"kind":"assertion", "subjectId": subject_id,
                        "predicate": predicate, "value": value, "storyTime": story_time,
                        "evidence": null}
                }));
            }
        }
        let (findings, findings_truncated, uncertain_pairs) = detect_conflicts(&entries);
        Ok(json!({
            "id": hypothesis_id, "projectId": project_id.as_str(),
            "baseSnapshotId": snapshot_id.as_str(), "intent": row.1,
            "createdAt": row.3, "assumptions": assumptions,
            "baseline": {"entryCount": baseline["entries"].as_array().ok_or(RepositoryError::CorruptState)?.len(),
                "findings": baseline["findings"], "coverage": baseline["coverage"]},
            "simulation": {"entries": entries, "findings": findings,
                "coverage": {"projectionComplete": baseline["coverage"]["complete"] == true
                    && found_suppressions.len() == suppressed.len() && !findings_truncated && uncertain_pairs == 0,
                    "unprojectedSuppressionCount": suppressed.len() - found_suppressions.len(),
                    "findingsTruncated": findings_truncated, "uncertainConflictPairs": uncertain_pairs,
                    "globalImpactComplete": false}},
            "authority": "hypothesisOnly"
        }))
    }
}

pub(crate) fn create_in_transaction(
    transaction: &Transaction<'_>,
    request: CreateHypothesis<'_>,
) -> Result<Value, RepositoryError> {
    if request.intent.trim().is_empty()
        || request.intent.chars().count() > 1_000
        || request.idempotency_key.is_empty()
        || request.idempotency_key.len() > 160
        || !(1..=MAX_ASSUMPTIONS).contains(&request.assumptions.len())
    {
        return Err(RepositoryError::InvalidContent(
            "invalid hypothesis request".into(),
        ));
    }
    let assumptions_json =
        serde_json::to_string(request.assumptions).map_err(|_| RepositoryError::CorruptState)?;
    if assumptions_json.len() > MAX_ASSUMPTION_BYTES {
        return Err(RepositoryError::InvalidContent(
            "hypothesis assumptions are too large".into(),
        ));
    }
    let digest = request_digest(
        request.base_snapshot_id.as_str(),
        request.intent,
        request.assumptions,
    );
    let existing = transaction.query_row(
            "SELECT hypothesis_id, request_digest FROM story_hypotheses_v1 WHERE project_id = ?1 AND idempotency_key = ?2",
            params![request.project_id.as_str(), request.idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?;
    if let Some((id, prior_digest)) = existing {
        if prior_digest != digest {
            return Err(RepositoryError::IdempotencyConflict);
        }
        return Ok(
            json!({"id": id, "baseSnapshotId": request.base_snapshot_id.as_str(), "replayed": true}),
        );
    }
    let valid_snapshot = transaction.query_row(
            "SELECT 1 FROM story_snapshots_v2 WHERE project_id = ?1 AND snapshot_id = ?2 AND branch_id = 'main'",
            params![request.project_id.as_str(), request.base_snapshot_id.as_str()],
            |row| row.get::<_, i64>(0),
        ).optional().map_err(storage_error)?;
    if valid_snapshot.is_none() {
        return Err(RepositoryError::CandidateNotFound);
    }
    validate_assumptions(
        transaction,
        request.project_id,
        request.base_snapshot_id,
        request.assumptions,
    )?;
    let id = format!("hypothesis-{}", uuid::Uuid::new_v4().simple());
    transaction.execute(
            "INSERT INTO story_hypotheses_v1(hypothesis_id, project_id, base_snapshot_id, intent, assumptions_json, idempotency_key, request_digest, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, request.project_id.as_str(), request.base_snapshot_id.as_str(), request.intent, assumptions_json, request.idempotency_key, digest, request.now],
        ).map_err(storage_error)?;
    Ok(json!({"id": id, "baseSnapshotId": request.base_snapshot_id.as_str(), "replayed": false}))
}

pub(crate) fn request_digest(
    snapshot: &str,
    intent: &str,
    assumptions: &[HypothesisAssumption],
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            json!({"snapshotId":snapshot,"intent":intent,"assumptions":assumptions})
                .to_string()
                .as_bytes()
        )
    )
}

fn validate_assumptions(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    snapshot_id: &SnapshotId,
    assumptions: &[HypothesisAssumption],
) -> Result<(), RepositoryError> {
    let mut seen = BTreeSet::new();
    for assumption in assumptions {
        let key = serde_json::to_string(assumption).map_err(|_| RepositoryError::CorruptState)?;
        if !seen.insert(key) {
            return Err(RepositoryError::InvalidContent(
                "duplicate hypothesis assumption".into(),
            ));
        }
        match assumption {
            HypothesisAssumption::SuppressClaim {
                modeling_batch_id,
                source_revision_id,
                assertion_index,
            } => {
                let payload = transaction.query_row(
                    "SELECT b.assertions_json FROM story_snapshot_knowledge_v2 k JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id AND b.project_id = ?1 AND b.object_id = k.object_id JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id AND m.revision_id = b.source_revision_id WHERE k.snapshot_id = ?2 AND k.batch_id = ?3 AND b.source_revision_id = ?4",
                    params![project_id.as_str(), snapshot_id.as_str(), modeling_batch_id, source_revision_id],
                    |row| row.get::<_, String>(0),
                ).optional().map_err(storage_error)?.ok_or(RepositoryError::CandidateNotFound)?;
                let claims: Vec<Value> =
                    serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
                if claims.get(*assertion_index).is_none() {
                    return Err(RepositoryError::CandidateNotFound);
                }
            }
            HypothesisAssumption::AssumeClaim {
                subject_id,
                predicate,
                value,
                ..
            } => {
                if subject_id.is_empty()
                    || subject_id.len() > 160
                    || predicate.trim().is_empty()
                    || predicate.len() > 160
                    || !(value.is_boolean()
                        || value.is_number()
                        || value.is_string()
                        || value.is_null())
                    || value.to_string().len() > 1_000
                {
                    return Err(RepositoryError::InvalidContent(
                        "invalid hypothetical claim".into(),
                    ));
                }
                let visible = transaction.query_row(
                    "SELECT 1 FROM story_entity_claims_v2 c JOIN story_snapshot_knowledge_v2 k ON k.batch_id = c.batch_id JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id AND b.object_id = k.object_id JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id AND m.revision_id = b.source_revision_id WHERE c.project_id = ?1 AND k.snapshot_id = ?2 AND c.entity_id = ?3 LIMIT 1",
                    params![project_id.as_str(), snapshot_id.as_str(), subject_id],
                    |row| row.get::<_, i64>(0),
                ).optional().map_err(storage_error)?;
                if visible.is_none() {
                    return Err(RepositoryError::CandidateNotFound);
                }
            }
        }
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "hypotheses_tests.rs"]
mod tests;

/// Public hypotheses capability; SQLite retains transaction ownership.
pub trait HypothesesRepository {
    /// Stores an immutable author hypothesis without changing the branch or Canon.
    fn create_hypothesis(
        &mut self,
        request: CreateHypothesis<'_>,
    ) -> Result<Value, RepositoryError>;
    /// Replays assumptions over the pinned historical projection, never into a formal head.
    fn read_hypothesis(
        &self,
        project_id: &ProjectId,
        hypothesis_id: &str,
    ) -> Result<Value, RepositoryError>;
}

impl HypothesesRepository for SqliteStoryRepository {
    /// Stores an immutable author hypothesis without changing the branch or Canon.
    fn create_hypothesis(
        &mut self,
        request: CreateHypothesis<'_>,
    ) -> Result<Value, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let result = create_in_transaction(&transaction, request)?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }

    /// Replays assumptions over the pinned historical projection, never into a formal head.
    fn read_hypothesis(
        &self,
        project_id: &ProjectId,
        hypothesis_id: &str,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| self.read_hypothesis_in_snapshot(project_id, hypothesis_id))?
    }
}

use crate::ReadSnapshotRepository;
