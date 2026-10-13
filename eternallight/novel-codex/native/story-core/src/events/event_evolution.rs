use std::collections::HashMap;

use serde_json::{Value, json};

use crate::RepositoryError;

/// Explain declared effects without executing them or granting causal authority.
/// Event and effect confirmations are resolved independently from the same fact ledger.
pub(crate) fn assess(
    event: &Value,
    batch_id: &str,
    temporal: &Value,
    conditions: &Value,
    confirmed: &HashMap<(String, usize), String>,
) -> Result<Value, RepositoryError> {
    let references = event["value"]["effects"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let timings = temporal["effects"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    if references.len() > 16 || references.len() != timings.len() {
        return Err(RepositoryError::CorruptState);
    }
    let mut effects = Vec::new();
    for (index, reference) in references.iter().enumerate() {
        let assertion_index = reference["assertionIndex"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .ok_or(RepositoryError::CorruptState)?;
        if !reference["sourceBatchId"].is_null() || timings[index]["referenceIndex"] != index {
            return Err(RepositoryError::CorruptState);
        }
        let fact_id = confirmed.get(&(batch_id.to_owned(), assertion_index));
        let compatibility = match conditions["result"].as_str() {
            Some("supported") => "supported",
            Some("refuted") => "refuted",
            Some("both") => "conflicted",
            Some("unknown") => "unknown",
            _ => return Err(RepositoryError::CorruptState),
        };
        let interpretation = match timings[index]["timing"].as_str() {
            Some("beforeEvent") => "preexistingClaim",
            Some("atEvent") => "coincidentClaim",
            Some("afterEvent") => "laterClaim",
            Some("unknownTime") => "unknownTime",
            Some("unsupportedEffectKind") => "unsupportedKind",
            _ => return Err(RepositoryError::CorruptState),
        };
        effects.push(json!({"referenceIndex":index,"interpretation":interpretation,
            "conditionCompatibility":if conditions["status"] == "noApplicableRule" {"unruled"} else {compatibility},
            "confirmationState":if fact_id.is_some() {"confirmed"} else {"candidate"},"confirmedFactId":fact_id}));
    }
    let report = json!({"basis":"declaredSourceEffectLinks","effects":effects,
        "stateChangesExecuted":false,"causalityCertified":false,"worldTruthCertified":false});
    if report.to_string().len() > 6144 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(report)
}

#[cfg(test)]
#[path = "event_evolution_tests.rs"]
mod tests;
