use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::knowledge_projection::relation_definition;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

type ClaimKey = (String, i64);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Claim {
    subject_id: String,
    target_id: String,
    relation_type: String,
    assertion: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RelationIndexAudit {
    pub published_batches: usize,
    pub expected_claims: usize,
    pub missing_claims: usize,
    pub extra_claims: usize,
    pub changed_claims: usize,
    pub affected_batch_ids: Vec<String>,
}

impl RelationIndexAudit {
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



pub(crate) fn relation_generation_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT generation FROM story_relation_index_state_v1 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)
        .map(|value| value.unwrap_or(0))
}

pub(crate) fn audit_relation_batch_on(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
    assertions_json: &str,
    entities_json: &str,
    audit: &mut RelationIndexAudit,
) -> Result<(), RepositoryError> {
    let expected = expected_batch(batch_id, assertions_json, entities_json)?;
    let mut statement = connection.prepare(
        "SELECT assertion_index, subject_id, target_id, relation_type, assertion_json FROM story_relation_claims_v1 WHERE project_id = ?1 AND batch_id = ?2"
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(params![project_id.as_str(), batch_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let (index, subject_id, target_id, relation_type, payload) = row.map_err(storage_error)?;
        actual.insert(
            (batch_id.to_owned(), index),
            Claim {
                subject_id,
                target_id,
                relation_type,
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

pub(crate) fn audit_relation_orphans_on(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut RelationIndexAudit,
) -> Result<(), RepositoryError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM story_relation_claims_v1 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id = c.batch_id AND b.project_id = ?1 AND b.published_at IS NOT NULL)",
        [project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    audit.extra_claims += count as usize;
    let mut statement = connection.prepare(
        "SELECT DISTINCT c.batch_id FROM story_relation_claims_v1 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id = c.batch_id AND b.project_id = ?1 AND b.published_at IS NOT NULL) ORDER BY c.batch_id LIMIT 20"
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    for row in rows {
        record_affected(audit, &row.map_err(storage_error)?);
    }
    Ok(())
}

fn record_affected(audit: &mut RelationIndexAudit, batch_id: &str) {
    if audit.affected_batch_ids.iter().any(|id| id == batch_id) {
        return;
    }
    audit.affected_batch_ids.push(batch_id.to_owned());
    audit.affected_batch_ids.sort();
    audit.affected_batch_ids.truncate(20);
}

fn audit_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<RelationIndexAudit, RepositoryError> {
    let (published_batches, expected) = expected_on(connection, project_id)?;
    let mut statement = connection.prepare(
        "SELECT batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json FROM story_relation_claims_v1 WHERE project_id = ?1 ORDER BY batch_id, assertion_index"
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
            ))
        })
        .map_err(storage_error)?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let (batch_id, index, subject_id, target_id, relation_type, assertion_json) =
            row.map_err(storage_error)?;
        actual.insert(
            (batch_id, index),
            Claim {
                subject_id,
                target_id,
                relation_type,
                assertion: serde_json::from_str(&assertion_json).unwrap_or(Value::Null),
            },
        );
    }
    let mut audit = RelationIndexAudit::empty();
    audit.published_batches = published_batches;
    audit.expected_claims = expected.len();
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
    for batch_id in affected {
        record_affected(&mut audit, &batch_id);
    }
    Ok(audit)
}

fn expected_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(usize, BTreeMap<ClaimKey, Claim>), RepositoryError> {
    let mut statement = connection.prepare(
        "SELECT batch_id, assertions_json, entities_json FROM story_modeling_batches_v2 WHERE project_id = ?1 AND published_at IS NOT NULL ORDER BY batch_id"
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
    let mut published_batches = 0;
    for row in rows {
        let (batch_id, assertions_json, entities_json) = row.map_err(storage_error)?;
        expected.extend(expected_batch(&batch_id, &assertions_json, &entities_json)?);
        published_batches += 1;
    }
    Ok((published_batches, expected))
}

fn expected_batch(
    batch_id: &str,
    assertions_json: &str,
    entities_json: &str,
) -> Result<BTreeMap<ClaimKey, Claim>, RepositoryError> {
    let assertions: Vec<Value> =
        serde_json::from_str(assertions_json).map_err(|_| RepositoryError::CorruptState)?;
    let entities: Vec<Value> =
        serde_json::from_str(entities_json).map_err(|_| RepositoryError::CorruptState)?;
    let propositions = entities
        .iter()
        .filter(|entity| entity["kind"] == "proposition")
        .filter_map(|entity| entity["id"].as_str())
        .collect::<BTreeSet<_>>();
    if assertions.len() > 100 {
        return Err(RepositoryError::CorruptState);
    }
    let mut expected = BTreeMap::new();
    for (index, assertion) in assertions.into_iter().enumerate() {
        match assertion["kind"].as_str() {
            Some("assertion" | "event" | "epistemic") => continue,
            Some("relation") => {}
            _ => return Err(RepositoryError::CorruptState),
        }
        let subject_id = assertion["subjectId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let target_id = assertion["targetId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let relation_type = assertion["relationType"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        if subject_id.is_empty()
            || target_id.is_empty()
            || assertion["propositionId"]
                .as_str()
                .is_none_or(|id| !propositions.contains(id))
            || relation_definition(relation_type).is_none()
            || assertion["predicate"] != relation_type
            || assertion.get("evidence").is_none()
        {
            return Err(RepositoryError::CorruptState);
        }
        let claim = Claim {
            subject_id: subject_id.to_owned(),
            target_id: target_id.to_owned(),
            relation_type: relation_type.to_owned(),
            assertion,
        };
        expected.insert((batch_id.to_owned(), index as i64), claim);
    }
    Ok(expected)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "relation_index_integrity_tests.rs"]
mod tests;

/// Public relation index integrity capability; SQLite retains transaction ownership.
pub trait RelationIndexIntegrityRepository {
    /// Checks derived relation rows against every published batch, including historical stale ones.
    fn audit_relation_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<RelationIndexAudit, RepositoryError>;
    /// Rebuilds only the indexed relation projection in one transaction; malformed source aborts.
    fn rebuild_relation_index(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<RelationIndexAudit, RepositoryError>;
}

impl RelationIndexIntegrityRepository for SqliteStoryRepository {
    /// Checks derived relation rows against every published batch, including historical stale ones.
    fn audit_relation_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<RelationIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let audit = audit_on(&read, project_id)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }

    /// Rebuilds only the indexed relation projection in one transaction; malformed source aborts.
    fn rebuild_relation_index(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<RelationIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (published_batches, expected) = expected_on(&tx, project_id)?;
        tx.execute(
            "DELETE FROM story_relation_claims_v1 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
        for ((batch_id, index), claim) in expected {
            tx.execute(
                "INSERT INTO story_relation_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![project_id.as_str(), batch_id, index, claim.subject_id, claim.target_id,
                    claim.relation_type, claim.assertion.to_string()],
            ).map_err(storage_error)?;
        }
        let audit = audit_on(&tx, project_id)?;
        if !audit.complete() || audit.published_batches != published_batches {
            return Err(RepositoryError::CorruptState);
        }
        let next_generation = relation_generation_on(&tx, project_id)?
            .checked_add(1)
            .ok_or_else(|| {
                RepositoryError::InvalidContent("relation index generation exhausted".into())
            })?;
        tx.execute(
            "INSERT INTO story_relation_index_state_v1(project_id, generation) VALUES (?1, ?2) ON CONFLICT(project_id) DO UPDATE SET generation = excluded.generation",
            params![project_id.as_str(), next_generation],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(audit)
    }
}

use crate::ProjectRevisionRepository;
