use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::RepositoryError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TemporalReference {
    reference_index: usize,
    timing: Timing,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Timing {
    ApplicableBeforeEvent,
    InactiveBeforeEvent,
    StartsAtEvent,
    BeforeEvent,
    AtEvent,
    AfterEvent,
    UnknownTime,
    UnsupportedConditionKind,
    UnsupportedEffectKind,
}

enum Side {
    Precondition,
    Effect,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TemporalAssessment {
    basis: &'static str,
    preconditions: Vec<TemporalReference>,
    effects: Vec<TemporalReference>,
    semantic_truth_inferred: bool,
    global_conditions_verified: bool,
    state_changes_executed: bool,
}

/// Explain source-bound temporal references, never execute effects or certify conditions.
pub(crate) fn assess(
    event: &Value,
    assertions: &[Value],
    external: &HashMap<crate::story_events::EventClaimRef, Value>,
) -> Result<Value, RepositoryError> {
    crate::story_events::validate_event_links(event, assertions)
        .map_err(|_| RepositoryError::CorruptState)?;
    let time = event["storyTime"].as_i64();
    let mut preconditions = Vec::new();
    let mut effects = Vec::new();
    for (field, side, output) in [
        ("preconditions", Side::Precondition, &mut preconditions),
        ("effects", Side::Effect, &mut effects),
    ] {
        let indices = event["value"][field]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        for (reference_index, reference) in indices.iter().enumerate() {
            let reference: crate::story_events::EventClaimRef =
                serde_json::from_value(reference.clone())
                    .map_err(|_| RepositoryError::CorruptState)?;
            let claim = if reference.source_batch_id.is_some() {
                external.get(&reference)
            } else {
                assertions.get(reference.assertion_index)
            }
            .ok_or(RepositoryError::CorruptState)?;
            output.push(TemporalReference {
                reference_index,
                timing: classify(claim, time, &side)?,
            });
        }
    }
    let result = serde_json::to_value(TemporalAssessment {
        basis: "sourceBoundTemporalReferences",
        preconditions,
        effects,
        semantic_truth_inferred: false,
        global_conditions_verified: false,
        state_changes_executed: false,
    })
    .map_err(|_| RepositoryError::CorruptState)?;
    if result.to_string().len() > 3_000 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(result)
}

fn classify(
    claim: &Value,
    event_time: Option<i64>,
    side: &Side,
) -> Result<Timing, RepositoryError> {
    if claim["kind"] != "state" {
        return Ok(match side {
            Side::Precondition => Timing::UnsupportedConditionKind,
            Side::Effect => Timing::UnsupportedEffectKind,
        });
    }
    crate::state_properties::schema(claim).map_err(|_| RepositoryError::CorruptState)?;
    let (Some(event_time), Some(start)) = (event_time, claim["validFrom"].as_i64()) else {
        return Ok(Timing::UnknownTime);
    };
    Ok(match side {
        Side::Precondition if start == event_time => Timing::StartsAtEvent,
        Side::Precondition
            if start > event_time
                || claim["validUntilExclusive"]
                    .as_i64()
                    .is_some_and(|end| end < event_time) =>
        {
            Timing::InactiveBeforeEvent
        }
        Side::Precondition => Timing::ApplicableBeforeEvent,
        Side::Effect => match start.cmp(&event_time) {
            std::cmp::Ordering::Less => Timing::BeforeEvent,
            std::cmp::Ordering::Equal => Timing::AtEvent,
            std::cmp::Ordering::Greater => Timing::AfterEvent,
        },
    })
}

#[cfg(test)]
#[path = "event_conditions_tests.rs"]
mod tests;
