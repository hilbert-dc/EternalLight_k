use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::{RepositoryError, event_conditions::assess};

fn claims(time: Value, start: Value) -> Vec<Value> {
    let evidence = json!({"quote":"甲交信","start":0,"end":3});
    vec![
        json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":time,
            "value":{"participants":[{"role":"actor","entityId":"a"}],"preconditions":[{"sourceBatchId":null,"assertionIndex":1}],"effects":[],"untilExclusive":null},"evidence":evidence}),
        json!({"kind":"state","subjectId":"a","predicate":"held","value":true,
            "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":start,"evidence":evidence}),
    ]
}

#[test]
fn temporal_interpretation_preserves_boundaries_unknown_clocks_and_extreme_times() {
    for (event_time, start, end, timing) in [
        (json!(5), json!(1), Some(5), "applicableBeforeEvent"),
        (json!(5), json!(1), Some(4), "inactiveBeforeEvent"),
        (json!(5), json!(5), None, "startsAtEvent"),
        (json!(5), json!(6), None, "inactiveBeforeEvent"),
        (json!(5), Value::Null, None, "unknownTime"),
        (Value::Null, json!(1), None, "unknownTime"),
        (json!(i64::MIN), json!(i64::MIN), None, "startsAtEvent"),
        (
            json!(i64::MAX),
            json!(i64::MIN),
            Some(i64::MAX),
            "applicableBeforeEvent",
        ),
    ] {
        let mut source = claims(event_time, start);
        if let Some(end) = end {
            source[1]["validUntilExclusive"] = json!(end);
            source[1]["validUntilEvidence"] = source[1]["evidence"].clone();
        }
        let expected = json!({"basis":"sourceBoundTemporalReferences","preconditions":[{"referenceIndex":0,"timing":timing}],"effects":[],
            "semanticTruthInferred":false,"globalConditionsVerified":false,"stateChangesExecuted":false});
        assert_eq!(
            assess(&source[0], &source, &HashMap::new()).unwrap(),
            expected
        );
        let original = source.clone();
        source[0]["value"]["preconditions"] = json!([]);
        source[0]["value"]["effects"] = json!([{"sourceBatchId":null,"assertionIndex":1}]);
        let result = assess(&source[0], &source, &HashMap::new()).unwrap();
        let expected = match (
            source[0]["storyTime"].as_i64(),
            source[1]["validFrom"].as_i64(),
        ) {
            (Some(event), Some(start)) if start < event => "beforeEvent",
            (Some(event), Some(start)) if start == event => "atEvent",
            (Some(_), Some(_)) => "afterEvent",
            _ => "unknownTime",
        };
        assert_eq!(
            result["effects"],
            json!([{"referenceIndex":0,"timing":expected}])
        );
        assert_eq!(original[1], source[1]);
    }
}

#[test]
fn unsupported_references_remain_unexplained_and_corrupt_states_fail_closed() {
    let mut source = claims(json!(5), json!(1));
    source[1] = json!({"kind":"epistemic","subjectId":"p","epistemicOwnerId":"a","predicate":"knows","epistemicTime":1,"evidence":source[1]["evidence"]});
    assert_eq!(
        assess(&source[0], &source, &HashMap::new()).unwrap()["preconditions"],
        json!([{"referenceIndex":0,"timing":"unsupportedConditionKind"}])
    );
    source = claims(json!(5), json!(1));
    source[1]["value"] = json!("not a boolean");
    assert_eq!(
        assess(&source[0], &source, &HashMap::new()),
        Err(RepositoryError::CorruptState)
    );
    source[0]["value"]["preconditions"] = json!([]);
    let empty = assess(&source[0], &source, &HashMap::new()).unwrap();
    assert_eq!(
        (empty["preconditions"].clone(), empty["effects"].clone()),
        (json!([]), json!([]))
    );
}

#[test]
fn maximum_reference_report_is_bounded_without_duplicating_source_claims() {
    let mut source = claims(json!(5), json!(1));
    source.resize(33, source[1].clone());
    source[0]["value"]["preconditions"] = json!(
        (1..=16)
            .map(|index| json!({"sourceBatchId":null,"assertionIndex":index}))
            .collect::<Vec<_>>()
    );
    source[0]["value"]["effects"] = json!(
        (17..=32)
            .map(|index| json!({"sourceBatchId":null,"assertionIndex":index}))
            .collect::<Vec<_>>()
    );
    let original = source.clone();
    let result = assess(&source[0], &source, &HashMap::new()).unwrap();
    assert_eq!(source, original);
    assert_eq!(
        (
            result["preconditions"].as_array().unwrap().len(),
            result["effects"].as_array().unwrap().len()
        ),
        (16, 16)
    );
    assert!(result.to_string().len() <= 3_000);
    assert!(!result.to_string().contains("甲交信"));
}

#[test]
fn maximum_external_references_keep_reports_small_and_claims_independent() {
    let mut source = claims(json!(5), json!(1));
    source.resize(17, source[1].clone());
    let mut external = HashMap::new();
    let references = (0..16).map(|index| {
        let reference = json!({"sourceBatchId":format!("{index:03}-{}","x".repeat(156)),"assertionIndex":0});
        external.insert(serde_json::from_value(reference.clone()).unwrap(),source[1].clone());
        reference
    }).collect::<Vec<_>>();
    source[0]["value"]["preconditions"] = json!(references);
    source[0]["value"]["effects"] = json!(
        (1..=16)
            .map(|index| json!({"sourceBatchId":null,"assertionIndex":index}))
            .collect::<Vec<_>>()
    );
    let original = source.clone();
    let result = assess(&source[0], &source, &external).unwrap();
    assert_eq!(source, original);
    assert!(source[0].to_string().len() <= 8_000);
    assert!(result.to_string().len() <= 3_000);
    assert!(!result.to_string().contains("sourceBatchId"));
    assert_eq!(result["preconditions"].as_array().unwrap().len(), 16);
    external.clear();
    assert_eq!(
        assess(&source[0], &source, &external),
        Err(RepositoryError::CorruptState)
    );
}
