use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::event_rules::EventRule;
use crate::state_properties::{ValueType, entity_kind_valid, parse_schema, value_valid};
use crate::{ObjectId, ProjectId, RepositoryError, SnapshotId};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Definition {
    pub(crate) predicate: String,
    pub(crate) subject_kind: String,
    pub(crate) schema: Value,
    description: String,
    pub(crate) exceptions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RulesPayload {
    definitions: Vec<Definition>,
    event_rules: Vec<EventRule>,
}

pub(crate) struct PropertyRules {
    pub revision_id: Option<String>,
    pub(crate) definitions: Vec<Definition>,
    pub(crate) event_rules: Vec<EventRule>,
}

impl PropertyRules {
    pub fn empty() -> Self {
        Self {
            revision_id: None,
            definitions: Vec::new(),
            event_rules: Vec::new(),
        }
    }

    pub fn event_rule_count(&self) -> usize {
        self.event_rules.len()
    }

    /// Apply approved structural rules without changing the source claim or its confirmation.
    pub fn project(&self, entry: &mut Value, entities: &Value) -> Result<bool, RepositoryError> {
        let assertion = &entry["assertion"];
        let subject_kind = entity_kind(entities, &assertion["subjectId"])?;
        let Some(rule) = self.definitions.iter().find(|rule| {
            assertion["predicate"].as_str() == Some(rule.predicate.as_str())
                && rule.subject_kind == subject_kind
        }) else {
            return Ok(false);
        };
        let schema = parse_schema(&rule.schema).map_err(|_| RepositoryError::CorruptState)?;
        let valid = value_valid(&schema, &assertion["value"])
            && match &schema.value_type {
                ValueType::Entity { kind } => {
                    entity_kind(entities, &assertion["value"]["entityId"])? == kind.as_str()
                }
                ValueType::Boolean | ValueType::Integer | ValueType::Text => true,
            };
        let status = if !rule.exceptions.is_empty() {
            "requiresInterpretation"
        } else if !valid {
            "typeMismatch"
        } else if assertion["stateSchema"] != rule.schema {
            "declarationMismatch"
        } else {
            "compatible"
        };
        let applicable = valid && rule.exceptions.is_empty();
        entry["propertyRuleAssessment"] = json!({"revisionId":self.revision_id,"predicate":rule.predicate,
            "subjectKind":rule.subject_kind,"schema":rule.schema,"description":rule.description,
            "exceptions":rule.exceptions,"status":status});
        entry["effectiveStateSchema"] = if applicable {
            rule.schema.clone()
        } else {
            Value::Null
        };
        entry["stateSchemaAuthority"] = json!(if applicable {
            "authorConfirmed"
        } else {
            "modelDeclared"
        });
        Ok(!applicable)
    }
}

fn entity_kind<'a>(entities: &'a Value, id: &Value) -> Result<&'a str, RepositoryError> {
    let kind = entities
        .as_array()
        .and_then(|entities| entities.iter().find(|entity| entity["id"] == *id))
        .and_then(|entity| entity["kind"].as_str())
        .ok_or(RepositoryError::CorruptState)?;
    if !entity_kind_valid(kind) {
        return Err(RepositoryError::CorruptState);
    }
    Ok(kind)
}

pub(crate) fn validate_write(object_id: &ObjectId, content: &str) -> Result<(), RepositoryError> {
    if object_id.as_str() != "artifact-propertyRules" {
        return Err(RepositoryError::InvalidObjectKind);
    }
    validate_content(content)
}

pub(crate) fn validate_content(content: &str) -> Result<(), RepositoryError> {
    parse_content(content).map(|_| ())
}

