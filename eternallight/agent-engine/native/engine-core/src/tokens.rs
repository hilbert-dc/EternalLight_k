use crate::Result;
use serde_json::Value;
use std::sync::OnceLock;
use tiktoken_rs::{CoreBPE, cl100k_base, o200k_base};

const MAX_ITEM_BYTES: usize = 256 * 1024;

pub fn count(text: &str) -> usize {
    static ENCODERS: OnceLock<[CoreBPE; 2]> = OnceLock::new();
    let encoders = ENCODERS.get_or_init(|| {
        [
            o200k_base().expect("embedded o200k encoding"),
            cl100k_base().expect("embedded cl100k encoding"),
        ]
    });
    let chars: Vec<char> = text.chars().collect();
    encoders
        .iter()
        .map(|encoder| {
            chars
                .chunks(256)
                .map(|chunk| {
                    encoder
                        .encode_ordinary(&chunk.iter().collect::<String>())
                        .len()
                })
                .sum()
        })
        .max()
        .unwrap_or(0)
}

pub fn bounded(value: &Value, label: &str) -> Result<()> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    if text.len() > MAX_ITEM_BYTES {
        return Err(format!("{label} exceeds the 256KiB item byte limit"));
    }
    if text.len() <= 10_000 || count(text) <= 10_000 {
        return Ok(());
    }
    Err(format!(
        "{label} exceeds the 10K-token conservative item limit"
    ))
}

pub fn bounded_output_item(item: &Value) -> Result<()> {
    let text = item.to_string();
    if let Err(error) = bounded(&Value::String(text.clone()), "provider output item") {
        let kind = match item["type"].as_str() {
            Some(
                kind @ ("message" | "reasoning" | "agent_message" | "function_call"
                | "custom_tool_call"),
            ) => kind,
            _ => "unknown",
        };
        let reasoning_bytes: Option<usize> = item["content"].as_array().map(|parts| {
            parts
                .iter()
                .filter(|part| part["type"] == "reasoning_text")
                .filter_map(|part| part["text"].as_str())
                .map(str::len)
                .sum()
        });
        let summary_bytes: Option<usize> = item["summary"].as_array().map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .map(str::len)
                .sum()
        });
        // A rejected multi-megabyte frame must not cause an unbounded diagnostic
        // tokenization pass. Null explicitly means the estimate was not measured.
        let conservative_tokens = (text.len() <= MAX_ITEM_BYTES).then(|| count(&text));
        let (limit_kind, limit) = if text.len() > MAX_ITEM_BYTES {
            ("utf8Bytes", MAX_ITEM_BYTES)
        } else {
            ("conservativeTokens", 10_000)
        };
        let diagnostic = serde_json::json!({
            "limitKind": limit_kind, "limit": limit,
            "type": kind, "utf8Bytes": text.len(), "conservativeTokens": conservative_tokens,
            "argsUtf8Bytes": item["arguments"].as_str().map(str::len),
            "reasoningTextUtf8Bytes": reasoning_bytes, "summaryTextUtf8Bytes": summary_bytes,
            "opaqueUtf8Bytes": item["encrypted_content"].as_str().map(str::len),
        });
        return Err(format!("{error}; outputItem={diagnostic}"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
