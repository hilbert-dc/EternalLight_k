use pretty_assertions::assert_eq;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::{ContextPackInput, build_context_pack};

fn input(task_id: Option<&str>, frozen_plan: bool) -> ContextPackInput {
    ContextPackInput {
        query_report: None,
        query_scope_policy: None,
        project_id: "context-project".into(),
        project_revision: 7,
        snapshot_id: "immutable-snapshot".into(),
        task_id: task_id.map(str::to_owned),
        purpose: "writer".into(),
        actor: "candidate".into(),
        position: json!({"readingChapter":3,"sceneOrder":1,"povCharacterId":"linxi"}),
        task_target: json!({"chapterId":"chapter-three","chapterNumber":3,"sourceRevisionId":"current-target"}),
        delegation_targets: None,
        input_manifest: None,
        title: "港城".into(),
        logline: "".into(),
        artifacts: Map::new(),
        drafts: vec![
            json!({"sourceRevisionId":"current-target","content":{"chapterNumber":3,"body":"必须保留的目标正文。"}}),
            json!({"sourceRevisionId":"visible-prior","content":{"chapterNumber":2,"body":"此前留下的可见原文。"}}),
        ],
        facts: vec![],
        modeling: vec![],
        commitments: json!({
            "entries":[],"coverage":{"complete":true,"truncated":false},
            "proposalContext":{"entries":[],"coverage":{"requested":false,"complete":true}},
            "assessmentContext":{"scope":if frozen_plan {"frozenPlanReferences"} else {"visibleContext"},
                "entries":[{"reference":{"commitmentId":"protected-plan-reference","revisionId":"immutable-plan"},"fulfillmentCriteria":"不得改变原范围或省略必需目标正文"}]}
        }),
        planning_review: None,
        modeling_batch: None,
        entity_catalog: None,
        working_knowledge: None,
        scene_plan_draft_links: (0..120).map(|index|json!({
            "chapterNumber":3,"sceneId":format!("scene-{index}"),"assessmentStatus":"assessed",
            "sceneOutcomeAssessment":{"revisionId":format!("assessment-{index}"),"verdict":"partial",
                "recommendation":"下一步核对原文与场景兑现证据。"}
        })).collect(),
        scene_links_truncated: false,
        scene_event_refs_truncated: false,
        source_watermarks: json!({"digest":"immutable-source-watermark","objectCount":3}),
        chapters_in_scope: 3,
        modeling_sources_in_scope: 0,
        drafts_truncated: false,
        facts_truncated: false,
        typed_state_facts_withheld: 0,
        typed_event_facts_withheld: 0,
        modeling_complete: true,
        working_knowledge_complete: true,
        pov_evidence_withheld: false,
        pov_prior_text_withheld: false,
        byte_limit: 60000,
        entry_limit: 24,
    }
}

fn assert_real_identity(pack: &Value) {
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
    let mut content = pack.clone();
    content.as_object_mut().unwrap().remove("id");
    content.as_object_mut().unwrap().remove("contentHash");
    let actual_hash = format!("{:x}", Sha256::digest(content.to_string().as_bytes()));
    assert_eq!(pack["contentHash"], actual_hash);
    assert_eq!(pack["id"], format!("context-{actual_hash}"));
}

#[test]
fn optional_scene_links_fit_real_finalized_model_item_with_current_coverage() {
    let original = input(None, false);
    let required = original.drafts[0].clone();
    let links = original.scene_plan_draft_links.clone();
    let pack = build_context_pack(original);
    assert!(crate::context_model_item_budget::fits(&pack));
    assert!(pack.to_string().len() + 200 <= 60000);
    assert_real_identity(&pack);
    let retained = pack["operationContext"]["scenePlanDraftLinks"]
        .as_array()
        .unwrap();
    assert!(!retained.is_empty());
    assert!(retained.len() < links.len());
    assert_eq!(retained.as_slice(), &links[..retained.len()]);
    assert_eq!(
        pack["sceneOutcomeRefs"].as_array().unwrap().len(),
        retained.len()
    );
    assert!(pack["relevantText"].as_array().unwrap().contains(&required));
    assert_eq!(pack["target"]["position"]["readingChapter"], 3);
    assert_eq!(pack["coverage"]["complete"], false);
    assert_eq!(
        pack["coverage"]["missingScope"],
        json!(["scenePlanDraftLinks"])
    );
    assert_eq!(
        pack["coverage"]["truncationReasons"],
        json!(["sceneLinkBudget"])
    );
    assert_eq!(
        pack["coverage"]["scannedScope"]["draftsIncluded"],
        pack["relevantText"].as_array().unwrap().len()
    );
    assert_eq!(
        pack["sourceWatermarks"],
        json!({"digest":"immutable-source-watermark","objectCount":3})
    );
}

#[test]
fn bound_cache_inputs_and_frozen_plan_links_are_not_trimmed_before_source_projection() {
    for (task_id, frozen_plan) in [(Some("bound-writer"), false), (None, true)] {
        let original = input(task_id, frozen_plan);
        let drafts = original.drafts.clone();
        let links = original.scene_plan_draft_links.clone();
        let criteria = original.commitments["assessmentContext"].clone();
        let pack = build_context_pack(original);
        assert_real_identity(&pack);
        assert_eq!(
            pack["operationContext"]["scenePlanDraftLinks"],
            json!(links)
        );
        assert_eq!(pack["sceneOutcomeRefs"].as_array().unwrap().len(), 120);
        assert_eq!(pack["operationContext"]["boundCommitmentInputs"], criteria);
        if task_id.is_some() {
            assert_eq!(pack["relevantText"], json!(drafts));
            assert_eq!(pack["coverage"]["scannedScope"]["draftsIncluded"], 2);
            assert_eq!(pack["taskId"], "bound-writer");
        } else {
            assert!(
                pack["relevantText"]
                    .as_array()
                    .unwrap()
                    .contains(&drafts[0])
            );
        }
        assert!(!crate::context_model_item_budget::fits(&pack));
        assert!(
            !pack["coverage"]["truncationReasons"]
                .as_array()
                .unwrap()
                .contains(&json!("sceneLinkBudget"))
        );
        assert_eq!(pack["snapshotId"], "immutable-snapshot");
        assert_eq!(pack["target"]["projectRevision"], 7);
        assert_eq!(
            pack["sourceWatermarks"],
            json!({"digest":"immutable-source-watermark","objectCount":3})
        );
    }
}
