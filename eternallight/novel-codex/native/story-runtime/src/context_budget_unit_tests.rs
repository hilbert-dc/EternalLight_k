use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::fit_optional_context;

fn current_context_fixture(mut pack: Value, byte_limit: usize) -> Value {
    pack["id"] = json!(format!("context-{}", "0".repeat(64)));
    pack["contentHash"] = json!("0".repeat(64));
    pack["purpose"] = json!("writer");
    pack["taskId"] = Value::Null;
    pack["budgetsUsed"] = json!({"byteLimit":byte_limit,"byteSize":0,"entryLimit":24});
    pack
}

#[test]
fn empty_projection_compaction_retains_active_scope_policy_and_authoritative_inputs() {
    let projection = json!({"projectId":"project-p","snapshotId":null,"identityRevisionId":null,
        "worldTime":null,"epistemicTime":12,"ownerId":"linxi","perspective":"character","readingChapter":2,"visibleThroughChapter":2,
        "selectors":{"entityId":null,"predicate":null},"propertyRulesRevisionId":null,"propertyRulesPolicy":"withheldForLimitedPerspective",
        "entries":[],"findings":[],"coverage":{"complete":false,"candidateEntries":0,"confirmedEntries":0,"unappliedPropertyRules":0}});
    let mut pack = current_context_fixture(
        json!({"target":{"projectId":"project-p"},"snapshotId":"frozen-source","creativeCapabilityBinding":{"frozen":true},
        "operationContext":{"workingKnowledge":projection},"relevantText":[],
        "coverage":{"complete":false,"missingScope":["workingKnowledge"],"truncationReasons":["contextKnowledgeBudget"],
            "scannedScope":{"draftsIncluded":0,"modelingSourcesIncluded":0}}}),
        60000,
    );
    let limit = independently_finalized(pack.clone()).to_string().len() - 1;
    pack["budgetsUsed"]["byteLimit"] = json!(limit);
    let before = pack.clone();
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
        let mut pack = current_context_fixture(
            json!({"target":{"operation":{"chapterNumber":1}},"operationContext":context,
            "relevantText":[{"content":{"chapterNumber":1,"body":"必须保留的正文".repeat(/*n*/100)}}],
            "coverage":{"complete":true,"missingScope":[],"truncationReasons":[]}}),
            500,
        );
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
    let mut pack = current_context_fixture(
        json!({
            "target":{"operation":{"chapterNumber":2}},
            "operationContext":{"boundCommitmentInputs":bound},
            "relevantText":[{"sourceRevisionId":"optional","content":{"chapterNumber":1,"body":"optional"}}],
            "coverage":{"complete":true,"missingScope":[],"truncationReasons":[]}
        }),
        500,
    );
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

fn finalized_budget_fixture(optional_repeats: usize) -> Value {
    json!({
        "id":format!("context-{}", "0".repeat(64)),"contentHash":"0".repeat(64),
        "purpose":"writer","taskId":null,"target":{"position":{"readingChapter":3}},
        "relevantText":[
            {"sourceRevisionId":"current-target","content":{"chapterNumber":3,"body":"必须保留的目标正文。"}},
            {"sourceRevisionId":"prior-optional","content":{"chapterNumber":2,"body":"钟表修理师藏好修理记录。".repeat(optional_repeats)}}
        ],
        "operationContext":{},
        "coverage":{"complete":true,"missingScope":[],"truncationReasons":[],
            "scannedScope":{"draftsIncluded":2,"modelingSourcesIncluded":0}},
        "budgetsUsed":{"byteLimit":60000,"byteSize":0,"entryLimit":24}
    })
}

// Independently reconstruct the current response identity; this is an assertion
// oracle, not the production finalizer under test.
fn independently_finalized(mut pack: Value) -> Value {
    loop {
        let actual_size = pack.to_string().len();
        if pack["budgetsUsed"]["byteSize"] == json!(actual_size) {
            break;
        }
        pack["budgetsUsed"]["byteSize"] = json!(actual_size);
    }
    let mut content = pack.clone();
    let object = content.as_object_mut().unwrap();
    object.remove("id");
    object.remove("contentHash");
    let hash = format!("{:x}", Sha256::digest(content.to_string().as_bytes()));
    pack["id"] = json!(format!("context-{hash}"));
    pack["contentHash"] = json!(hash);
    pack
}

#[test]
fn finalized_pack_budget_trims_optional_instead_of_false_required_rejection() {
    let mut pack = finalized_budget_fixture(719);
    let required = pack["relevantText"][0].clone();
    let placeholder_tokens = crate::context_model_item_budget::model_item_tokens(&pack);
    let actual_before = independently_finalized(pack.clone());
    let finalized_tokens = crate::context_model_item_budget::model_item_tokens(&actual_before);
    assert_eq!(placeholder_tokens, 8976);
    assert_eq!(finalized_tokens, 9008);
    assert!(pack.to_string().len() + 200 < 60000);
    assert!(actual_before.to_string().len() + 200 < 60000);
    assert!(crate::context_model_item_budget::fits(&pack));
    assert!(!crate::context_model_item_budget::fits(&actual_before));
    let mut required_only = pack.clone();
    required_only["relevantText"] = json!([required.clone()]);
    required_only["coverage"]["scannedScope"]["draftsIncluded"] = json!(1);
    assert!(crate::context_model_item_budget::fits(
        &independently_finalized(required_only)
    ));

    fit_optional_context(&mut pack, 60000);
    let actual_after = independently_finalized(pack.clone());
    assert!(
        crate::context_model_item_budget::fits(&actual_after),
        "optional fitting left a removable prior draft in the finalized over-budget response: {} tokens",
        crate::context_model_item_budget::model_item_tokens(&actual_after)
    );
    assert_eq!(pack, actual_after);
    assert_eq!(pack["relevantText"], json!([required]));
    assert_eq!(pack["coverage"]["scannedScope"]["draftsIncluded"], 1);
    assert_eq!(
        pack["coverage"]["scannedScope"]["modelingSourcesIncluded"],
        0
    );
    assert_eq!(pack["coverage"]["complete"], false);
    assert_eq!(pack["coverage"]["missingScope"], json!(["chapterText"]));
    assert_eq!(
        pack["coverage"]["truncationReasons"],
        json!(["contextTextBudget"])
    );
}

#[test]
fn finalized_pack_budget_records_real_counts_size_hash_and_is_idempotent() {
    let mut pack = finalized_budget_fixture(3);
    pack["coverage"]["scannedScope"]["draftsIncluded"] = json!(999);
    pack["coverage"]["scannedScope"]["modelingSourcesIncluded"] = json!(999);
    pack["operationContext"]["modeling"] = json!([{"sourceRevisionId":"visible-modeling-source"}]);
    let required_and_optional = pack["relevantText"].clone();
    let target = pack["target"].clone();
    fit_optional_context(&mut pack, 60000);
    assert_eq!(pack["relevantText"], required_and_optional);
    assert_eq!(pack["target"], target);
    assert_eq!(pack["coverage"]["scannedScope"]["draftsIncluded"], 2);
    assert_eq!(
        pack["coverage"]["scannedScope"]["modelingSourcesIncluded"],
        1
    );
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
    assert_eq!(pack, independently_finalized(pack.clone()));
    let first = pack.clone();
    fit_optional_context(&mut pack, 60000);
    assert_eq!(pack, first);
}

#[test]
fn finalized_pack_budget_preserves_required_prose_when_tokens_really_exceed_limit() {
    let mut pack = finalized_budget_fixture(0);
    pack["relevantText"] = json!([pack["relevantText"][0].clone()]);
    pack["relevantText"][0]["content"]["body"] = json!("霾".repeat(16000));
    let required = pack["relevantText"].clone();
    let target = pack["target"].clone();
    fit_optional_context(&mut pack, 60000);
    assert_eq!(pack["relevantText"], required);
    assert_eq!(pack["target"], target);
    assert!(pack.to_string().len() + 200 < 60000);
    assert!(!crate::context_model_item_budget::fits(&pack));
    assert_eq!(pack, independently_finalized(pack.clone()));
}
