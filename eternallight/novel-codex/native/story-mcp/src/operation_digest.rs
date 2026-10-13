#[path = "../../../shared/novel-operation-digest.rs"]
mod implementation;

pub(super) use implementation::{host_json, operation_digest};

#[cfg(test)]
#[path = "operation_digest_tests.rs"]
mod tests;

// This envelope is created at the model-tool boundary, before transport-only
// defaults. Never trust an envelope supplied by the caller: bind_model replaces it.
pub(super) fn bind_model(name: &str, args: &serde_json::Value, meta: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut bound = meta.clone();
    let fields = bound.as_object_mut().ok_or("execution metadata must be an object")?;
    fields.insert("novelModelRequest".into(), serde_json::json!({
        "tool":name,"callId":meta["callId"],"digest":operation_digest(name,args)?
    }));
    Ok(bound)
}
pub(super) fn transport_hash(name: &str, meta: &serde_json::Value, fallback: &str) -> String {
    let request = &meta["novelModelRequest"];
    if request["tool"] == name && request["callId"] == meta["callId"] {
        if let Some(hash) = request["digest"].as_str() { return hash.to_owned(); }
    }
    fallback.to_owned()
}
