use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::RepositoryError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Participant {
    role: String,
    entity_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EventValue {
    participants: Vec<Participant>,
    preconditions: Vec<EventClaimRef>,
    effects: Vec<EventClaimRef>,
    until_exclusive: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventClaimRef {
    pub source_batch_id: Option<String>,
    pub assertion_index: usize,
}

pub(crate) fn preconditions(assertion: &Value) -> Result<Vec<EventClaimRef>, RepositoryError> {
    Ok(parse(assertion)?.preconditions)
}

fn parse(assertion: &Value) -> Result<EventValue, RepositoryError> {
    let fields = assertion.as_object().ok_or_else(invalid)?;
    if !crate::state_properties::evidence_shape_valid(&assertion["evidence"]) {
        return Err(RepositoryError::InvalidContent(
            "event evidence requires {quote,start,end} from the exact source text; end is exclusive and end-start must equal the Unicode character count of quote".into(),
        ));
    }
    if assertion["kind"] != "event"
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "kind" | "subjectId" | "predicate" | "value" | "storyTime" | "evidence"
            )
        })
        || assertion["subjectId"]
            .as_str()
            .is_none_or(|id| id.trim().is_empty() || id.len() > 160)
        || assertion["predicate"]
            .as_str()
            .is_none_or(|kind| kind.trim().is_empty() || kind.len() > 80)
        || assertion
            .get("storyTime")
            .is_none_or(|time| !time.is_null() && !time.is_i64())
        || assertion.to_string().len() > 8_000
    {
        return Err(invalid());
    }
    if assertion["value"]
        .as_object()
        .is_none_or(|fields| fields.len() != 4)
    {
        return Err(RepositoryError::InvalidContent(
            "event value requires participants, preconditions, effects and untilExclusive; use empty arrays and null when no links or end time are known".into(),
        ));
    }
    let event: EventValue =
        serde_json::from_value(assertion["value"].clone()).map_err(|_| invalid())?;
    if event.participants.is_empty()
        || event.participants.len() > 16
        || event.preconditions.len() > 16
        || event.effects.len() > 16
        || event.until_exclusive.is_some_and(|end| {
            assertion["storyTime"]
                .as_i64()
                .is_none_or(|start| end <= start)
        })
    {
        return Err(invalid());
    }
    let mut participants = HashSet::new();
    for participant in &event.participants {
        if role_kinds(&participant.role).is_empty()
            || participant.entity_id.trim().is_empty()
            || participant.entity_id.len() > 160
            || !participants.insert((&participant.role, &participant.entity_id))
        {
            return Err(invalid());
        }
    }
    let mut references = HashSet::new();
    if ["preconditions", "effects"].iter().any(|field| {
        assertion["value"][field].as_array().is_none_or(|refs| {
            refs.iter().any(|reference| {
                reference
                    .as_object()
                    .is_none_or(|fields| fields.len() != 2 || !fields.contains_key("sourceBatchId"))
            })
        })
    }) || event
        .effects
        .iter()
        .any(|reference| reference.source_batch_id.is_some())
    {
        return Err(invalid());
    }
    if event
        .preconditions
        .iter()
        .chain(&event.effects)
        .any(|reference| {
            reference.assertion_index >= 100
                || reference
                    .source_batch_id
                    .as_ref()
                    .is_some_and(|id| id.trim().is_empty() || id.len() > 160)
                || !references.insert(reference)
        })
    {
        return Err(invalid());
    }
    Ok(event)
}

pub(crate) fn role_kinds(role: &str) -> &'static [&'static str] {
    match role {
        "actor" | "recipient" | "beneficiary" => &["character", "organization"],
        "affected" => &["character", "organization", "location", "item"],
        "location" => &["location"],
        "instrument" | "item" => &["item"],
        "witness" => &["character"],
        _ => &[],
    }
}

