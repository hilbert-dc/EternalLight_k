use serde_json::{Value, json};

use crate::RepositoryError;

/// Resolve a literal quote in an already authorized source. Numeric anchors are
/// checked, never corrected; absent anchors require a unique exact occurrence.
pub(crate) fn resolve(evidence: &Value, body: &str) -> Result<Value, RepositoryError> {
    let invalid = |code: &str| RepositoryError::InvalidContent(code.into());
    let fields = evidence
        .as_object()
        .ok_or_else(|| invalid("evidence_shape_invalid"))?;
    if fields
        .keys()
        .any(|key| !matches!(key.as_str(), "quote" | "start" | "end"))
    {
        return Err(invalid("evidence_shape_invalid"));
    }
    let quote = evidence["quote"]
        .as_str()
        .filter(|quote| !quote.is_empty() && quote.chars().count() <= 2000)
        .ok_or_else(|| invalid("evidence_quote_invalid"))?;
    let length = quote.chars().count();
    let (start, end) = match (fields.get("start"), fields.get("end")) {
        (None, None) => {
            let byte = body
                .find(quote)
                .ok_or_else(|| invalid("evidence_quote_not_found"))?;
            let next = byte
                + body[byte..]
                    .chars()
                    .next()
                    .ok_or_else(|| invalid("evidence_quote_not_found"))?
                    .len_utf8();
            if body[next..].contains(quote) {
                return Err(invalid("evidence_quote_ambiguous"));
            }
            let start = body[..byte].chars().count();
            (start, start + length)
        }
        (Some(start), Some(end)) => {
            let start = start
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid("evidence_anchor_invalid"))?;
            let end = end
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid("evidence_anchor_invalid"))?;
            if end.checked_sub(start) != Some(length)
                || end > body.chars().count()
                || body.chars().skip(start).take(length).collect::<String>() != quote
            {
                return Err(invalid("evidence_anchor_mismatch"));
            }
            (start, end)
        }
        (Some(_), None) | (None, Some(_)) => return Err(invalid("evidence_anchor_incomplete")),
    };
    Ok(json!({"quote":quote,"start":start,"end":end}))
}

/// Preserve actionable evidence errors across both direct and independent checks.
pub(crate) fn located_error(error: crate::RepositoryError, path: String, group: &str, revision: &Value) -> crate::RepositoryError {
    let crate::RepositoryError::InvalidContent(reason) = &error else { return error; };
    let code = match reason.as_str() {
        "evidence_quote_not_found" => "evidence_quote_not_found",
        "evidence_quote_ambiguous" => "evidence_quote_ambiguous",
        "evidence_anchor_mismatch" => "evidence_anchor_mismatch",
        "evidence_anchor_invalid" => "evidence_anchor_invalid",
        "evidence_anchor_incomplete" => "evidence_anchor_incomplete",
        "evidence_quote_invalid" => "evidence_quote_invalid",
        "evidence_shape_invalid" => "evidence_shape_invalid",
        _ => return error,
    };
    crate::RepositoryError::PayloadValidation { code, instance_path:path,
        details: json!({"sourceRef":{"groupId":group,"draftRevisionId":revision}}) }
}
