use serde_json::{Value, json};

pub(crate) const MODEL_ITEM_TOKEN_LIMIT: u64 = 9_000;

/// Context carries each scene's text once, with its authoritative identity and locks.
pub(crate) fn project_text(draft: &mut Value) {
    let Some(content) = draft.get_mut("content").and_then(Value::as_object_mut) else {
        return;
    };
    let reconstructed = content
        .get("sceneGroups")
        .and_then(Value::as_array)
        .filter(|groups| !groups.is_empty())
        .and_then(|groups| {
            groups
                .iter()
                .map(|group| group["body"].as_str().map(str::trim))
                .collect::<Option<Vec<_>>>()
        })
        .map(|bodies| bodies.join("\n\n---\n\n"));
    if reconstructed
        .as_deref()
        .is_some_and(|text| content.get("body").and_then(Value::as_str) == Some(text))
    {
        content.remove("body");
        draft["textRepresentation"] = json!("sceneGroups");
    } else {
        draft["textRepresentation"] = json!("body");
    }
}

/// Budget the escaped MCP result inside a model item, including a maximal call identity.
/// The remaining 1K covers Codex's tool prefix and transport-specific result formatting.
pub(crate) fn model_item_tokens(pack: &Value) -> u64 {
    let result = json!({"content":[{"type":"text","text":pack.to_string()}]});
    let item = json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":result.to_string()});
    eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
        .expect("embedded model tokenizer")
        .as_u64()
        .expect("token count")
}

pub(crate) fn fits(pack: &Value) -> bool {
    (pack["purpose"] != "writer" && pack["taskId"].is_null())
        || model_item_tokens(pack) <= MODEL_ITEM_TOKEN_LIMIT
}

#[cfg(test)]
#[path = "context_model_item_budget_tests.rs"]
mod tests;