/// Validate source-local event identity and interpretations, without executing their effects.
pub(crate) fn validate_batch(
    assertions: &[Value],
    entities: &[Value],
) -> Result<(), RepositoryError> {
    validate_links(assertions)?;
    let mut kinds = HashMap::new();
    for entity in entities {
        let id = entity["id"].as_str().ok_or_else(invalid)?;
        let kind = entity["kind"].as_str().ok_or_else(invalid)?;
        if kinds.insert(id, kind).is_some() {
            return Err(invalid());
        }
    }
    for assertion in assertions.iter().filter(|claim| claim["kind"] == "event") {
        let event = parse(assertion)?;
        if kinds.get(assertion["subjectId"].as_str().ok_or_else(invalid)?) != Some(&"event")
            || event.participants.iter().any(|participant| {
                kinds
                    .get(participant.entity_id.as_str())
                    .is_none_or(|kind| !role_kinds(&participant.role).contains(kind))
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// References are ordinals in this immutable batch, never global IDs or new state authority.
fn validate_links(assertions: &[Value]) -> Result<(), RepositoryError> {
    for assertion in assertions.iter().filter(|claim| claim["kind"] == "event") {
        validate_event_links(assertion, assertions)?;
    }
    Ok(())
}

/// Validate one event without repeatedly walking unrelated events in the same batch.
pub(crate) fn validate_event_links(
    assertion: &Value,
    assertions: &[Value],
) -> Result<(), RepositoryError> {
    let event = parse(assertion)?;
    let start = assertion["evidence"]["start"]
        .as_u64()
        .ok_or_else(invalid)?;
    let end = assertion["evidence"]["end"].as_u64().ok_or_else(invalid)?;
    for reference in event.preconditions.iter().chain(&event.effects) {
        if reference.source_batch_id.is_some() {
            continue;
        }
        let claim = assertions
            .get(reference.assertion_index)
            .ok_or_else(invalid)?;
        validate_reference_subject(assertion, claim)?;
        if !crate::state_properties::evidence_shape_valid(&claim["evidence"])
            || claim["evidence"]["start"]
                .as_u64()
                .is_none_or(|offset| offset < start)
            || claim["evidence"]["end"]
                .as_u64()
                .is_none_or(|offset| offset > end)
        {
            return Err(invalid());
        }
        for key in ["validUntilEvidence", "epistemicUntilEvidence"] {
            if let Some(evidence) = claim.get(key)
                && (!crate::state_properties::evidence_shape_valid(evidence)
                    || evidence["start"]
                        .as_u64()
                        .is_none_or(|offset| offset < start)
                    || evidence["end"].as_u64().is_none_or(|offset| offset > end))
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_reference_subject(
    event: &Value,
    claim: &Value,
) -> Result<(), RepositoryError> {
    if claim["kind"] == "state" {
        crate::state_properties::schema(claim)?;
    }
    let subject = if claim["kind"] == "epistemic" {
        &claim["epistemicOwnerId"]
    } else {
        &claim["subjectId"]
    };
    if !matches!(
        claim["kind"].as_str(),
        Some("assertion" | "state" | "relation" | "epistemic")
    ) || !event["value"]["participants"]
        .as_array()
        .is_some_and(|participants| {
            participants.iter().any(|participant| {
                subject
                    .as_str()
                    .is_some_and(|id| participant["entityId"] == id)
            })
        })
    {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn validate(assertion: &Value, chars: &[char]) -> Result<(), RepositoryError> {
    parse(assertion)?;
    crate::modeling::validate_evidence(&assertion["evidence"], chars)
}

pub(crate) fn participant_ids(assertion: &Value) -> impl Iterator<Item = &str> {
    assertion["value"]["participants"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |_| assertion["kind"] == "event")
        .filter_map(|participant| participant["entityId"].as_str())
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("event requires typed identity, participants, interval and source-bound claim references; effects must be local".into())
}

#[cfg(test)]
#[path = "story_events_tests.rs"]
mod tests;
