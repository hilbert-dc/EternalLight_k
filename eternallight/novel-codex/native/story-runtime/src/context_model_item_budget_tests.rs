use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{MODEL_ITEM_TOKEN_LIMIT, fits, model_item_tokens, project_text};

#[test]
fn writer_context_preserves_scenes_and_source_while_removing_exact_duplicate_text() {
    let groups = json!([
        {"id":"locked-scene","body":"  沈遥收起账簿。  ","locked":true,"sourcePlanRefs":[{"sceneId":"planned-scene"}]},
        {"id":"next-scene","body":"她关掉手电。","locked":false}
    ]);
    let mut pack = json!({"purpose":"writer","relevantText":[{"sourceRevisionId":"frozen-source","authority":"working",
        "content":{"title":"站台余响","body":"沈遥收起账簿。\n\n---\n\n她关掉手电。","sceneGroups":groups}}]});
    let mut expected = pack.clone();
    expected["relevantText"][0]["content"]
        .as_object_mut()
        .unwrap()
        .remove("body");
    expected["relevantText"][0]["textRepresentation"] = json!("sceneGroups");
    project_text(&mut pack["relevantText"][0]);
    assert_eq!(pack, expected);
}

#[test]
fn distinct_or_body_only_sources_remain_complete() {
    let drafts = json!([
        {"content":{"body":"不同的正文","sceneGroups":[{"id":"scene","body":"场景正文","locked":true}]}},
        {"content":{"body":"完整的单表示正文"}}
    ]);
    let mut writer = json!({"purpose":"writer","relevantText":drafts});
    let mut expected = writer.clone();
    for draft in expected["relevantText"].as_array_mut().unwrap() {
        draft["textRepresentation"] = json!("body");
    }
    for draft in writer["relevantText"].as_array_mut().unwrap() {
        project_text(draft);
    }
    assert_eq!(writer, expected);
}

#[test]
fn writer_budget_includes_escaped_tool_envelope_and_preserves_required_text_on_failure() {
    let mut pack = json!({"purpose":"writer","target":{"operation":{"chapterNumber":1}},
        "relevantText":[{"content":{"chapterNumber":1,"body":"钟表修理师沈遥藏好修理记录。\"\n".repeat(/*n*/900)}}],
        "operationContext":{},"coverage":{"complete":true,"missingScope":[],"truncationReasons":[],"scannedScope":{}}});
    let before = pack.clone();
    let tokens = model_item_tokens(&pack);
    assert!(tokens > MODEL_ITEM_TOKEN_LIMIT);
    crate::context_budget::fit_optional_context(&mut pack, /*byte_limit*/ 100_000);
    assert!(!fits(&pack));
    assert_eq!(pack["relevantText"], before["relevantText"]);
    let actual_codex_item = json!({"type":"function_call_output","call_id":"call-1","output":[
        {"type":"input_text","text":"Tool result: novel_workspace.read_context"},
        {"type":"input_text","text":pack.to_string()}
    ]});
    let actual_tokens = eternallight_engine_core::dispatch(
        "tokens.count",
        Value::String(actual_codex_item.to_string()),
    )
    .unwrap()
    .as_u64()
    .unwrap();
    assert!(actual_tokens < tokens + 1_000);
    let short = json!({"purpose":"writer","relevantText":[{"content":{"body":"沈遥关掉手电。"}}]});
    assert!(fits(&short));
}

#[test]
fn model_budget_applies_to_bound_analysis_and_writer_previews_with_author_planner_independent() {
    let mut pack = json!({"purpose":"planner","taskId":null,"relevantText":[{"content":{"body":"沈遥藏起父亲的修理记录。".repeat(/*n*/1100)}}]});
    assert!(model_item_tokens(&pack) > MODEL_ITEM_TOKEN_LIMIT);
    assert!(fits(&pack));
    pack["taskId"] = json!("frozen-analysis-task");
    assert!(!fits(&pack));
    pack["purpose"] = json!("critic");
    assert!(!fits(&pack));
    pack["taskId"] = Value::Null;
    pack["purpose"] = json!("writer");
    assert!(!fits(&pack));
}
