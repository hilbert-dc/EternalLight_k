use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Runtime's candidate identity after MCP payload validation/normalization.
/// This is distinct from the root MCP call hash and constructs no draft body.
pub(crate) fn draft_request_digest(
    payload: &Value,
    expected_revision: i64,
    expected_candidate_revision_id: Option<&str>,
) -> String {
    let identity = json!({
        "kind": "draft",
        "payload": payload,
        "expectedRevision": expected_revision,
        "expectedCandidateRevisionId": expected_candidate_revision_id,
    });
    format!("{:x}", Sha256::digest(identity.to_string().as_bytes()))
}