fn parse_content(content: &str) -> Result<RulesPayload, RepositoryError> {
    let envelope: Value = serde_json::from_str(content).map_err(|_| invalid())?;
    let payload = &envelope["payload"];
    if !matches!(envelope["source"].as_str(), Some("author" | "agent"))
        || envelope["version"]
            .as_u64()
            .is_none_or(|version| version == 0)
        || envelope["baseProjectRevision"]
            .as_i64()
            .is_none_or(|revision| revision < 0)
        || payload.as_object().is_none_or(|fields| fields.len() != 2)
        || payload.to_string().len() > 16 * 1024
    {
        return Err(invalid());
    }
    let payload: RulesPayload = serde_json::from_value(payload.clone()).map_err(|_| invalid())?;
    if payload.definitions.len() > 64 {
        return Err(invalid());
    }
    let mut keys = HashSet::new();
    let mut schemas = HashMap::new();
    for rule in &payload.definitions {
        if rule.predicate.trim().is_empty()
            || rule.predicate.len() > 80
            || !entity_kind_valid(&rule.subject_kind)
            || rule.description.trim().is_empty()
            || rule.description.len() > 512
            || rule.exceptions.len() > 8
            || rule
                .exceptions
                .iter()
                .any(|text| text.trim().is_empty() || text.len() > 256)
            || !keys.insert((&rule.subject_kind, &rule.predicate))
        {
            return Err(invalid());
        }
        schemas.insert(
            (rule.subject_kind.as_str(), rule.predicate.as_str()),
            parse_schema(&rule.schema)?,
        );
    }
    crate::event_rules::validate(&payload.event_rules, &schemas)?;
    Ok(payload)
}

pub(crate) fn load(
    connection: &Connection,
    project: &ProjectId,
    snapshot: Option<&SnapshotId>,
) -> Result<PropertyRules, RepositoryError> {
    let latest: Option<(String, String, Option<String>, Option<String>)> = connection.query_row(
        "WITH RECURSIVE ancestors(snapshot_id) AS (SELECT ?2 UNION ALL SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN ancestors a ON a.snapshot_id=s.snapshot_id WHERE s.project_id=?1 AND s.parent_snapshot_id IS NOT NULL) \
         SELECT a.operation,a.accepted_revision_id,t.previous_accepted_revision_id,t.commit_id FROM story_acceptances_v2 a LEFT JOIN story_acceptances_v2 t ON t.commit_id=a.target_commit_id AND t.project_id=a.project_id AND t.object_id=a.object_id AND t.object_kind=a.object_kind AND t.branch_id=a.branch_id AND t.operation='accept' AND t.accepted_revision_id=a.previous_accepted_revision_id \
         WHERE a.project_id=?1 AND a.object_kind='propertyRules' AND a.object_id='artifact-propertyRules' AND a.branch_id='main' AND (?2 IS NULL OR a.result_snapshot_id IN (SELECT snapshot_id FROM ancestors)) ORDER BY a.result_revision DESC LIMIT 1",
        params![project.as_str(),snapshot.map(SnapshotId::as_str)], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).optional().map_err(storage)?;
    let accepted = match latest {
        Some((operation, accepted, _, _)) if operation == "accept" => Some(accepted),
        Some((operation, _, previous, Some(_))) if operation == "undo" => previous,
        None => None,
        _ => return Err(RepositoryError::CorruptState),
    };
    if snapshot.is_none() {
        let head: Option<(String,Option<String>)> = connection.query_row(
            "SELECT object_kind,accepted_revision_id FROM story_object_heads_v2 WHERE project_id=?1 AND object_id='artifact-propertyRules'",
            [project.as_str()], |row|Ok((row.get(0)?,row.get(1)?)),
        ).optional().map_err(storage)?;
        if head
            .as_ref()
            .is_some_and(|(kind, _)| kind != "propertyRules")
            || head.and_then(|(_, revision)| revision) != accepted
        {
            return Err(RepositoryError::CorruptState);
        }
    }
    let Some(revision_id) = accepted else {
        return Ok(PropertyRules::empty());
    };
    let content: String = connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id='artifact-propertyRules' AND object_kind='propertyRules' AND revision_id=?2",
        params![project.as_str(),revision_id], |row|row.get(0),
    ).map_err(storage)?;
    let payload = parse_content(&content).map_err(|_| RepositoryError::CorruptState)?;
    Ok(PropertyRules {
        revision_id: Some(revision_id),
        definitions: payload.definitions,
        event_rules: payload.event_rules,
    })
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent(
        "property rules require bounded definitions, unique typed scopes and explicit exceptions"
            .into(),
    )
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "property_rules_tests.rs"]
mod tests;
