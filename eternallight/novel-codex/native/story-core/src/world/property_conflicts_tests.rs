use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::detect;

fn entry(index: usize, start: Value) -> Value {
    json!({"canonicalSubjectId":"linxi","canonicalStateValue":index,"sourceRevisionId":format!("revision-{index}"),
        "confirmationState":"candidate","stateSchemaAuthority":"modelDeclared",
        "effectiveStateSchema":{"cardinality":"single","valueType":{"type":"integer"}},"assertion":{"kind":"state","subjectId":"linxi","predicate":"age",
            "value":index,"stateSchema":{"cardinality":"single","valueType":{"type":"integer"}},
            "validFrom":start,"evidence":{"quote":"年","start":0,"end":1}}})
}

#[test]
fn property_findings_share_a_hard_budget_and_unknown_overlap_is_not_a_conflict() {
    let entries = (0..16)
        .map(|index| entry(index, json!(1)))
        .collect::<Vec<_>>();
    let (findings, truncated, uncertain) = detect(&entries, /*budget*/ 100);
    assert_eq!((findings.len(), truncated, uncertain), (100, true, 0));
    let (findings, truncated, uncertain) = detect(&entries, /*budget*/ 0);
    assert_eq!((findings.len(), truncated, uncertain), (0, true, 0));
    assert_eq!(
        detect(
            &[entry(0, json!(1)), entry(1, Value::Null)],
            /*budget*/ 100
        ),
        (Vec::new(), false, 1)
    );
}

#[test]
fn canonical_identity_equivalence_does_not_conflict_or_overwrite_independent_claims() {
    let mut first = entry(0, json!(1));
    let mut second = entry(1, json!(1));
    for (row, raw) in [(&mut first, "raw-a"), (&mut second, "raw-b")] {
        row["assertion"]["stateSchema"]["valueType"] = json!({"type":"entity","kind":"location"});
        row["assertion"]["value"] = json!({"entityId":raw});
        row["effectiveStateSchema"] = row["assertion"]["stateSchema"].clone();
        row["canonicalStateValue"] = json!({"entityId":"canonical-port"});
    }
    let source_entries = vec![first, second];
    let before = source_entries.clone();
    assert_eq!(
        detect(&source_entries, /*budget*/ 100),
        (Vec::new(), false, 0)
    );
    assert_eq!(source_entries, before);
}
