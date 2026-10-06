use super::failure::Outcome;
use crate::{HttpEvent, SseDecoder, canonical};
use serde_json::{Value, json};

// Called only at local codec validation sites, never on provider HTTP/error prose.
pub(super) fn is_invalid_arguments(message: &str) -> bool {
    crate::tool_validation::diagnostic_message(message).is_some()
        || matches!(
            message,
            "invalid_tool_arguments"
                | "tool arguments are not valid JSON"
                | "tool arguments must decode to an object"
        )
}

pub(super) fn reported_usage(usage: &Value) -> Option<Value> {
    let count = |value: &Value| {
        value.as_f64().filter(|value| {
            value.is_finite()
                && *value >= 0.0
                && *value <= 9_007_199_254_740_991.0
                && value.fract() == 0.0
        })
    };
    count(&usage["total_tokens"])?;
    for field in ["input_tokens", "output_tokens"] {
        if !usage[field].is_null() {
            count(&usage[field])?;
        }
    }
    Some(usage.clone())
}

pub(super) fn chat_usage(usage: &Value) -> Option<Value> {
    // Use the same normalization as successful Chat decoding, without interpreting tools.
    reported_usage(
        &canonical::envelope(&json!({"id":"usage-only","items":[],"usage":{
        "inputTokens":usage["prompt_tokens"],"outputTokens":usage["completion_tokens"],
        "totalTokens":usage["total_tokens"],"inputTokenDetails":usage["prompt_tokens_details"],
        "outputTokenDetails":usage["completion_tokens_details"]}}))["usage"],
    )
}

pub(super) fn payload_usage(protocol: &str, payload: &Value) -> Option<Value> {
    match protocol {
        "responses" => reported_usage(&payload["usage"]),
        "chat-completions" => chat_usage(&payload["usage"]),
        _ => None,
    }
}

pub(super) fn terminal_usage(frame: &Value) -> Option<Option<Value>> {
    let data = &frame["data"];
    if !matches!(
        data["type"].as_str(),
        Some("response.completed" | "response.failed" | "response.incomplete")
    ) {
        return None;
    }
    Some(reported_usage(&data["response"]["usage"]))
}

// No decoding, display or execution after rejection. Read only the same bounded transport.
// A transport/timeout/cancellation failure keeps the original validation failure and unknown usage.
pub(super) fn drain_usage(
    mut frames: Vec<Value>,
    sse: &mut SseDecoder,
    mut next: impl FnMut() -> Outcome<HttpEvent>,
    mut remaining_bytes: usize,
) -> Option<Value> {
    loop {
        for frame in frames {
            if let Some(usage) = terminal_usage(&frame) {
                return usage;
            }
        }
        match next().ok()? {
            HttpEvent::Chunk(chunk) => {
                remaining_bytes = remaining_bytes.checked_sub(chunk.len())?;
                frames = sse.push(&chunk).ok()?;
            }
            HttpEvent::End => return sse.finish().ok()?.iter().find_map(terminal_usage).flatten(),
            HttpEvent::Error(_) | HttpEvent::Headers { .. } => return None,
        }
    }
}
