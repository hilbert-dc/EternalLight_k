use crate::{
    Result,
    canonical::{array, string},
    evidence, storage,
};
use serde_json::{Value, json};
use std::{fs::File, io::Read, path::Path};

fn bounded(value: &Value, maximum: usize, redactions: &[Value]) -> String {
    let mut result = match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        value => value.to_string(),
    };
    for secret in redactions
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        result = result.replace(secret, "[REDACTED]");
    }
    let mut length = 0;
    result
        .chars()
        .take_while(|c| {
            length += c.len_utf16();
            length <= maximum
        })
        .collect()
}

fn diagnostic(value: &Value, redactions: &[Value], depth: usize) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(_) => bounded(value, 500, redactions).into(),
        Value::Array(_) | Value::Object(_) if depth >= 5 => "[DEPTH_LIMIT]".into(),
        Value::Array(values) => values
            .iter()
            .take(320)
            .map(|v| diagnostic(v, redactions, depth + 1))
            .collect(),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .take(64)
                .map(|(k, v)| {
                    (
                        bounded(&json!(k), 120, redactions),
                        diagnostic(v, redactions, depth + 1),
                    )
                })
                .collect(),
        ),
    }
}

fn timestamp(value: &Value, field: &str) -> Result<String> {
    let date = chrono::DateTime::parse_from_rfc3339(string(value))
        .map_err(|_| format!("capability report {field} is invalid"))?;
    Ok(date
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

pub(crate) fn define(input: &Value) -> Result<Value> {
    let connection = &input["connection"];
    if ["id", "provider", "model"]
        .iter()
        .any(|field| string(&connection[field]).is_empty())
    {
        return Err("capability report connection identity is required".into());
    }
    let result = &input["result"];
    let error = &input["error"];
    let redactions = array(&input["redactions"]);
    let attempts = if result["attempts"].is_null() {
        &error["attempts"]
    } else {
        &result["attempts"]
    };
    let attempts: Vec<_> = array(attempts).iter().map(|attempt| {
        let mut value = json!({"protocol":bounded(&attempt["protocol"],80,redactions),"status":bounded(&attempt["status"],40,redactions)});
        for (field,limit) in [("suite",120),("category",80),("message",500)] {
            if !attempt[field].is_null() && attempt[field] != "" { value[field] = bounded(&attempt[field],limit,redactions).into(); }
        }
        for field in ["budget","diagnostics"] { if !attempt[field].is_null() { value[field] = diagnostic(&attempt[field],redactions,0); } }
        value
    }).collect();
    let raw = if result["records"].is_null() {
        &error["records"]
    } else {
        &result["records"]
    };
    let records = array(raw)
        .iter()
        .map(|r| evidence::normalize(r, evidence::RecordMode::Draft))
        .collect::<Result<Vec<_>>>()?;
    let selected: Vec<_> = records
        .iter()
        .filter(|r| {
            result["selectedProtocol"].is_null()
                || r["scope"]["protocol"] == result["selectedProtocol"]
        })
        .collect();
    let status = if !error.is_null() || selected.iter().any(|r| r["status"] == "failed") {
        "failed"
    } else if selected.is_empty()
        || selected
            .iter()
            .any(|r| matches!(r["status"].as_str(), Some("unknown" | "declared")))
    {
        "partial"
    } else {
        "verified"
    };
    Ok(
        json!({"version":1,"status":status,"connection":{"id":connection["id"],"provider":connection["provider"],"model":connection["model"]},"selectedProtocol":result["selectedProtocol"],"engineContractHash":input["contractDigest"],"startedAt":timestamp(&input["startedAt"],"startedAt")?,"completedAt":timestamp(&input["completedAt"],"completedAt")?,"attempts":attempts,"records":records,"error":if error.is_null() { Value::Null } else { json!({"message":bounded(&error["message"],500,redactions)}) }}),
    )
}

/// Performs bounded report IO; callers verify the canonical content digest before adoption.
pub fn execute(directory: &Path, method: &str, input: &Value) -> Result<Value> {
    let maximum = input["maxReportBytes"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= storage::MAX_DOCUMENT_BYTES)
        .ok_or("capability report size limit is invalid")?;
    let id = if method == "write" {
        &input["report"]["reportId"]
    } else {
        &input["reportId"]
    };
    let id = id
        .as_str()
        .filter(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .ok_or("capability report id is invalid")?;
    match method {
        "write" => {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(directory).map_err(|e| e.to_string())?;
            let report = &input["report"];
            let completed = timestamp(&report["completedAt"], "completedAt")?.replace(':', "-");
            let path = directory.join(format!("{completed}-{id}.json"));
            let bytes =
                serde_json::to_vec_pretty(report).map_err(|_| "invalid capability report")?;
            if bytes.len() as u64 + 1 > maximum {
                return Err("capability report exceeds size limit".into());
            }
            storage::durable_replace_json(&path, report)?;
            Ok(path.to_str().ok_or("report path is not UTF-8")?.into())
        }
        "read" => {
            let entries = std::fs::read_dir(directory).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    format!("capability report is missing: {id}")
                } else {
                    e.to_string()
                }
            })?;
            let suffix = format!("-{id}.json");
            let mut selected = None;
            for (count, entry) in entries.enumerate() {
                if count >= 100_000 {
                    return Err("capability report directory exceeds entry limit".into());
                }
                let entry = entry.map_err(|e| e.to_string())?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(&suffix))
                {
                    if selected.is_some() {
                        return Err(format!("ambiguous capability report: {id}"));
                    }
                    selected = Some(entry.path());
                }
            }
            let path = selected.ok_or_else(|| format!("capability report is missing: {id}"))?;
            let mut bytes = Vec::new();
            File::open(path)
                .map_err(|e| e.to_string())?
                .take(maximum + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > maximum {
                return Err(format!("capability report exceeds size limit: {id}"));
            }
            let report: Value =
                serde_json::from_slice(&bytes).map_err(|_| "invalid capability report JSON")?;
            if report["reportId"] != id {
                return Err(format!("capability report digest mismatch: {id}"));
            }
            Ok(report)
        }
        _ => Err("unknown report store operation".into()),
    }
}
