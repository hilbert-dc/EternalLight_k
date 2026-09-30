use pretty_assertions::assert_eq;
use serde_json::json;

use super::fit_optional_context;

#[test]
fn empty_projection_compaction_retains_active_scope_policy_and_authoritative_inputs() {
    let projection = json!({"projectId":"project-p","snapshotId":null,"identityRevisionId":null,
        "worldTime":null,"epistemicTime":12,"ownerId":"linxi","perspective":"character","readingChapter":2,"visibleThroughChapter":2,
        "selectors":{"entityId":null,"predicate":null},"propertyRulesRevisionId":null,"propertyRulesPolicy":"withheldForLimitedPerspective",
        "entries":[],"findings":[],"coverage":{"complete":false,"candidateEntries":0,"confirmedEntries":0,"unappliedPropertyRules":0}});
    let mut pack = json!({"target":{"projectId":"project-p"},"snapshotId":"frozen-source","creativeCapabilityBinding":{"frozen":true},
        "operationContext":{"workingKnowledge":projection},"relevantText":[],
        "coverage":{"complete":false,"missingScope":["workingKnowledge"],"truncationReasons":["contextKnowledgeBudget"],
            "scannedScope":{"draftsIncluded":0,"modelingSourcesIncluded":0}}});
    let before = pack.clone();
    let limit = pack.to_string().len() - 1;
    fit_optional_context(&mut pack, limit);
    let mut expected = projection;
    for field in [
        "projectId",
        "snapshotId",
        "identityRevisionId",
        "worldTime",
        "selectors",
    ] {
        expected.as_object_mut().unwrap().remove(field);
    }
    expected["metadataTruncated"] = json!(true);
    assert_eq!(pack["operationContext"]["workingKnowledge"], expected);
    assert_eq!(
        (
            &pack["target"],
            &pack["snapshotId"],
            &pack["creativeCapabilityBinding"]
        ),
        (
            &before["target"],
            &before["snapshotId"],
            &before["creativeCapabilityBinding"]
        )
    );
    assert!(pack.to_string().len() <= limit);
    assert_eq!(
        pack["coverage"]["truncationReasons"],
        json!(["contextKnowledgeBudget", "contextKnowledgeMetadataBudget"])
    );
}

#[test]
fn optional_checks_do_not_create_missing_or_null_context_fields() {
    for context in [
        json!({}),
        json!({"entityCatalog":null,"workingKnowledge":null,"modeling":null}),
    ] {
        let mut pack = json!({"target":{"operation":{"chapterNumber":1}},"operationContext":context,
            "relevantText":[{"content":{"chapterNumber":1,"body":"必须保留的正文".repeat(/*n*/100)}}],
            "coverage":{"complete":true,"missingScope":[],"truncationReasons":[]}});
        let before = pack.clone();
        fit_optional_context(&mut pack, /*byte_limit*/ 500);
        assert_eq!(pack["operationContext"], before["operationContext"]);
        assert_eq!(pack["relevantText"], before["relevantText"]);
        assert!(pack.get("candidateCommitmentProposals").is_none());
        assert!(pack.get("relevantCommitments").is_none());
        assert!(pack.to_string().len() > 500);
    }
}

#[test]
fn insufficient_budget_does_not_erase_bound_commitment_criteria_or_references() {
    let bound = json!({"scope":"frozenPlanReferences","entries":[{
        "reference":{"type":"commitment","commitmentId":"lamp","revisionId":"original"},
        "referenceState":"current","fulfillmentCriteria":"解释白塔的秘密".repeat(/*n*/300)
    }]});
    let mut pack = json!({
        "target":{"operation":{"chapterNumber":2}},
        "operationContext":{"boundCommitmentInputs":bound},
        "relevantText":[{"sourceRevisionId":"optional","content":{"chapterNumber":1,"body":"optional"}}],
        "coverage":{"complete":true,"missingScope":[],"truncationReasons":[]}
    });
    fit_optional_context(&mut pack, /*byte_limit*/ 500);
    assert_eq!(pack["operationContext"]["boundCommitmentInputs"], bound);
    assert_eq!(pack["relevantText"], json!([]));
    assert!(pack.to_string().len() > 500);
    assert_eq!(
        pack["coverage"],
        json!({"complete":false,"missingScope":["chapterText"],"truncationReasons":["contextTextBudget"],
        "scannedScope":{"draftsIncluded":0,"modelingSourcesIncluded":0}})
    );
}
