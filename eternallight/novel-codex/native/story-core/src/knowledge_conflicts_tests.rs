use serde_json::json;

use super::detect_conflicts;

#[test]
fn relation_conflict_requires_overlapping_world_intervals() {
    let first = json!({
        "sourceRevisionId": "revision-a", "confirmationState": "confirmed",
        "assertion": {"kind": "relation", "subjectId": "a", "predicate": "owns",
            "relationType": "owns", "targetId": "item", "propositionId": "a-owns-item", "value": true,
            "validFrom": 1, "validUntilExclusive": 10,
            "evidence": {"quote": "owned"}}
    });
    let second = json!({
        "sourceRevisionId": "revision-b", "confirmationState": "confirmed",
        "assertion": {"kind": "relation", "subjectId": "a", "predicate": "owns",
            "relationType": "owns", "targetId": "item", "propositionId": "a-owns-item", "value": false,
            "validFrom": 9, "validUntilExclusive": 20,
            "evidence": {"quote": "not owned"}}
    });
    let (findings, truncated, uncertain) = detect_conflicts(&[first.clone(), second.clone()]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["confirmationState"], "confirmedConflict");
    assert!(!truncated);
    assert_eq!(uncertain, 0);

    let mut later = second;
    later["assertion"]["validFrom"] = json!(10);
    let (findings, truncated, uncertain) = detect_conflicts(&[first.clone(), later]);
    assert!(findings.is_empty());
    assert!(!truncated);
    assert_eq!(uncertain, 0);

    let mut unknown = first;
    unknown["assertion"]
        .as_object_mut()
        .unwrap()
        .remove("validFrom");
    let (findings, truncated, uncertain) = detect_conflicts(&[
        unknown,
        json!({
            "sourceRevisionId": "revision-c", "confirmationState": "candidate",
            "assertion": {"kind": "relation", "subjectId": "a", "predicate": "owns",
                "relationType": "owns", "targetId": "item", "propositionId": "a-owns-item", "value": false,
                "validFrom": 5, "evidence": {"quote": "not owned"}}
        }),
    ]);
    assert!(findings.is_empty());
    assert!(!truncated);
    assert_eq!(uncertain, 1);
}

#[test]
fn epistemic_disagreement_keeps_owner_proposition_scope_and_evidence() {
    let first = json!({"sourceRevisionId":"first", "confirmationState":"confirmed",
        "canonicalEpistemicOwnerId":"linxi", "propositionRef":{"batchId":"origin","propositionId":"door"},
        "assertion":{"kind":"epistemic","epistemicOwnerId":"lin-alias","epistemicState":"known",
            "epistemicTime":1,"epistemicUntilExclusive":5,"value":true,"evidence":{"quote":"确信"}}});
    let second = json!({"sourceRevisionId":"second", "confirmationState":"candidate",
        "canonicalEpistemicOwnerId":"linxi", "propositionRef":{"batchId":"origin","propositionId":"door"},
        "assertion":{"kind":"epistemic","epistemicOwnerId":"linxi","epistemicState":"suspected",
            "epistemicTime":4,"value":true,"evidence":{"quote":"怀疑"}}});
    assert_eq!(
        detect_conflicts(&[first.clone(), second.clone()]),
        (
            vec![json!({
                "type":"epistemicDisagreement","subjectId":"linxi","predicate":"epistemicState",
                "ownerId":"linxi","propositionRef":{"batchId":"origin","propositionId":"door"},
                "leftState":"known","rightState":"suspected","leftEntryIndex":0,"rightEntryIndex":1,
                "leftSourceRevisionId":"first","rightSourceRevisionId":"second",
                "leftEvidence":{"quote":"确信"},"rightEvidence":{"quote":"怀疑"},
                "confirmationState":"candidateConflict"
            })],
            false,
            0
        )
    );
    for changed in [
        json!({"canonicalEpistemicOwnerId":"other"}),
        json!({"propositionRef":{"batchId":"unrelated","propositionId":"door"}}),
        json!({"assertion":{"kind":"epistemic","epistemicOwnerId":"linxi","epistemicState":"suspected",
            "epistemicTime":5,"value":true}}),
        json!({"assertion":{"kind":"epistemic","epistemicOwnerId":"linxi","epistemicState":"known",
            "epistemicTime":4,"value":true}}),
    ] {
        let mut different = second.clone();
        for (key, value) in changed.as_object().unwrap() {
            different[key] = value.clone();
        }
        assert_eq!(
            detect_conflicts(&[first.clone(), different]),
            (Vec::new(), false, 0)
        );
    }
    let mut undated = second;
    undated["assertion"]
        .as_object_mut()
        .unwrap()
        .remove("epistemicTime");
    assert_eq!(detect_conflicts(&[first, undated]), (Vec::new(), false, 1));
}
