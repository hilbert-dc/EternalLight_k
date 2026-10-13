//! Admission diagnostics contain counts only, never model input or credentials.
use serde::{Deserialize, Serialize};

const PREFIX: &str = "encoded request exceeds the route input budget; contextCapacity=";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Capacity {
    boundary: Boundary,
    estimated_input_tokens: u64,
    input_limit_tokens: u64,
    excess_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Boundary {
    Connection,
    Route,
}

pub(crate) fn check(estimated: u64, limit: u64, boundary: Boundary) -> crate::Result<()> {
    if estimated <= limit {
        return Ok(());
    }
    let report = Capacity {
        boundary,
        estimated_input_tokens: estimated,
        input_limit_tokens: limit,
        excess_tokens: estimated - limit,
    };
    Err(format!(
        "{PREFIX}{}",
        serde_json::to_string(&report).unwrap()
    ))
}

pub(crate) fn diagnostic(message: &str) -> Option<serde_json::Value> {
    let report: Capacity = serde_json::from_str(message.strip_prefix(PREFIX)?).ok()?;
    if report
        .estimated_input_tokens
        .checked_sub(report.input_limit_tokens)?
        != report.excess_tokens
        || report.excess_tokens == 0
    {
        return None;
    }
    serde_json::to_value(report).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_boundary_and_count_only_diagnostics() {
        assert!(check(10, 10, Boundary::Route).is_ok());
        assert!(check(9, 10, Boundary::Connection).is_ok());
        let error = check(11, 10, Boundary::Route).unwrap_err();
        let report = diagnostic(&error).unwrap();
        assert_eq!(
            report,
            serde_json::json!({"boundary":"route","estimatedInputTokens":11,"inputLimitTokens":10,"excessTokens":1})
        );
        for invalid in [
            error.replace("\"excessTokens\":1", "\"excessTokens\":0"),
            error.replace("\"route\"", "\"private text\""),
            error.replace('}', ",\"input\":\"private text\"}"),
            "encoded request exceeds the route input budget".into(),
        ] {
            assert!(diagnostic(&invalid).is_none());
        }
    }
}
