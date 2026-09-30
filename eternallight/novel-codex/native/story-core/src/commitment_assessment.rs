use std::collections::HashSet;

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

use crate::plan_commitments::CommitmentRef;
use crate::{ProjectId, RepositoryError};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Assessment {
    reference: Value,
    reference_state: String,
    outcome: String,
    rationale: String,
    actual_event_refs: Vec<Value>,
}

/// Validates a candidate interpretation and returns deviation/evidence flags for
/// the existing scene verdict. It never changes a NarrativeCommitment status.
pub(crate) fn validate(
    db: &Connection,
    project: &ProjectId,
    chapter: i64,
    planned: &Value,
    effect: &Value,
) -> Result<(bool, bool), RepositoryError> {
    let invalid = || RepositoryError::TaskOutputForbidden;
    let expected = planned["commitmentRefs"].as_array().ok_or_else(invalid)?;
    let evaluated = effect["commitmentAssessments"]
        .as_array()
        .ok_or_else(invalid)?;
    let events = effect["actualEventRefs"].as_array().ok_or_else(invalid)?;
    if expected.len() > 4 || evaluated.len() != expected.len() {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    let mut deviation = false;
    let mut insufficient = false;
    for item in evaluated {
        let assessment: Assessment = serde_json::from_value(item.clone()).map_err(|_| invalid())?;
        if !expected.contains(&assessment.reference)
            || !seen.insert(CommitmentRef::parse(&assessment.reference)?)
            || assessment.rationale.trim().is_empty()
            || assessment.rationale.chars().count() > 1000
            || assessment.actual_event_refs.len() > 12
        {
            return Err(invalid());
        }
        let input =
            crate::scene_commitment_context::resolve(db, project, &assessment.reference, chapter)?;
        if input["referenceState"] != assessment.reference_state
            || (assessment.reference_state != "current"
                && assessment.outcome != "insufficientEvidence")
        {
            return Err(invalid());
        }
        match assessment.outcome.as_str() {
            "advanced" | "partiallyFulfilled" | "fulfilled" => {
                if assessment.actual_event_refs.is_empty() {
                    return Err(invalid());
                }
            }
            "contradicted" => {
                if assessment.actual_event_refs.is_empty() {
                    return Err(invalid());
                }
                deviation = true;
            }
            "insufficientEvidence" => insufficient = true,
            "unchanged" => {}
            _ => return Err(invalid()),
        }
        let mut evidence_seen = HashSet::new();
        for reference in &assessment.actual_event_refs {
            if !events.contains(reference) || !evidence_seen.insert(reference.to_string()) {
                return Err(invalid());
            }
        }
    }
    Ok((deviation, insufficient))
}

pub(crate) fn inputs_current(
    db: &Connection,
    project: &ProjectId,
    chapter: i64,
    content: &Value,
) -> Result<bool, RepositoryError> {
    for scene in content["assessments"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
    {
        for effect in scene["effectAssessments"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
        {
            for assessment in effect["commitmentAssessments"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?
            {
                let input = crate::scene_commitment_context::resolve(
                    db,
                    project,
                    &assessment["reference"],
                    chapter,
                )?;
                if input["referenceState"] != assessment["referenceState"] {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
