use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

type ClaimKey = (String, i64);

#[derive(Clone, Debug, Eq, PartialEq)]
struct EpistemicClaim {
    owner_id: String,
    proposition_id: String,
    proposition_source_batch_id: String,
    state: String,
    time: Option<i64>,
    assertion: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EpistemicIndexAudit {
    pub published_batches: usize,
    pub expected_claims: usize,
    pub missing_claims: usize,
    pub extra_claims: usize,
    pub changed_claims: usize,
    pub affected_batch_ids: Vec<String>,
}

impl EpistemicIndexAudit {
    pub fn complete(&self) -> bool {
        self.missing_claims == 0 && self.extra_claims == 0 && self.changed_claims == 0
    }

    pub(crate) fn empty() -> Self {
        Self {
            published_batches: 0,
            expected_claims: 0,
            missing_claims: 0,
            extra_claims: 0,
            changed_claims: 0,
            affected_batch_ids: Vec::new(),
        }
    }
}



pub(crate) fn audit_epistemic_batch_on(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
    assertions_json: &str,
    entities_json: &str,
    audit: &mut EpistemicIndexAudit,
) -> Result<(), RepositoryError> {
    let expected = expected_batch(
        connection,
        project_id,
        batch_id,
        assertions_json,
        entities_json,
    )?;
    let mut statement = connection.prepare(
        "SELECT assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json FROM story_epistemic_claims_v1 WHERE project_id = ?1 AND batch_id = ?2",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(params![project_id.as_str(), batch_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let (index, owner_id, proposition_id, proposition_source_batch_id, state, time, payload) =
            row.map_err(storage_error)?;
        actual.insert(
            (batch_id.to_owned(), index),
            EpistemicClaim {
                owner_id,
                proposition_id,
                proposition_source_batch_id,
                state,
                time,
                assertion: serde_json::from_str(&payload).unwrap_or(Value::Null),
            },
        );
    }
    audit.published_batches += 1;
    audit.expected_claims += expected.len();
    let mut affected = false;
    for (key, claim) in &expected {
        match actual.get(key) {
            None => {
                audit.missing_claims += 1;
                affected = true;
            }
            Some(actual) if actual != claim => {
                audit.changed_claims += 1;
                affected = true;
            }
            Some(_) => {}
        }
    }
    for key in actual.keys() {
        if !expected.contains_key(key) {
            audit.extra_claims += 1;
            affected = true;
        }
    }
    if affected {
        record_affected(audit, batch_id);
    }
    Ok(())
}

pub(crate) fn audit_epistemic_orphans_on(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut EpistemicIndexAudit,
) -> Result<(), RepositoryError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM story_epistemic_claims_v1 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.project_id = ?1 AND b.batch_id = c.batch_id AND b.published_at IS NOT NULL)",
        [project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    audit.extra_claims += count as usize;
    let mut statement = connection.prepare(
        "SELECT DISTINCT c.batch_id FROM story_epistemic_claims_v1 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.project_id = ?1 AND b.batch_id = c.batch_id AND b.published_at IS NOT NULL) ORDER BY c.batch_id LIMIT 20",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    for row in rows {
        record_affected(audit, &row.map_err(storage_error)?);
    }
    Ok(())
}

fn record_affected(audit: &mut EpistemicIndexAudit, batch_id: &str) {
    if !audit.affected_batch_ids.iter().any(|id| id == batch_id) {
        audit.affected_batch_ids.push(batch_id.to_owned());
        audit.affected_batch_ids.sort();
        audit.affected_batch_ids.truncate(20);
    }
}

fn audit_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<EpistemicIndexAudit, RepositoryError> {
    let (published_batches, expected) = expected_on(connection, project_id)?;
    let mut statement = connection.prepare(
        "SELECT batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json FROM story_epistemic_claims_v1 WHERE project_id = ?1 ORDER BY batch_id, assertion_index",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let (
            batch_id,
            index,
            owner_id,
            proposition_id,
            proposition_source_batch_id,
            state,
            time,
            payload,
        ) = row.map_err(storage_error)?;
        actual.insert(
            (batch_id, index),
            EpistemicClaim {
                owner_id,
                proposition_id,
                proposition_source_batch_id,
                state,
                time,
                assertion: serde_json::from_str(&payload).unwrap_or(Value::Null),
            },
        );
    }
    let mut audit = EpistemicIndexAudit {
        published_batches,
        expected_claims: expected.len(),
        missing_claims: 0,
        extra_claims: 0,
        changed_claims: 0,
        affected_batch_ids: Vec::new(),
    };
    let mut affected = BTreeSet::new();
    for (key, claim) in &expected {
        match actual.get(key) {
            None => {
                audit.missing_claims += 1;
                affected.insert(key.0.clone());
            }
            Some(actual) if actual != claim => {
                audit.changed_claims += 1;
                affected.insert(key.0.clone());
            }
            Some(_) => {}
        }
    }
    for key in actual.keys() {
        if !expected.contains_key(key) {
            audit.extra_claims += 1;
            affected.insert(key.0.clone());
        }
    }
    audit.affected_batch_ids = affected.into_iter().take(20).collect();
    Ok(audit)
}

fn expected_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(usize, BTreeMap<ClaimKey, EpistemicClaim>), RepositoryError> {
    let mut statement = connection.prepare(
        "SELECT batch_id, assertions_json, entities_json FROM story_modeling_batches_v2 WHERE project_id = ?1 AND published_at IS NOT NULL ORDER BY batch_id",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(storage_error)?;
    let mut expected = BTreeMap::new();
    let mut batches = 0;
    for row in rows {
        let (batch_id, assertions_json, entities_json) = row.map_err(storage_error)?;
        expected.extend(expected_batch(
            connection,
            project_id,
            &batch_id,
            &assertions_json,
            &entities_json,
        )?);
        batches += 1;
    }
    Ok((batches, expected))
}

fn expected_batch(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
    assertions_json: &str,
    entities_json: &str,
) -> Result<BTreeMap<ClaimKey, EpistemicClaim>, RepositoryError> {
    let assertions: Vec<Value> =
        serde_json::from_str(assertions_json).map_err(|_| RepositoryError::CorruptState)?;
    let entities: Vec<Value> =
        serde_json::from_str(entities_json).map_err(|_| RepositoryError::CorruptState)?;
    if assertions.len() > 100 || entities.len() > 100 {
        return Err(RepositoryError::CorruptState);
    }
    let kinds = entities
        .iter()
        .filter_map(|entity| Some((entity["id"].as_str()?, entity["kind"].as_str()?)))
        .collect::<BTreeMap<_, _>>();
    let mut expected = BTreeMap::new();
    for (index, assertion) in assertions.into_iter().enumerate() {
        if assertion["kind"] != "epistemic" {
            continue;
        }
        let owner = assertion["epistemicOwnerId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let proposition = assertion["propositionId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let state = assertion["epistemicState"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let time = assertion["epistemicTime"].as_i64();
        let proposition_source = match assertion.get("propositionSourceBatchId") {
            None => batch_id,
            Some(value) => value.as_str().ok_or(RepositoryError::CorruptState)?,
        };
        let proposition_valid = if proposition_source == batch_id {
            assertion.get("propositionSourceBatchId").is_none()
                && kinds.get(proposition) == Some(&"proposition")
        } else {
            !kinds.contains_key(proposition)
                && crate::proposition_sources::proposition_source_visible(
                    connection,
                    project_id,
                    crate::model_sources::ModelSourceScope::Published,
                    proposition_source,
                    proposition,
                )?
        };
        if kinds.get(owner) != Some(&"character")
            || !proposition_valid
            || assertion["subjectId"] != proposition
            || !matches!(state, "known" | "suspected" | "false")
            || !crate::epistemic_time::epistemic_interval_valid(&assertion)
            || assertion
                .get("epistemicTime")
                .is_some_and(|value| !value.is_null() && !value.is_i64())
        {
            return Err(RepositoryError::CorruptState);
        }
        expected.insert(
            (batch_id.to_owned(), index as i64),
            EpistemicClaim {
                owner_id: owner.to_owned(),
                proposition_id: proposition.to_owned(),
                proposition_source_batch_id: proposition_source.to_owned(),
                state: state.to_owned(),
                time,
                assertion,
            },
        );
    }
    Ok(expected)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "epistemic_index_integrity_tests.rs"]
mod tests;

/// Public epistemic index integrity capability; SQLite retains transaction ownership.
pub trait EpistemicIndexIntegrityRepository {
    /// Audits the derived character/proposition lookup against every published batch.
    fn audit_epistemic_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<EpistemicIndexAudit, RepositoryError>;
    /// Rebuilds only epistemic lookup rows; malformed source rolls back the whole transaction.
    fn rebuild_epistemic_index(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<EpistemicIndexAudit, RepositoryError>;
}

impl EpistemicIndexIntegrityRepository for SqliteStoryRepository {
    /// Audits the derived character/proposition lookup against every published batch.
    fn audit_epistemic_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<EpistemicIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let audit = audit_on(&read, project_id)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }

    /// Rebuilds only epistemic lookup rows; malformed source rolls back the whole transaction.
    fn rebuild_epistemic_index(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<EpistemicIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (published_batches, expected) = expected_on(&tx, project_id)?;
        tx.execute(
            "DELETE FROM story_epistemic_claims_v1 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
        for ((batch_id, index), claim) in expected {
            tx.execute(
                "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![project_id.as_str(), batch_id, index, claim.owner_id, claim.proposition_id,
                    claim.proposition_source_batch_id, claim.state, claim.time, claim.assertion.to_string()],
            ).map_err(storage_error)?;
        }
        let audit = audit_on(&tx, project_id)?;
        if audit.published_batches != published_batches || !audit.complete() {
            return Err(RepositoryError::CorruptState);
        }
        crate::knowledge_audit_jobs::bump_knowledge_generation(&tx, project_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(audit)
    }
}

use crate::ProjectRevisionRepository;
