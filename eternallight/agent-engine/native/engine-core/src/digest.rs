use crate::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Canonical JSON for engine identities. Values cross a JSON-only
/// boundary: absent properties are absent, array holes are null, and non-JSON
/// objects are represented by their JSON form. No JavaScript-only sentinel is
/// part of the identity format. serde_json objects use deterministic key order.
pub(crate) fn stable(value: &Value) -> String {
    value.to_string()
}

pub(crate) fn digest(value: &Value) -> String {
    let mut hash = Sha256::new();
    hash.update(stable(value));
    format!("{:x}", hash.finalize())
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "identity.stableJson" => Ok(stable(input).into()),
        "identity.digest" => Ok(digest(input).into()),
        _ => Err("unknown identity operation".into()),
    }
}
