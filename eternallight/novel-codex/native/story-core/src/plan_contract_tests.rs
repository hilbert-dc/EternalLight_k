use serde_json::json;

use super::validate_generated_plan;

#[test]
fn generated_plan_requires_distinct_testable_effects_for_every_scene() {
    let valid = json!({"payload":{"chapters":[{"chapterPlanId":"chapter-plan-1","chapterNumber":1,"scenes":[
        {"id":"scene-1","goal":"查找线索","expectedEffects":[
            {"id":"effect-1","dimension":"knowledge","description":"林夕发现白塔灯时记录","commitmentRefs":[]}
        ]}
    ]}]}});
    assert!(validate_generated_plan(&valid.to_string()).is_ok());
    let mut missing_refs = valid.clone();
    missing_refs["payload"]["chapters"][0]["scenes"][0]["expectedEffects"][0]
        .as_object_mut()
        .unwrap()
        .remove("commitmentRefs");
    assert!(validate_generated_plan(&missing_refs.to_string()).is_err());
    let mut duplicate_refs = valid.clone();
    let reference = json!({"type":"proposal","reportRevisionId":"report","proposalId":"lamp","sourceRevisionId":"draft"});
    duplicate_refs["payload"]["chapters"][0]["scenes"][0]["expectedEffects"][0]["commitmentRefs"] =
        json!([reference, reference]);
    assert!(validate_generated_plan(&duplicate_refs.to_string()).is_err());
    let mut too_many_refs = valid.clone();
    too_many_refs["payload"]["chapters"][0]["scenes"][0]["expectedEffects"][0]["commitmentRefs"] = json!((0..5).map(|index|
        json!({"type":"commitment","commitmentId":format!("promise-{index}"),"revisionId":"revision"})).collect::<Vec<_>>());
    assert!(validate_generated_plan(&too_many_refs.to_string()).is_err());
    let mut forged_refs = valid.clone();
    forged_refs["payload"]["chapters"][0]["scenes"][0]["expectedEffects"][0]["commitmentRefs"] = json!([
        {"type":"commitment","commitmentId":"promise","revisionId":"revision","confirmed":true}
    ]);
    assert!(validate_generated_plan(&forged_refs.to_string()).is_err());
    let mut missing = valid.clone();
    missing["payload"]["chapters"][0]["scenes"][0]["expectedEffects"] = json!([]);
    assert!(validate_generated_plan(&missing.to_string()).is_err());
    let mut duplicate = valid;
    duplicate["payload"]["chapters"][0]["scenes"][0]["expectedEffects"] = json!([
        {"id":"effect-1","dimension":"knowledge","description":"林夕发现白塔灯时记录","commitmentRefs":[]} ,
        {"id":"effect-1","dimension":"world","description":"钟声改变港城的夜晚","commitmentRefs":[]}
    ]);
    assert!(validate_generated_plan(&duplicate.to_string()).is_err());
}
