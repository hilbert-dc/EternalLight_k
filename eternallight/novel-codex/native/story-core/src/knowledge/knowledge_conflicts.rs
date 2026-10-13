use serde_json::{Value, json};

const MAX_FINDINGS: usize = 100;

pub(crate) fn detect_conflicts(entries: &[Value]) -> (Vec<Value>, bool, usize) {
    let mut findings = Vec::new();
    let mut truncated = false;
    let mut uncertain_pairs = 0;
    for (left_index, left) in entries.iter().enumerate() {
        let left_assertion = &left["assertion"];
        let kind = left_assertion["kind"].as_str().unwrap_or_default();
        if kind == "epistemic" {
            let owner = left["canonicalEpistemicOwnerId"]
                .as_str()
                .or_else(|| left_assertion["epistemicOwnerId"].as_str());
            let proposition = &left["propositionRef"];
            if owner.is_none()
                || proposition["batchId"].as_str().is_none()
                || proposition["propositionId"].as_str().is_none()
            {
                continue;
            }
            for (right_index, right) in entries.iter().enumerate().skip(left_index + 1) {
                let right_assertion = &right["assertion"];
                let right_owner = right["canonicalEpistemicOwnerId"]
                    .as_str()
                    .or_else(|| right_assertion["epistemicOwnerId"].as_str());
                if right_assertion["kind"] != "epistemic"
                    || owner != right_owner
                    || proposition != &right["propositionRef"]
                    || (left_assertion["epistemicState"] == right_assertion["epistemicState"]
                        && left_assertion["value"] == right_assertion["value"])
                {
                    continue;
                }
                let (Some(left_start), Some(right_start)) = (
                    left_assertion["epistemicTime"].as_i64(),
                    right_assertion["epistemicTime"].as_i64(),
                ) else {
                    uncertain_pairs += 1;
                    continue;
                };
                if left_assertion["epistemicUntilExclusive"]
                    .as_i64()
                    .is_some_and(|end| right_start >= end)
                    || right_assertion["epistemicUntilExclusive"]
                        .as_i64()
                        .is_some_and(|end| left_start >= end)
                {
                    continue;
                }
                if findings.len() == MAX_FINDINGS {
                    truncated = true;
                    continue;
                }
                findings.push(json!({
                    "type": "epistemicDisagreement", "subjectId": owner,
                    "predicate": "epistemicState", "ownerId": owner,
                    "propositionRef": proposition,
                    "leftState": left_assertion["epistemicState"],
                    "rightState": right_assertion["epistemicState"],
                    "leftEntryIndex": left_index, "rightEntryIndex": right_index,
                    "leftSourceRevisionId": left["sourceRevisionId"],
                    "rightSourceRevisionId": right["sourceRevisionId"],
                    "leftEvidence": left_assertion["evidence"],
                    "rightEvidence": right_assertion["evidence"],
                    "confirmationState": conflict_authority(left, right)
                }));
            }
            continue;
        }
        if !matches!(kind, "assertion" | "relation") {
            continue;
        }
        let Some(left_value) = left_assertion["value"].as_bool() else {
            continue;
        };
        let left_subject = left["canonicalSubjectId"]
            .as_str()
            .or_else(|| left_assertion["subjectId"].as_str());
        let left_target = left["canonicalTargetId"]
            .as_str()
            .or_else(|| left_assertion["targetId"].as_str());
        for (right_index, right) in entries.iter().enumerate().skip(left_index + 1) {
            let right_assertion = &right["assertion"];
            let right_subject = right["canonicalSubjectId"]
                .as_str()
                .or_else(|| right_assertion["subjectId"].as_str());
            let right_target = right["canonicalTargetId"]
                .as_str()
                .or_else(|| right_assertion["targetId"].as_str());
            if right_assertion["kind"] != kind
                || right_subject != left_subject
                || right_assertion["predicate"] != left_assertion["predicate"]
                || (kind == "relation"
                    && (right_target != left_target
                        || right_assertion["relationType"] != left_assertion["relationType"]))
                || right_assertion["value"].as_bool() != Some(!left_value)
            {
                continue;
            }
            let left_time = left_assertion["validFrom"]
                .as_i64()
                .or_else(|| left_assertion["storyTime"].as_i64());
            let right_time = right_assertion["validFrom"]
                .as_i64()
                .or_else(|| right_assertion["storyTime"].as_i64());
            let overlaps = match (left_time, right_time) {
                (Some(left_start), Some(right_start)) if kind == "relation" => {
                    left_assertion["validUntilExclusive"]
                        .as_i64()
                        .is_none_or(|end| right_start < end)
                        && right_assertion["validUntilExclusive"]
                            .as_i64()
                            .is_none_or(|end| left_start < end)
                }
                (Some(left_at), Some(right_at)) => left_at == right_at,
                _ => {
                    uncertain_pairs += 1;
                    false
                }
            };
            if !overlaps {
                continue;
            }
            if findings.len() == MAX_FINDINGS {
                truncated = true;
                continue;
            }
            findings.push(json!({
                "type": "polarityConflict",
                "subjectId": left_subject,
                "predicate": left_assertion["predicate"],
                "targetId": if kind == "relation" { json!(left_target) } else { Value::Null },
                "leftClaimSubjectId": left_assertion["subjectId"],
                "rightClaimSubjectId": right_assertion["subjectId"],
                "leftEntryIndex": left_index, "rightEntryIndex": right_index,
                "leftSourceRevisionId": left["sourceRevisionId"],
                "rightSourceRevisionId": right["sourceRevisionId"],
                "leftEvidence": left_assertion["evidence"],
                "rightEvidence": right_assertion["evidence"],
                "confirmationState": conflict_authority(left, right)
            }));
        }
    }
    let (properties, property_truncated, property_uncertain) =
        crate::property_conflicts::detect(entries, MAX_FINDINGS - findings.len());
    findings.extend(properties);
    (
        findings,
        truncated || property_truncated,
        uncertain_pairs + property_uncertain,
    )
}

pub(crate) fn conflict_authority(left: &Value, right: &Value) -> &'static str {
    if left["confirmationState"] == "assumed" || right["confirmationState"] == "assumed" {
        "hypothesisConflict"
    } else if left["confirmationState"] == "confirmed" && right["confirmationState"] == "confirmed"
    {
        "confirmedConflict"
    } else {
        "candidateConflict"
    }
}

#[cfg(test)]
#[path = "knowledge_conflicts_tests.rs"]
mod tests;
