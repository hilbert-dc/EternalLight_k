use serde_json::Value;

/// Whether a source-bound epistemic claim applies at the requested story time.
/// An unknown start never grants knowledge, and an evidenced end is exclusive.
pub fn epistemic_claim_active_at(assertion: &Value, at: i64) -> bool {
    assertion["kind"] == "epistemic"
        && epistemic_interval_valid(assertion)
        && assertion["epistemicTime"]
            .as_i64()
            .is_some_and(|start| start <= at)
        && assertion["epistemicUntilExclusive"]
            .as_i64()
            .is_none_or(|end| at < end)
}

pub(crate) fn epistemic_interval_valid(assertion: &Value) -> bool {
    let end = assertion.get("epistemicUntilExclusive");
    let evidence = assertion.get("epistemicUntilEvidence");
    match (end, evidence) {
        (None, None) => true,
        (Some(end), Some(evidence)) => {
            assertion["kind"] == "epistemic"
                && assertion["epistemicTime"]
                    .as_i64()
                    .zip(end.as_i64())
                    .is_some_and(|(start, end)| start < end)
                && evidence.is_object()
        }
        (None, Some(_)) | (Some(_), None) => false,
    }
}
