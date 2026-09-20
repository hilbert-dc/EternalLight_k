use crate::Result;
use serde_json::Value;
use std::sync::OnceLock;
use tiktoken_rs::{CoreBPE, cl100k_base, o200k_base};

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
    if text.len() <= 10_000 || (text.len() <= 40_000 && count(text) <= 10_000) {
        return Ok(());
    }
    Err(format!(
        "{label} exceeds the 10K-token conservative item limit"
    ))
}
