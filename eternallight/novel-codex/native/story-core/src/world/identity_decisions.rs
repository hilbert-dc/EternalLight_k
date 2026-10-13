use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{ObjectId, ProjectId, RepositoryError, RevisionId, SnapshotId};

const MAX_DECISIONS: usize = 256;

pub(crate) struct IdentityResolution {
    pub revision_id: Option<String>,
    pub decisions: Value,
    same: HashMap<String, String>,
    distinct: Vec<(String, String)>,
}

impl IdentityResolution {
    pub fn unresolved() -> Self {
        Self {
            revision_id: None,
            decisions: json!([]),
            same: HashMap::new(),
            distinct: Vec::new(),
        }
    }

    pub fn canonical_id(&self, id: &str) -> String {
        let mut current = id;
        for _ in 0..=MAX_DECISIONS {
            let Some(next) = self.same.get(current) else {
                return current.to_owned();
            };
            current = next;
        }
        // Accepted payloads are validated before commit; a cycle here is corrupt state.
        current.to_owned()
    }

    pub fn raw_ids_for_canonical(&self, canonical_id: &str) -> Vec<String> {
        let canonical = self.canonical_id(canonical_id);
        let mut ids = self
            .same
            .keys()
            .filter(|id| self.canonical_id(id) == canonical)
            .cloned()
            .collect::<Vec<_>>();
        ids.push(canonical);
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn explicitly_distinct(&self, left: &str, right: &str) -> bool {
        let left = self.canonical_id(left);
        let right = self.canonical_id(right);
        self.distinct.iter().any(|(a, b)| {
            (self.canonical_id(a) == left && self.canonical_id(b) == right)
                || (self.canonical_id(a) == right && self.canonical_id(b) == left)
        })
    }
}

pub(crate) fn validate_identity_content(content_json: &str) -> Result<(), RepositoryError> {
    let content: Value = serde_json::from_str(content_json)
        .map_err(|_| RepositoryError::InvalidContent("invalid identity artifact".into()))?;
    parse_decisions(&content["payload"])?;
    Ok(())
}

pub(crate) fn validate_identity_acceptance(
    connection: &Connection,
    project_id: &ProjectId,
    object_id: &ObjectId,
    revision_id: &RevisionId,
) -> Result<(), RepositoryError> {
    if object_id.as_str() != "artifact-identity" {
        return Err(RepositoryError::InvalidObjectKind);
    }
    let content: String = connection
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3",
            params![project_id.as_str(), object_id.as_str(), revision_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    let content: Value =
        serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
    let resolution = parse_decisions(&content["payload"])?;
    let kinds = current_entity_kinds(connection, project_id)?;
    for (source, target) in &resolution.same {
        if !kinds.contains_key(source) || kinds.get(source) != kinds.get(target) {
            return Err(RepositoryError::InvalidContent(
                "identity merge requires two active entities of the same kind".into(),
            ));
        }
    }
    for (left, right) in &resolution.distinct {
        if !kinds.contains_key(left) || kinds.get(left) != kinds.get(right) {
            return Err(RepositoryError::InvalidContent(
                "identity distinction requires two active entities of the same kind".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn identity_resolution(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<IdentityResolution, RepositoryError> {
    let accepted: Option<(String, String)> = connection
        .query_row(
            "SELECT r.revision_id, r.content_json FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.project_id = h.project_id AND r.object_id = h.object_id AND r.revision_id = h.accepted_revision_id WHERE h.project_id = ?1 AND h.object_id = 'artifact-identity' AND h.object_kind = 'identity'",
            [project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((revision_id, content)) = accepted else {
        return Ok(IdentityResolution::unresolved());
    };
    resolution_from_content(revision_id, &content)
}

pub(crate) fn identity_resolution_at_snapshot(
    connection: &Connection,
    project_id: &ProjectId,
    snapshot_id: &SnapshotId,
) -> Result<IdentityResolution, RepositoryError> {
    let latest: Option<(String, Option<String>, Option<String>)> = connection.query_row(
        "WITH RECURSIVE ancestors(snapshot_id) AS (\
           SELECT ?2 UNION ALL SELECT s.parent_snapshot_id FROM story_snapshots_v2 s \
           JOIN ancestors a ON a.snapshot_id = s.snapshot_id WHERE s.parent_snapshot_id IS NOT NULL\
         ) SELECT a.operation, a.accepted_revision_id, target.previous_accepted_revision_id \
         FROM story_acceptances_v2 a LEFT JOIN story_acceptances_v2 target ON target.commit_id = a.target_commit_id \
         WHERE a.project_id = ?1 AND a.object_kind = 'identity' \
           AND a.result_snapshot_id IN (SELECT snapshot_id FROM ancestors) \
         ORDER BY a.result_revision DESC LIMIT 1",
        params![project_id.as_str(), snapshot_id.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(storage_error)?;
    let revision_id = match latest {
        Some((operation, accepted, _)) if operation == "accept" => accepted,
        Some((operation, _, previous)) if operation == "undo" => previous,
        None => None,
        _ => return Err(RepositoryError::CorruptState),
    };
    let Some(revision_id) = revision_id else {
        return Ok(IdentityResolution::unresolved());
    };
    let content: String = connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = 'artifact-identity' AND revision_id = ?2",
        params![project_id.as_str(), revision_id], |row| row.get(0),
    ).map_err(storage_error)?;
    resolution_from_content(revision_id, &content)
}

fn resolution_from_content(
    revision_id: String,
    content: &str,
) -> Result<IdentityResolution, RepositoryError> {
    let content: Value =
        serde_json::from_str(content).map_err(|_| RepositoryError::CorruptState)?;
    let mut resolution =
        parse_decisions(&content["payload"]).map_err(|_| RepositoryError::CorruptState)?;
    resolution.revision_id = Some(revision_id);
    Ok(resolution)
}

fn parse_decisions(payload: &Value) -> Result<IdentityResolution, RepositoryError> {
    let decisions = payload["decisions"]
        .as_array()
        .ok_or_else(invalid_decisions)?;
    if payload.as_object().is_none_or(|object| object.len() != 1) || decisions.len() > MAX_DECISIONS
    {
        return Err(invalid_decisions());
    }
    let mut same = HashMap::new();
    let mut distinct = Vec::new();
    let mut distinct_seen = HashSet::new();
    for decision in decisions {
        let fields = decision.as_object().ok_or_else(invalid_decisions)?;
        let (left_key, right_key) = match decision["kind"].as_str() {
            Some("same") if fields.len() == 3 => ("fromId", "toId"),
            Some("distinct") if fields.len() == 3 => ("leftId", "rightId"),
            _ => return Err(invalid_decisions()),
        };
        if !fields.contains_key(left_key) || !fields.contains_key(right_key) {
            return Err(invalid_decisions());
        }
        let left = decision[left_key].as_str().ok_or_else(invalid_decisions)?;
        let right = decision[right_key].as_str().ok_or_else(invalid_decisions)?;
        if left.is_empty()
            || right.is_empty()
            || left.len() > 160
            || right.len() > 160
            || left == right
        {
            return Err(invalid_decisions());
        }
        if decision["kind"] == "same" {
            if same.insert(left.to_owned(), right.to_owned()).is_some() {
                return Err(invalid_decisions());
            }
        } else {
            let pair = if left < right {
                (left.to_owned(), right.to_owned())
            } else {
                (right.to_owned(), left.to_owned())
            };
            if !distinct_seen.insert(pair.clone()) {
                return Err(invalid_decisions());
            }
            distinct.push(pair);
        }
    }
    for start in same.keys() {
        let mut current = start.as_str();
        let mut visited = HashSet::new();
        while let Some(next) = same.get(current) {
            if !visited.insert(current) {
                return Err(invalid_decisions());
            }
            current = next;
        }
    }
    let resolution = IdentityResolution {
        revision_id: None,
        decisions: Value::Array(decisions.clone()),
        same,
        distinct,
    };
    if resolution
        .distinct
        .iter()
        .any(|(left, right)| resolution.canonical_id(left) == resolution.canonical_id(right))
    {
        return Err(invalid_decisions());
    }
    Ok(resolution)
}

fn current_entity_kinds(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<HashMap<String, String>, RepositoryError> {
    let mut statement = connection.prepare(
        "SELECT c.entity_id, c.kind FROM story_entity_claims_v2 c JOIN story_knowledge_heads_v2 h ON h.project_id = c.project_id AND h.batch_id = c.batch_id JOIN story_modeling_batches_v2 b ON b.project_id = c.project_id AND b.batch_id = c.batch_id WHERE c.project_id = ?1 AND b.status = 'ready'",
    ).map_err(storage_error)?;
    let mut kinds = HashMap::new();
    let rows = statement
        .query_map([project_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    for row in rows {
        let (id, kind) = row.map_err(storage_error)?;
        if kinds
            .insert(id.clone(), kind.clone())
            .is_some_and(|old| old != kind)
        {
            return Err(RepositoryError::CorruptState);
        }
    }
    Ok(kinds)
}

fn invalid_decisions() -> RepositoryError {
    RepositoryError::InvalidContent("identity decisions are invalid".into())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
