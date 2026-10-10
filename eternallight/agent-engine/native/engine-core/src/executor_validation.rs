//! Only the embedding host may delegate schema checks to a trusted executor.
//! Tool identity, JSON shape, size, choice and stream admission stay in the bridge.
use crate::Result;
use serde_json::Value;

pub(crate) fn check(policy: &Value) -> Result<()> {
    if policy.is_null() {
        return Ok(());
    }
    let entries = policy
        .as_array()
        .filter(|v| v.len() <= 128)
        .ok_or("executorValidatedTools must be a bounded list")?;
    for entry in entries {
        let object = entry
            .as_object()
            .ok_or("invalid executor validation binding")?;
        if object.len() != 2 || !object.contains_key("namespace") || !object.contains_key("name") {
            return Err("executor validation requires exact namespace and name".into());
        }
        for field in ["namespace", "name"] {
            if field == "namespace" && entry[field].is_null() {
                continue;
            }
            if entry[field].as_str().is_none_or(|v| {
                v.is_empty()
                    || v.len() > 128
                    || !v
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            }) {
                return Err("invalid executor validation identity".into());
            }
        }
    }
    Ok(())
}

pub(crate) fn delegated(context: &Value, namespace: &Value, name: &Value) -> bool {
    context["executorValidatedTools"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|entry| entry["namespace"] == *namespace && entry["name"] == *name)
}
