use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn dispatch(method: &str, input: Value) -> Result<Value> {
    let mut entries = input["entries"].as_array().cloned().unwrap_or_default();
    let namespace = input["namespace"].clone();
    let name = input["name"].as_str().unwrap_or("");
    let key = serde_json::to_string(&json!([namespace, name])).map_err(|e| e.to_string())?;
    match method {
        "tools.register" => {
            if !namespace.is_null() && namespace.as_str().is_none_or(str::is_empty) { return Err("tool namespace is invalid".into()); }
            if name.is_empty() { return Err("tool name is required".into()); }
            let executable = input["executable"].as_bool().unwrap_or(true);
            let schema = input["schema"].clone();
            let entry = if let Some(entry) = entries.iter_mut().find(|e| e["key"] == key) {
                if executable { entry["executable"] = true.into(); }
                if !schema.is_null() { entry["schema"] = schema; }
                entry.clone()
            } else {
                let max = input["maxTools"].as_u64().filter(|n| *n > 0).ok_or("tool registry limit must be positive")?;
                if entries.len() as u64 >= max { return Err(format!("tool catalog exceeds {max} entries")); }
                let direct = namespace.as_str().map_or_else(|| format!("f__{name}"), |ns| format!("n{}__{ns}__{name}", ns.encode_utf16().count()));
                let wire_name = if direct.len() <= 64 && direct.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') { direct } else { format!("t__{:.40}", format!("{:x}", Sha256::digest(format!("{key}:0")))) };
                if entries.iter().any(|e| e["wireName"] == wire_name) { return Err("tool alias hash collision".into()); }
                let entry = json!({"key":key,"namespace":namespace,"name":name,"wireName":wire_name,"schema":schema,"executable":executable});
                entries.push(entry.clone()); entry
            };
            Ok(json!({"entries":entries,"entry":entry}))
        }
        "tools.require" => {
            let wire = input["wireName"].as_str().filter(|s| !s.is_empty()).ok_or("tool call name is required")?;
            entries.iter().find(|e| e["wireName"] == wire && (!input["executable"].as_bool().unwrap_or(true) || e["executable"] == true)).cloned().ok_or_else(|| format!("tool is not authorized for this request: {wire}"))
        }
        "tools.name" => entries.iter().find(|e| e["key"] == key).map(|e| e["wireName"].clone()).ok_or_else(|| format!("tool is not declared for this request: {name}")),
        "tools.snapshot" => Ok(entries.iter().map(|e| json!({"namespace":e["namespace"],"name":e["name"],"wireName":e["wireName"],"executable":e["executable"]})).collect()),
        _ => Err("unknown tool registry operation".into()),
    }
}
