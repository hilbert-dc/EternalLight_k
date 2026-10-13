use super::assess;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn temporal_effects_and_condition_conflicts_never_execute_or_cascade_confirmation() {
    let event = json!({"value":{"effects":(0..5).map(|index| json!({"sourceBatchId":null,"assertionIndex":index+1})).collect::<Vec<_>>()}});
    let temporal = json!({"effects":[{"referenceIndex":0,"timing":"beforeEvent"},{"referenceIndex":1,"timing":"atEvent"},{"referenceIndex":2,"timing":"afterEvent"},{"referenceIndex":3,"timing":"unknownTime"},{"referenceIndex":4,"timing":"unsupportedEffectKind"}]});
    let confirmed = HashMap::from([
        (("batch".into(), 0), "confirmed-event".into()),
        (("batch".into(), 2), "independently-confirmed-effect".into()),
    ]);
    for (result, compatibility) in [
        ("supported", "supported"),
        ("refuted", "refuted"),
        ("both", "conflicted"),
        ("unknown", "unknown"),
    ] {
        let report = assess(
            &event,
            "batch",
            &temporal,
            &json!({"status":"interpreted","result":result}),
            &confirmed,
        )
        .unwrap();
        assert_eq!(
            report,
            json!({"basis":"declaredSourceEffectLinks","effects":[
            {"referenceIndex":0,"interpretation":"preexistingClaim","conditionCompatibility":compatibility,"confirmationState":"candidate","confirmedFactId":null},
            {"referenceIndex":1,"interpretation":"coincidentClaim","conditionCompatibility":compatibility,"confirmationState":"confirmed","confirmedFactId":"independently-confirmed-effect"},
            {"referenceIndex":2,"interpretation":"laterClaim","conditionCompatibility":compatibility,"confirmationState":"candidate","confirmedFactId":null},
            {"referenceIndex":3,"interpretation":"unknownTime","conditionCompatibility":compatibility,"confirmationState":"candidate","confirmedFactId":null},
            {"referenceIndex":4,"interpretation":"unsupportedKind","conditionCompatibility":compatibility,"confirmationState":"candidate","confirmedFactId":null}],
            "stateChangesExecuted":false,"causalityCertified":false,"worldTruthCertified":false})
        );
    }
    let report = assess(
        &event,
        "batch",
        &temporal,
        &json!({"status":"noApplicableRule","result":"unknown"}),
        &confirmed,
    )
    .unwrap();
    assert!(
        report["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|effect| effect["conditionCompatibility"] == "unruled")
    );
}

#[test]
fn effect_reports_keep_long_confirmation_ids_bounded_and_reject_misaligned_references() {
    let event = json!({"value":{"effects":(0..16).map(|index| json!({"sourceBatchId":null,"assertionIndex":index})).collect::<Vec<_>>()}});
    let mut temporal = json!({"effects":(0..16).map(|index| json!({"referenceIndex":index,"timing":"atEvent"})).collect::<Vec<_>>()});
    let confirmed = (0..16)
        .map(|index| {
            (
                ("batch".into(), index),
                format!("{index:03}{}", "f".repeat(157)),
            )
        })
        .collect();
    let conditions = json!({"status":"interpreted","result":"supported"});
    let report = assess(&event, "batch", &temporal, &conditions, &confirmed).unwrap();
    assert!(report.to_string().len() <= 6144);
    temporal["effects"][15]["referenceIndex"] = json!(0);
    assert!(assess(&event, "batch", &temporal, &conditions, &confirmed).is_err());
}
