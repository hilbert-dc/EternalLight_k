use serde_json::{Value, json};

use crate::state_properties::{Cardinality, parse_schema, schema};

pub(crate) fn detect(entries: &[Value], budget: usize) -> (Vec<Value>, bool, usize) {
    let mut findings = Vec::new();
    let mut truncated = false;
    let mut uncertain = 0;
    for (left_index, left) in entries.iter().enumerate() {
        let a = &left["assertion"];
        if a["kind"] != "state" {
            continue;
        }
        if schema(a).is_err() {
            continue;
        }
        let Ok(left_schema) = parse_schema(&left["effectiveStateSchema"]) else {
            continue;
        };
        for (right_index, right) in entries.iter().enumerate().skip(left_index + 1) {
            let b = &right["assertion"];
            if b["kind"] != "state"
                || left["canonicalSubjectId"] != right["canonicalSubjectId"]
                || a["predicate"] != b["predicate"]
            {
                continue;
            }
            if schema(b).is_err() {
                continue;
            }
            let Ok(right_schema) = parse_schema(&right["effectiveStateSchema"]) else {
                continue;
            };
            let rule_changed = left_schema != right_schema;
            if !rule_changed
                && (left_schema.cardinality == Cardinality::Multiple
                    || left["canonicalStateValue"] == right["canonicalStateValue"])
            {
                continue;
            }
            let (Some(left_start), Some(right_start)) =
                (a["validFrom"].as_i64(), b["validFrom"].as_i64())
            else {
                uncertain += 1;
                continue;
            };
            if a["validUntilExclusive"]
                .as_i64()
                .is_some_and(|end| right_start >= end)
                || b["validUntilExclusive"]
                    .as_i64()
                    .is_some_and(|end| left_start >= end)
            {
                continue;
            }
            if findings.len() == budget {
                truncated = true;
                continue;
            }
            findings.push(json!({"type":if rule_changed {"stateSchemaDisagreement"} else {"stateValueConflict"},
                "subjectId":left["canonicalSubjectId"],"predicate":a["predicate"],"schemaAuthority":left["stateSchemaAuthority"],
                "propertyRulesRevisionId":left["propertyRuleAssessment"]["revisionId"],
                "leftEntryIndex":left_index,"rightEntryIndex":right_index,
                "leftSchema":left["effectiveStateSchema"],"rightSchema":right["effectiveStateSchema"],
                "leftValue":left["canonicalStateValue"],"rightValue":right["canonicalStateValue"],
                "leftSourceRevisionId":left["sourceRevisionId"],"rightSourceRevisionId":right["sourceRevisionId"],
                "leftEvidence":a["evidence"],"rightEvidence":b["evidence"],
                "confirmationState":crate::knowledge_conflicts::conflict_authority(left,right)}));
        }
    }
    (findings, truncated, uncertain)
}

#[cfg(test)]
#[path = "property_conflicts_tests.rs"]
mod tests;
