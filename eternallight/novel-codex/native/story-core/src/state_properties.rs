use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::RepositoryError;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PropertySchema {
    pub cardinality: Cardinality,
    pub value_type: ValueType,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Cardinality {
    Single,
    Multiple,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ValueType {
    Boolean,
    Integer,
    Text,
    Entity { kind: String },
}

/// A source-bound attribute interpretation; the schema never grants world-rule authority.
pub(crate) fn schema(assertion: &Value) -> Result<PropertySchema, RepositoryError> {
    let fields = assertion.as_object().ok_or_else(invalid)?;
    if assertion["kind"] != "state"
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "kind"
                    | "subjectId"
                    | "predicate"
                    | "value"
                    | "stateSchema"
                    | "validFrom"
                    | "validUntilExclusive"
                    | "validUntilEvidence"
                    | "evidence"
            )
        })
        || assertion["subjectId"]
            .as_str()
            .is_none_or(|id| id.trim().is_empty() || id.len() > 160)
        || assertion["predicate"]
            .as_str()
            .is_none_or(|id| id.trim().is_empty() || id.len() > 80)
        || !evidence_shape_valid(&assertion["evidence"])
        || assertion.to_string().len() > 8000
    {
        return Err(invalid());
    }
    let schema = parse_schema(&assertion["stateSchema"])?;
    let start = assertion.get("validFrom").ok_or_else(invalid)?;
    let interval_valid = (start.is_i64() || start.is_null())
        && match (
            assertion.get("validUntilExclusive"),
            assertion.get("validUntilEvidence"),
        ) {
            (None, None) => true,
            (Some(end), Some(evidence)) => {
                start
                    .as_i64()
                    .zip(end.as_i64())
                    .is_some_and(|(start, end)| start < end)
                    && evidence_shape_valid(evidence)
            }
            (None, Some(_)) | (Some(_), None) => false,
        };
    if !value_valid(&schema, &assertion["value"]) || !interval_valid {
        return Err(invalid());
    }
    Ok(schema)
}

pub(crate) fn parse_schema(value: &Value) -> Result<PropertySchema, RepositoryError> {
    let schema: PropertySchema = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let type_fields = value["valueType"].as_object().ok_or_else(invalid)?;
    if matches!(&schema.value_type, ValueType::Entity { kind } if !entity_kind_valid(kind))
        || type_fields.len()
            != if matches!(schema.value_type, ValueType::Entity { .. }) {
                2
            } else {
                1
            }
    {
        return Err(invalid());
    }
    Ok(schema)
}

pub(crate) fn entity_kind_valid(kind: &str) -> bool {
    matches!(
        kind,
        "character" | "organization" | "location" | "item" | "proposition" | "event"
    )
}

pub(crate) fn value_valid(schema: &PropertySchema, value: &Value) -> bool {
    match &schema.value_type {
        ValueType::Boolean => value.is_boolean(),
        ValueType::Integer => value.is_i64(),
        ValueType::Text => value
            .as_str()
            .is_some_and(|value| !value.trim().is_empty() && value.len() <= 512),
        ValueType::Entity { .. } => value.as_object().is_some_and(|value| {
            value.len() == 1
                && value
                    .get("entityId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.trim().is_empty() && id.len() <= 160)
        }),
    }
}

pub(crate) fn validate(
    assertion: &Value,
    chars: &[char],
    entities: &HashMap<&str, &str>,
) -> Result<(), RepositoryError> {
    let schema = schema(assertion)?;
    if !entities.contains_key(assertion["subjectId"].as_str().ok_or_else(invalid)?) {
        return Err(invalid());
    }
    if let ValueType::Entity { kind } = schema.value_type
        && entities
            .get(
                assertion["value"]["entityId"]
                    .as_str()
                    .ok_or_else(invalid)?,
            )
            .copied()
            != Some(kind.as_str())
    {
        return Err(invalid());
    }
    crate::modeling::validate_evidence(&assertion["evidence"], chars)?;
    if let Some(evidence) = assertion.get("validUntilEvidence") {
        crate::modeling::validate_evidence(evidence, chars)?;
    }
    Ok(())
}

pub(crate) fn evidence_shape_valid(evidence: &Value) -> bool {
    let Some(fields) = evidence.as_object() else {
        return false;
    };
    let Some(quote) = evidence["quote"].as_str() else {
        return false;
    };
    let length = quote.chars().count();
    fields.len() == 3
        && length > 0
        && length <= 2000
        && evidence["start"]
            .as_u64()
            .zip(evidence["end"].as_u64())
            .is_some_and(|(start, end)| end > start && end - start == length as u64)
}

/// Entity-valued properties participate in the same derived endpoint index as relations.
pub(crate) fn target_entity(assertion: &Value) -> Option<&str> {
    if assertion["kind"] == "state" {
        (assertion["stateSchema"]["valueType"]["type"] == "entity")
            .then(|| assertion["value"]["entityId"].as_str())
            .flatten()
    } else {
        assertion["targetId"].as_str()
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent(
        "state property requires a typed schema, valid interval and source-bound entities".into(),
    )
}
