use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

type ClaimKey = (String, String);
type NameKey = (String, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Claim {
    kind: String,
    display_name: String,
    aliases: Value,
    evidence: Value,
}

struct ExpectedEntityIndex {
    published_batches: usize,
    claims: BTreeMap<ClaimKey, Claim>,
    names: BTreeSet<NameKey>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityIndexAudit {
    pub published_batches: usize,
    pub expected_claims: usize,
    pub expected_names: usize,
    pub missing_claims: usize,
    pub extra_claims: usize,
    pub changed_claims: usize,
    pub missing_names: usize,
    pub extra_names: usize,
    pub affected_batch_ids: Vec<String>,
}

impl EntityIndexAudit {
    pub fn complete(&self) -> bool {
        self.missing_claims == 0
            && self.extra_claims == 0
            && self.changed_claims == 0
            && self.missing_names == 0
            && self.extra_names == 0
    }

    pub(crate) fn empty() -> Self {
        Self {
            published_batches: 0,
            expected_claims: 0,
            expected_names: 0,
            missing_claims: 0,
            extra_claims: 0,
            changed_claims: 0,
            missing_names: 0,
            extra_names: 0,
            affected_batch_ids: Vec::new(),
        }
    }
}

impl SqliteStoryRepository {
    /// Checks both derived identity tables against all published modeling payloads, including
    /// stale historical batches that can still be referenced by frozen snapshots.
    pub fn audit_entity_index(
        &self,
        project_id: &ProjectId,
    ) -> Result<EntityIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let audit = audit_on(&read, project_id)?;
        read.commit().map_err(storage_error)?;
        Ok(audit)
    }

    /// Atomically replaces only derived identity lookup rows; source batches and author decisions
    /// are never changed. A corrupt source payload aborts without deleting existing lookup rows.
    pub fn rebuild_entity_index(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<EntityIndexAudit, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let ExpectedEntityIndex { claims, names, .. } = expected_on(&tx, project_id)?;
        tx.execute(
            "DELETE FROM story_entity_names_v2 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM story_entity_claims_v2 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .map_err(storage_error)?;
        for ((batch_id, entity_id), claim) in claims {
            tx.execute(
                "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![project_id.as_str(), batch_id, entity_id, claim.kind, claim.display_name,
                    claim.aliases.to_string(), claim.evidence.to_string()],
            )
            .map_err(storage_error)?;
        }
        for (batch_id, entity_id, name) in names {
            tx.execute(
                "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, ?2, ?3, ?4)",
                params![project_id.as_str(), batch_id, entity_id, name],
            )
            .map_err(storage_error)?;
        }
        let audit = audit_on(&tx, project_id)?;
        if !audit.complete() {
            return Err(RepositoryError::CorruptState);
        }
        let next_generation = entity_generation_on(&tx, project_id)?
            .checked_add(1)
            .ok_or_else(|| {
                RepositoryError::InvalidContent("entity index generation exhausted".into())
            })?;
        tx.execute(
            "INSERT INTO story_entity_index_state_v1(project_id, generation) VALUES (?1, ?2) ON CONFLICT(project_id) DO UPDATE SET generation = excluded.generation",
            params![project_id.as_str(), next_generation],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(audit)
    }
}

pub(crate) fn entity_generation_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT generation FROM story_entity_index_state_v1 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)
        .map(|value| value.unwrap_or(0))
}

fn expected_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<ExpectedEntityIndex, RepositoryError> {
    let mut statement = connection
        .prepare("SELECT batch_id, entities_json FROM story_modeling_batches_v2 WHERE project_id = ?1 AND published_at IS NOT NULL ORDER BY batch_id")
        .map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    let mut count = 0;
    let mut claims = BTreeMap::new();
    let mut names = BTreeSet::new();
    for row in rows {
        let (batch_id, entities_json) = row.map_err(storage_error)?;
        let batch = expected_batch(&batch_id, &entities_json)?;
        count += 1;
        claims.extend(batch.claims);
        names.extend(batch.names);
    }
    Ok(ExpectedEntityIndex {
        published_batches: count,
        claims,
        names,
    })
}

