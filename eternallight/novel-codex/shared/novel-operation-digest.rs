use serde_json::Value;
use sha2::{Digest, Sha256};

// The Host receives JSON through JavaScript. Preserve IEEE-754 numbers while
// removing the integral-float representation that serde_json alone retains.
// Larger integers cannot survive that boundary exactly; reject instead of
// certifying two different identities for the same call.
pub(crate) fn host_json(value: &Value) -> Result<Value, String> {
    match value {
        Value::Number(number) => {
            let number = number.as_f64().ok_or("operation number is not finite")?;
            if number.fract() == 0.0 {
                if number.abs() > 9_007_199_254_740_991.0 {
                    return Err("operation integer exceeds the exact JSON interoperability range; use a string".into());
                }
                Ok(Value::from(number as i64))
            } else {
                Ok(value.clone())
            }
        }
        Value::Array(items) => items
            .iter()
            .map(host_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(items) => items
            .iter()
            .map(|(key, value)| Ok((key.clone(), host_json(value)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(Value::Object),
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(value.clone()),
    }
}

pub(crate) fn operation_digest(tool: &str, arguments: &Value) -> Result<String, String> {
    let mut arguments = host_json(arguments)?;
    // Runtime transport carries these fields; model arguments do not. The ledger
    // independently binds project/task/thread, never obtains identity from this hash.
    if let Some(object) = arguments.as_object_mut() {
        for field in ["projectId", "taskId", "threadId"] {
            object.remove(field);
        }
    }
    let canonical = serde_json::json!({"tool":tool,"arguments":arguments});
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical.to_string().as_bytes())
    ))
}
