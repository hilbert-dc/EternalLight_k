use serde_json::{Value, json};

pub(crate) const MODEL_ITEM_BYTE_LIMIT: u64 = 256 * 1024 - 1024;

pub(crate) use novel_codex_story_core::project_draft_source_text as project_text;

/// Bound the fully escaped tool envelope; reserve 1 KiB for Core's tool prefix.
/// Model token capacity is checked by Engine on the complete request.
pub(crate) fn model_item_bytes(pack: &Value) -> u64 {
    let result = json!({"content":[{"type":"text","text":pack.to_string()}]});
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":result.to_string()});
    item.to_string().len() as u64
}

pub(crate) fn fits(pack: &Value) -> bool {
    (pack["purpose"] != "writer" && pack["taskId"].is_null())
        || model_item_bytes(pack) <= MODEL_ITEM_BYTE_LIMIT
}

#[cfg(test)]
#[path = "context_model_item_budget_tests.rs"]
mod tests;
