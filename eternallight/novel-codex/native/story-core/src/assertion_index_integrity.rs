use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

type ClaimKey = (String, i64);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Claim {
    subject_id: Option<String>,
    target_id: Option<String>,
    owner_id: Option<String>,
    assertion: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionIndexAudit {
    pub published_batches: usize,
    pub expected_claims: usize,
    pub missing_claims: usize,
    pub extra_claims: usize,
    pub changed_claims: usize,
    pub missing_dependencies: usize,
    pub extra_dependencies: usize,
    pub affected_batch_ids: Vec<String>,
}

impl AssertionIndexAudit {
    pub fn complete(&self) -> bool {
        self.missing_claims == 0
            && self.extra_claims == 0
            && self.changed_claims == 0
            && self.missing_dependencies == 0
            && self.extra_dependencies == 0
    }

    pub(crate) fn empty() -> Self {
        Self {
            published_batches: 0,
            expected_claims: 0,
            missing_claims: 0,
            extra_claims: 0,
            changed_claims: 0,
            missing_dependencies: 0,
            extra_dependencies: 0,
            affected_batch_ids: Vec::new(),
        }
    }
}

impl SqliteStoryRepository {
    /// Compares the task-search lookup to every published source batch, including stale history.
    pub fn audit_assertion_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<AssertionIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let audit = audit_on(&read, project_id)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }

    /// Replaces only derived assertion rows; malformed source rolls back the transaction.
    pub fn rebuild_assertion_index(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<AssertionIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (_, expected) = expected_on(&tx, project_id)?;
        tx.execute(
            "DELETE FROM story_assertion_claims_v1 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
        for ((batch_id, index), claim) in expected {
            tx.execute(
                "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, owner_id, assertion_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![project_id.as_str(), batch_id, index, claim.subject_id, claim.target_id,
                    claim.owner_id, claim.assertion.to_string()],
            ).map_err(storage_error)?;
        }
        crate::model_dependencies::rebuild(&tx, project_id)?;
        let audit = audit_on(&tx, project_id)?;
        if !audit.complete() {
            return Err(RepositoryError::CorruptState);
        }
        crate::knowledge_audit_jobs::bump_knowledge_generation(&tx, project_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(audit)
    }
}

pub(crate) fn audit_assertion_batch_on(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
    audit: &mut AssertionIndexAudit,
) -> Result<(), RepositoryError> {
    let assertions = validated_assertions(connection, project_id, batch_id)?;
    let expected = assertions
        .into_iter()
        .enumerate()
        .map(|(index, assertion)| {
            (
                index as i64,
                Claim {
                    subject_id: assertion["subjectId"].as_str().map(str::to_owned),
                    target_id: crate::state_properties::target_entity(&assertion)
                        .map(str::to_owned),
                    owner_id: assertion["epistemicOwnerId"].as_str().map(str::to_owned),
                    assertion,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut statement = connection.prepare(
        "SELECT assertion_index, subject_id, target_id, owner_id, assertion_json FROM story_assertion_claims_v1 WHERE project_id = ?1 AND batch_id = ?2",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(params![project_id.as_str(), batch_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let (index, subject_id, target_id, owner_id, payload) = row.map_err(storage_error)?;
        actual.insert(
            index,
            Claim {
                subject_id,
                target_id,
                owner_id,
                assertion: serde_json::from_str(&payload).unwrap_or(Value::Null),
            },
        );
    }
    audit.published_batches += 1;
    audit.expected_claims += expected.len();
    let mut affected = false;
    for (index, claim) in &expected {
        match actual.get(index) {
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
    for index in actual.keys() {
        if !expected.contains_key(index) {
            audit.extra_claims += 1;
            affected = true;
        }
    }
    let (missing, extra) =
        crate::model_dependencies::differences(connection, project_id, batch_id)?;
    audit.missing_dependencies += missing;
    audit.extra_dependencies += extra;
    if affected || missing != 0 || extra != 0 {
        record_affected(audit, batch_id);
    }
    Ok(())
}

pub(crate) fn audit_assertion_orphans_on(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut AssertionIndexAudit,
) -> Result<(), RepositoryError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM story_assertion_claims_v1 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.project_id = ?1 AND b.batch_id = c.batch_id AND b.published_at IS NOT NULL)",
        [project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    audit.extra_dependencies += crate::model_dependencies::orphan_count(connection, project_id)?;
    audit.extra_claims += count as usize;
    let mut statement = connection.prepare(
        "SELECT DISTINCT c.batch_id FROM story_assertion_claims_v1 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.project_id = ?1 AND b.batch_id = c.batch_id AND b.published_at IS NOT NULL) ORDER BY c.batch_id LIMIT 20",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    for row in rows {
        record_affected(audit, &row.map_err(storage_error)?);
    }
    Ok(())
}

fn record_affected(audit: &mut AssertionIndexAudit, batch_id: &str) {
    if !audit.affected_batch_ids.iter().any(|id| id == batch_id) {
        audit.affected_batch_ids.push(batch_id.to_owned());
        audit.affected_batch_ids.sort();
        audit.affected_batch_ids.truncate(20);
    }
}

fn audit_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<AssertionIndexAudit, RepositoryError> {
    let (published_batches, expected) = expected_on(connection, project_id)?;
    let mut statement = connection.prepare(
        "SELECT batch_id, assertion_index, subject_id, target_id, owner_id, assertion_json FROM story_assertion_claims_v1 WHERE project_id = ?1",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let (batch_id, index, subject_id, target_id, owner_id, payload) =
            row.map_err(storage_error)?;
        actual.insert(
            (batch_id, index),
            Claim {
                subject_id,
                target_id,
                owner_id,
                assertion: serde_json::from_str(&payload).unwrap_or(Value::Null),
            },
        );
    }
    let mut audit = AssertionIndexAudit {
        published_batches,
        expected_claims: expected.len(),
        missing_claims: 0,
        extra_claims: 0,
        changed_claims: 0,
        missing_dependencies: 0,
        extra_dependencies: 0,
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
    let mut batches = connection.prepare("SELECT batch_id FROM story_modeling_batches_v2 WHERE project_id=?1 AND published_at IS NOT NULL").map_err(storage_error)?;
    for batch_id in batches
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?
    {
        let batch_id = batch_id.map_err(storage_error)?;
        let (missing, extra) =
            crate::model_dependencies::differences(connection, project_id, &batch_id)?;
        audit.missing_dependencies += missing;
        audit.extra_dependencies += extra;
        if missing != 0 || extra != 0 {
            affected.insert(batch_id);
        }
    }
    audit.extra_dependencies += crate::model_dependencies::orphan_count(connection, project_id)?;
    audit.affected_batch_ids = affected.into_iter().take(20).collect();
    Ok(audit)
}

fn expected_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(usize, BTreeMap<ClaimKey, Claim>), RepositoryError> {
    let mut statement = connection.prepare(
        "SELECT batch_id FROM story_modeling_batches_v2 WHERE project_id = ?1 AND published_at IS NOT NULL ORDER BY batch_id",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    let mut expected = BTreeMap::new();
    let mut batches = 0;
    for row in rows {
        let batch_id = row.map_err(storage_error)?;
        let assertions = validated_assertions(connection, project_id, &batch_id)?;
        for (index, assertion) in assertions.into_iter().enumerate() {
            expected.insert(
                (batch_id.clone(), index as i64),
                Claim {
                    subject_id: assertion["subjectId"].as_str().map(str::to_owned),
                    target_id: crate::state_properties::target_entity(&assertion)
                        .map(str::to_owned),
                    owner_id: assertion["epistemicOwnerId"].as_str().map(str::to_owned),
                    assertion,
                },
            );
        }
        batches += 1;
    }
    Ok((batches, expected))
}

fn validated_assertions(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
) -> Result<Vec<Value>, RepositoryError> {
    let scope = crate::model_sources::ModelSourceScope::Published;
    let source = crate::model_sources::validated_source(connection, project_id, scope, batch_id)?
        .ok_or(RepositoryError::CorruptState)?;
    crate::event_sources::validate_batch(
        connection,
        project_id,
        scope,
        batch_id,
        &source.assertions,
        &source.entities,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    Ok(source.assertions)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "assertion_index_integrity_tests.rs"]
mod tests;