fn expected_batch(
    batch_id: &str,
    entities_json: &str,
) -> Result<ExpectedEntityIndex, RepositoryError> {
    let entities: Vec<Value> =
        serde_json::from_str(entities_json).map_err(|_| RepositoryError::CorruptState)?;
    if entities.len() > 100 {
        return Err(RepositoryError::CorruptState);
    }
    let mut claims = BTreeMap::new();
    let mut names = BTreeSet::new();
    for entity in entities {
        let id = entity["id"].as_str().ok_or(RepositoryError::CorruptState)?;
        let kind = entity["kind"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let display_name = entity["displayName"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        // The current publication contract allows omission, meaning no aliases.
        let aliases = match entity.get("aliases") {
            None => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or(RepositoryError::CorruptState)?
                .clone(),
        };
        let evidence = entity
            .get("evidence")
            .ok_or(RepositoryError::CorruptState)?;
        let claim = Claim {
            kind: kind.into(),
            display_name: display_name.into(),
            aliases: Value::Array(aliases.clone()),
            evidence: evidence.clone(),
        };
        if claims
            .insert((batch_id.to_owned(), id.into()), claim)
            .is_some()
        {
            return Err(RepositoryError::CorruptState);
        }
        let alias_names = aliases
            .iter()
            .map(|value| value.as_str().ok_or(RepositoryError::CorruptState))
            .collect::<Result<Vec<_>, _>>()?;
        for name in std::iter::once(display_name).chain(alias_names) {
            let name = name.trim().to_lowercase();
            if !name.is_empty() {
                names.insert((batch_id.to_owned(), id.into(), name));
            }
        }
    }
    Ok(ExpectedEntityIndex {
        published_batches: 1,
        claims,
        names,
    })
}

pub(crate) fn audit_entity_batch_on(
    connection: &Connection,
    project_id: &ProjectId,
    batch_id: &str,
    entities_json: &str,
    audit: &mut EntityIndexAudit,
) -> Result<(), RepositoryError> {
    let expected = expected_batch(batch_id, entities_json)?;
    let mut statement = connection.prepare(
        "SELECT entity_id, kind, display_name, aliases_json, evidence_json FROM story_entity_claims_v2 WHERE batch_id = ?1 AND project_id = ?2",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map(params![batch_id, project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual_claims = BTreeMap::new();
    for row in rows {
        let (entity_id, kind, display_name, aliases, evidence) = row.map_err(storage_error)?;
        actual_claims.insert(
            (batch_id.to_owned(), entity_id),
            Claim {
                kind,
                display_name,
                aliases: serde_json::from_str(&aliases).unwrap_or(Value::Null),
                evidence: serde_json::from_str(&evidence).unwrap_or(Value::Null),
            },
        );
    }
    let mut statement = connection.prepare(
        "SELECT entity_id, name FROM story_entity_names_v2 WHERE batch_id = ?1 AND project_id = ?2",
    ).map_err(storage_error)?;
    let actual_names = statement
        .query_map(params![batch_id, project_id.as_str()], |row| {
            Ok((
                batch_id.to_owned(),
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        })
        .map_err(storage_error)?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(storage_error)?;
    audit.published_batches += 1;
    audit.expected_claims += expected.claims.len();
    audit.expected_names += expected.names.len();
    let mut affected = false;
    for (key, claim) in &expected.claims {
        match actual_claims.get(key) {
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
    for key in actual_claims.keys() {
        if !expected.claims.contains_key(key) {
            audit.extra_claims += 1;
            affected = true;
        }
    }
    for _ in expected.names.difference(&actual_names) {
        audit.missing_names += 1;
        affected = true;
    }
    for _ in actual_names.difference(&expected.names) {
        audit.extra_names += 1;
        affected = true;
    }
    if affected {
        record_affected_batch(audit, batch_id);
    }
    Ok(())
}

pub(crate) fn audit_entity_orphans_on(
    connection: &Connection,
    project_id: &ProjectId,
    audit: &mut EntityIndexAudit,
) -> Result<(), RepositoryError> {
    let orphan_claims: i64 = connection.query_row(
        "SELECT COUNT(*) FROM story_entity_claims_v2 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id = c.batch_id AND b.project_id = ?1 AND b.published_at IS NOT NULL)",
        [project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    let orphan_names: i64 = connection.query_row(
        "SELECT COUNT(*) FROM story_entity_names_v2 n WHERE n.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id = n.batch_id AND b.project_id = ?1 AND b.published_at IS NOT NULL)",
        [project_id.as_str()], |row| row.get(0),
    ).map_err(storage_error)?;
    audit.extra_claims += orphan_claims as usize;
    audit.extra_names += orphan_names as usize;
    let mut statement = connection.prepare(
        "SELECT batch_id FROM (\
           SELECT c.batch_id FROM story_entity_claims_v2 c WHERE c.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id = c.batch_id AND b.project_id = ?1 AND b.published_at IS NOT NULL) \
           UNION SELECT n.batch_id FROM story_entity_names_v2 n WHERE n.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM story_modeling_batches_v2 b WHERE b.batch_id = n.batch_id AND b.project_id = ?1 AND b.published_at IS NOT NULL)\
         ) ORDER BY batch_id LIMIT 20",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    for row in rows {
        record_affected_batch(audit, &row.map_err(storage_error)?);
    }
    Ok(())
}

fn record_affected_batch(audit: &mut EntityIndexAudit, batch_id: &str) {
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
) -> Result<EntityIndexAudit, RepositoryError> {
    let ExpectedEntityIndex {
        published_batches,
        claims: expected_claims,
        names: expected_names,
    } = expected_on(connection, project_id)?;
    let mut statement = connection
        .prepare("SELECT batch_id, entity_id, kind, display_name, aliases_json, evidence_json FROM story_entity_claims_v2 WHERE project_id = ?1")
        .map_err(storage_error)?;
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(storage_error)?;
    let mut actual_claims = BTreeMap::new();
    for row in rows {
        let (batch_id, entity_id, kind, display_name, aliases, evidence) =
            row.map_err(storage_error)?;
        actual_claims.insert(
            (batch_id, entity_id),
            Claim {
                kind,
                display_name,
                // A malformed derived row is a repairable mismatch, not a corrupt source.
                aliases: serde_json::from_str(&aliases).unwrap_or(Value::Null),
                evidence: serde_json::from_str(&evidence).unwrap_or(Value::Null),
            },
        );
    }
    let mut statement = connection
        .prepare(
            "SELECT batch_id, entity_id, name FROM story_entity_names_v2 WHERE project_id = ?1",
        )
        .map_err(storage_error)?;
    let actual_names = statement
        .query_map([project_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(storage_error)?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(storage_error)?;
    let mut affected = BTreeSet::new();
    let mut audit = EntityIndexAudit {
        published_batches,
        expected_claims: expected_claims.len(),
        expected_names: expected_names.len(),
        missing_claims: 0,
        extra_claims: 0,
        changed_claims: 0,
        missing_names: 0,
        extra_names: 0,
        affected_batch_ids: Vec::new(),
    };
    for (key, expected) in &expected_claims {
        match actual_claims.get(key) {
            None => audit.missing_claims += 1,
            Some(actual) if actual != expected => audit.changed_claims += 1,
            Some(_) => continue,
        }
        affected.insert(key.0.clone());
    }
    for key in actual_claims.keys() {
        if !expected_claims.contains_key(key) {
            audit.extra_claims += 1;
            affected.insert(key.0.clone());
        }
    }
    for key in expected_names.difference(&actual_names) {
        audit.missing_names += 1;
        affected.insert(key.0.clone());
    }
    for key in actual_names.difference(&expected_names) {
        audit.extra_names += 1;
        affected.insert(key.0.clone());
    }
    audit.affected_batch_ids = affected.into_iter().take(20).collect();
    Ok(audit)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "entity_index_integrity_tests.rs"]
mod tests;
