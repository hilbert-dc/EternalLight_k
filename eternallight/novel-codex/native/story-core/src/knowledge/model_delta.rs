use std::collections::{BTreeMap, BTreeSet};

use rusqlite::params;
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, RevisionId, SqliteStoryRepository};

const MAX_CANDIDATES: usize = 30;
const MAX_CANDIDATE_BYTES: usize = 64_000;

struct PublishedBatch {
    id: String,
    status: String,
    assertions: Vec<Value>,
}

enum BatchSelection {
    Available(PublishedBatch),
    Missing,
    Pending,
    Ambiguous,
}

impl BatchSelection {
    fn coverage_status(&self) -> &'static str {
        match self {
            Self::Available(_) => "available",
            Self::Missing => "missingPublishedBatch",
            Self::Pending => "pendingPublication",
            Self::Ambiguous => "ambiguousPublishedBatches",
        }
    }
}

impl SqliteStoryRepository {
    /// Compares source-bound extraction candidates; this does not establish Canon truth.
    pub(crate) fn draft_model_candidate_delta(
        &self,
        project_id: &ProjectId,
        object_id: &str,
        from_revision_id: &RevisionId,
        to_revision_id: &RevisionId,
    ) -> Result<Value, RepositoryError> {
        let from = select_batch(self, project_id, object_id, from_revision_id)?;
        let to = select_batch(self, project_id, object_id, to_revision_id)?;
        let from_status = from.coverage_status();
        let to_status = to.coverage_status();
        let (BatchSelection::Available(from), BatchSelection::Available(to)) = (from, to) else {
            return Ok(json!({
                "status": "unavailable", "candidateChanges": [],
                "coverage": {"from": from_status, "to": to_status,
                    "complete": false, "semanticTruthInferred": false}
            }));
        };
        let mut old_claims = claim_multiset(&from.assertions)?;
        let mut new_claims = claim_multiset(&to.assertions)?;
        let keys = old_claims
            .keys()
            .chain(new_claims.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut changes = Vec::new();
        let mut changed_entity_ids = BTreeSet::new();
        let mut total = 0;
        let mut evidence_reanchored = 0;
        let mut bytes = 0;
        let mut truncated = false;
        for key in keys {
            let mut old = old_claims.remove(&key).unwrap_or_default();
            let mut new = new_claims.remove(&key).unwrap_or_default();
            let mut old_index = 0;
            while old_index < old.len() {
                if let Some(new_index) =
                    new.iter().position(|(_, claim)| claim == &old[old_index].1)
                {
                    old.remove(old_index);
                    new.remove(new_index);
                } else {
                    old_index += 1;
                }
            }
            let common = old.len().min(new.len());
            evidence_reanchored += common;
            for (change, assertion, batch, revision, index) in old[common..]
                .iter()
                .map(|(index, assertion)| {
                    (
                        "noLongerExtracted",
                        assertion,
                        &from,
                        from_revision_id,
                        index,
                    )
                })
                .chain(new[common..].iter().map(|(index, assertion)| {
                    ("newlyExtracted", assertion, &to, to_revision_id, index)
                }))
            {
                total += 1;
                let candidate = json!({
                    "change": change, "claim": claim_without_evidence(assertion)?,
                    "sourceObjectId": object_id, "sourceRevisionId": revision.as_str(),
                    "modelingBatchId": batch.id, "assertionIndex": index,
                    "evidence": assertion["evidence"],
                    "validUntilEvidence": assertion.get("validUntilEvidence")
                });
                let candidate_bytes = candidate.to_string().len();
                if changes.len() >= MAX_CANDIDATES || bytes + candidate_bytes > MAX_CANDIDATE_BYTES
                {
                    truncated = true;
                    continue;
                }
                bytes += candidate_bytes;
                for id in assertion["subjectId"]
                    .as_str()
                    .into_iter()
                    .chain(crate::state_properties::target_entity(assertion))
                    .chain(crate::story_events::participant_ids(assertion))
                {
                    changed_entity_ids.insert(id.to_owned());
                }
                changes.push(candidate);
            }
        }
        Ok(json!({
            "status": "comparedCandidates",
            "fromModelingBatchId": from.id,
            "toModelingBatchId": to.id,
            "fromBatchStatus": from.status,
            "toBatchStatus": to.status,
            "candidateChanges": changes,
            "candidateChangedEntityIds": changed_entity_ids,
            "coverage": {"from": from_status, "to": to_status,
                "complete": !truncated, "truncated": truncated,
                "totalCandidateChanges": total, "semanticTruthInferred": false,
                "evidenceReanchoredClaimCount": evidence_reanchored}
        }))
    }
}

fn select_batch(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    object_id: &str,
    revision_id: &RevisionId,
) -> Result<BatchSelection, RepositoryError> {
    let mut statement = repository
        .connection
        .prepare(
            "SELECT batch_id, status, assertions_json, published_at, entities_json FROM story_modeling_batches_v2 \
         WHERE project_id = ?1 AND object_id = ?2 AND source_revision_id = ?3 \
         ORDER BY published_at DESC, batch_id LIMIT 3",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(
            params![project_id.as_str(), object_id, revision_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .map_err(storage_error)?;
    let mut published = Vec::new();
    let mut pending = false;
    for row in rows {
        let (id, status, assertions_json, published_at, entities_json) =
            row.map_err(storage_error)?;
        if published_at.is_some() {
            published.push((id, status, assertions_json, entities_json));
        } else {
            pending = true;
        }
    }
    if published.len() > 1 {
        return Ok(BatchSelection::Ambiguous);
    }
    let Some((id, status, assertions_json, entities_json)) = published.pop() else {
        return Ok(if pending {
            BatchSelection::Pending
        } else {
            BatchSelection::Missing
        });
    };
    let assertions: Vec<Value> =
        serde_json::from_str(&assertions_json).map_err(|_| RepositoryError::CorruptState)?;
    if assertions.len() > 100 || assertions.iter().any(|value| !value.is_object()) {
        return Err(RepositoryError::CorruptState);
    }
    let entities: Vec<Value> =
        serde_json::from_str(&entities_json).map_err(|_| RepositoryError::CorruptState)?;
    crate::story_events::validate_batch(&assertions, &entities)
        .map_err(|_| RepositoryError::CorruptState)?;
    crate::event_sources::validate_batch(
        &repository.connection,
        project_id,
        crate::model_sources::ModelSourceScope::Published,
        &id,
        &assertions,
        &entities,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    Ok(BatchSelection::Available(PublishedBatch {
        id,
        status,
        assertions,
    }))
}

fn claim_without_evidence(assertion: &Value) -> Result<Value, RepositoryError> {
    let mut claim = assertion.clone();
    let fields = claim.as_object_mut().ok_or(RepositoryError::CorruptState)?;
    fields.remove("evidence");
    fields.remove("validUntilEvidence");
    Ok(claim)
}

fn claim_multiset(
    assertions: &[Value],
) -> Result<BTreeMap<String, Vec<(usize, Value)>>, RepositoryError> {
    let mut claims: BTreeMap<String, Vec<(usize, Value)>> = BTreeMap::new();
    for (index, assertion) in assertions.iter().enumerate() {
        let key = claim_without_evidence(assertion)?.to_string();
        claims
            .entry(key)
            .or_default()
            .push((index, assertion.clone()));
    }
    Ok(claims)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "model_delta_tests.rs"]
mod tests;
