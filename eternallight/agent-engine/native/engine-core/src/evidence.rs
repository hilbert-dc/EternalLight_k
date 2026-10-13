use crate::{
    Result,
    canonical::{array, string},
    contracts,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn timestamp(value: &Value) -> Result<i64> {
    chrono::DateTime::parse_from_rfc3339(string(value))
        .map(|time| time.timestamp_millis())
        .map_err(|_| "evidence timestamps are invalid".into())
}

fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|v| {
        v.len() == 64
            && v.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|v| !v.is_empty())
}

pub(crate) fn assertion(input: &Value) -> Result<Value> {
    if !input.is_object() {
        return Err("evidence assertion must be an object".into());
    }
    let id = string(&input["id"]);
    if id.is_empty()
        || id.len() > 160
        || !id.as_bytes()[0].is_ascii_alphanumeric()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
    {
        return Err("evidence assertion id is invalid".into());
    }
    if !matches!(
        input["result"].as_str(),
        Some("passed" | "failed" | "unknown" | "not-run")
    ) {
        return Err("evidence assertion result is invalid".into());
    }
    if !input["traceRef"].is_null()
        && (!nonempty(&input["traceRef"])
            || string(&input["traceRef"]).encode_utf16().count() > 500)
    {
        return Err("evidence assertion traceRef is invalid".into());
    }
    let mut result = json!({"id":id,"result":input["result"],"traceRef":input["traceRef"]});
    for field in ["expected", "observed"] {
        if let Some(value) = input.get(field) {
            if value.to_string().encode_utf16().count() > 8192 {
                return Err(format!(
                    "evidence assertion {field} exceeds the evidence size limit"
                ));
            }
            result[field] = value.clone();
        }
    }
    Ok(result)
}

pub(crate) fn dependencies(input: &Value) -> Result<Value> {
    let entries = input
        .as_object()
        .ok_or("evidence dependencies must be an object")?;
    if entries.is_empty() {
        return Err("evidence dependencies must not be empty".into());
    }
    for (kind, dependency) in entries {
        if !["modelProtocol", "engineBuild", "route", "access", "report"].contains(&kind.as_str()) {
            return Err(format!("unknown evidence dependency kind: {kind}"));
        }
        let object = dependency
            .as_object()
            .ok_or_else(|| format!("evidence dependency {kind} must be an object"))?;
        if !hash(&dependency["digest"]) {
            return Err(format!("evidence dependency {kind} digest is invalid"));
        }
        if dependency.get("ref").is_some()
            && (!nonempty(&dependency["ref"])
                || string(&dependency["ref"]).encode_utf16().count() > 500)
        {
            return Err(format!("evidence dependency {kind} ref is invalid"));
        }
        if object
            .keys()
            .any(|key| !["digest", "ref"].contains(&key.as_str()))
        {
            return Err(format!("evidence dependency {kind} has unknown fields"));
        }
    }
    Ok(input.clone())
}

fn scope(input: &Value) -> Result<Value> {
    let object = input
        .as_object()
        .ok_or("evidence scope must be an object")?;
    for field in ["protocol", "suite", "mode"] {
        if !nonempty(&input[field]) {
            return Err(format!("evidence scope {field} is required"));
        }
    }
    if object.keys().any(|key| {
        ![
            "protocol",
            "suite",
            "mode",
            "toolTypes",
            "parameters",
            "routes",
            "stateCapabilities",
            "extensions",
            "behaviors",
        ]
        .contains(&key.as_str())
    }) {
        return Err("evidence scope has unknown fields".into());
    }
    let mut result =
        json!({"protocol":input["protocol"],"suite":input["suite"],"mode":input["mode"]});
    for field in [
        "toolTypes",
        "routes",
        "stateCapabilities",
        "extensions",
        "behaviors",
    ] {
        if let Some(values) = input[field].as_array() {
            let sorted: BTreeSet<_> = values.iter().map(Value::to_string).collect();
            result[field] = sorted
                .into_iter()
                .map(|text| serde_json::from_str::<Value>(&text).unwrap())
                .collect();
        }
    }
    if input["parameters"].is_object() {
        result["parameters"] = input["parameters"].clone();
    }
    Ok(result)
}

pub(crate) fn covers(actual: &Value, required: &Value) -> bool {
    if !actual.is_object() || !required.is_object() || actual["protocol"] != required["protocol"] {
        return false;
    }
    if nonempty(&required["mode"]) && actual["mode"] != required["mode"] && actual["mode"] != "all"
    {
        return false;
    }
    for field in [
        "toolTypes",
        "routes",
        "stateCapabilities",
        "extensions",
        "behaviors",
    ] {
        if !array(&required[field])
            .iter()
            .all(|value| array(&actual[field]).contains(value))
        {
            return false;
        }
    }
    required["parameters"]
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::iter)
        .all(|(key, value)| actual["parameters"].get(key) == Some(value))
}

pub(crate) enum RecordMode {
    Draft,
    Bound,
}

pub(crate) fn normalize(input: &Value, mode: RecordMode) -> Result<Value> {
    let capability = string(&input["capability"]);
    let domain = contracts::domain(capability)?;
    if input.get("domain").is_some_and(|value| value != domain) {
        return Err(format!(
            "evidence domain does not match capability: {capability}"
        ));
    }
    let measurement = match string(&input["status"]) {
        "declared" => "not-run",
        "verified" => "passed",
        "failed" => "failed",
        "unknown" => "unknown",
        _ => return Err("invalid evidence status".into()),
    };
    if !matches!(
        input["source"].as_str(),
        Some("metadata" | "probe" | "conformance-suite")
    ) {
        return Err("invalid evidence source".into());
    }
    for field in ["environmentDigest", "evidenceRef", "checkedAt", "expiresAt"] {
        if !nonempty(&input[field]) {
            return Err(format!("evidence {field} is required"));
        }
    }
    timestamp(&input["checkedAt"])?;
    timestamp(&input["expiresAt"])?;
    let supersedes = if input["supersedes"].is_null() {
        &[][..]
    } else {
        input["supersedes"]
            .as_array()
            .ok_or("evidence supersedes must contain evidence references")?
    };
    if supersedes.iter().any(|reference| !nonempty(reference)) {
        return Err("evidence supersedes must contain evidence references".into());
    }
    let assertions = if input["assertions"].is_null() {
        &[][..]
    } else {
        input["assertions"]
            .as_array()
            .ok_or("evidence assertions must be an array")?
    };
    let mut assertions = assertions
        .iter()
        .map(assertion)
        .collect::<Result<Vec<_>>>()?;
    let ids: BTreeSet<_> = assertions.iter().map(|a| string(&a["id"])).collect();
    if ids.len() != assertions.len() {
        return Err("evidence assertion ids must be unique".into());
    }
    if measurement == "passed"
        && (assertions.is_empty() || assertions.iter().any(|a| a["result"] != "passed"))
    {
        return Err("verified evidence requires every assertion to pass".into());
    }
    assertions.sort_by(|a, b| string(&a["id"]).cmp(string(&b["id"])));
    if matches!(mode, RecordMode::Bound)
        && input["source"] == "conformance-suite"
        && (!hash(&input["reportRef"]) || !hash(&input["reportDigest"]))
    {
        return Err("conformance evidence requires a bound report reference and digest".into());
    }
    Ok(
        json!({"capability":capability,"domain":domain,"status":input["status"],"measurementStatus":measurement,"scope":scope(&input["scope"])? ,"source":input["source"],"credentialRealm":input["credentialRealm"],"environmentDigest":input["environmentDigest"],"evidenceRef":input["evidenceRef"],"checkedAt":input["checkedAt"],"expiresAt":input["expiresAt"],"sample":input["sample"],"assertions":assertions,"dependencies":dependencies(&input["dependencies"])? ,"reportRef":input["reportRef"],"reportDigest":input["reportDigest"],"supersedes":supersedes.iter().map(string).collect::<BTreeSet<_>>()}),
    )
}

fn source_priority(record: &Value) -> u8 {
    match string(&record["source"]) {
        "conformance-suite" => 2,
        "probe" => 1,
        _ => 0,
    }
}
fn status_priority(record: &Value) -> u8 {
    match string(&record["status"]) {
        "failed" => 3,
        "verified" => 2,
        "unknown" => 1,
        _ => 0,
    }
}

fn effective(records: &[Value]) -> Result<Vec<Value>> {
    let records = records
        .iter()
        .map(|r| normalize(r, RecordMode::Bound))
        .collect::<Result<Vec<_>>>()?;
    let mut superseded = BTreeSet::new();
    for record in &records {
        if !matches!(record["status"].as_str(), Some("verified" | "failed")) {
            continue;
        }
        for reference in array(&record["supersedes"]) {
            if let Some(target) = records
                .iter()
                .rev()
                .find(|r| r["evidenceRef"] == *reference)
                && target["capability"] == record["capability"]
                && target["environmentDigest"] == record["environmentDigest"]
                && target["scope"] == record["scope"]
                && source_priority(record) >= source_priority(target)
                && timestamp(&record["checkedAt"])? >= timestamp(&target["checkedAt"])?
            {
                superseded.insert(string(reference).to_owned());
            }
        }
    }
    Ok(records
        .into_iter()
        .filter(|r| !superseded.contains(string(&r["evidenceRef"])))
        .collect())
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "evidence.assertion" => assertion(input),
        "evidence.dependencies" => dependencies(input),
        "evidence.normalize" => normalize(input, RecordMode::Bound),
        "evidence.draft" => normalize(input, RecordMode::Draft),
        "evidence.covers" => Ok(covers(&input["actual"], &input["required"]).into()),
        "evidence.effective" => effective(array(input)).map(Value::from),
        "evidence.active" => {
            let cutoff = input["now"].as_i64().ok_or("evidence clock is invalid")?;
            Ok(array(&input["records"])
                .iter()
                .map(|record| normalize(record, RecordMode::Bound))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .filter(|r| {
                    r["environmentDigest"] == input["environmentDigest"]
                        && timestamp(&r["expiresAt"]).is_ok_and(|time| time > cutoff)
                })
                .collect::<Vec<_>>()
                .into())
        }
        "evidence.adoption" => {
            let record = normalize(&input["record"], RecordMode::Bound)?;
            let result = if record["measurementStatus"] != "passed" {
                if record["measurementStatus"] == "failed" {
                    "rejected"
                } else {
                    "not-qualified"
                }
            } else if timestamp(&record["expiresAt"])?
                <= input["now"].as_i64().ok_or("evidence clock is invalid")?
            {
                "expired"
            } else if input["expectedDependencies"]
                .as_object()
                .into_iter()
                .flat_map(serde_json::Map::iter)
                .any(|(kind, expected)| {
                    let actual = &record["dependencies"][kind]["digest"];
                    if let Some(values) = expected.as_array() {
                        !values.contains(actual)
                    } else {
                        actual != expected
                    }
                })
            {
                "dependency-mismatch"
            } else {
                "qualified"
            };
            Ok(result.into())
        }
        "evidence.summary" => {
            let mut records = effective(array(input))?;
            records.sort_by(|a, b| {
                timestamp(&a["checkedAt"])
                    .unwrap()
                    .cmp(&timestamp(&b["checkedAt"]).unwrap())
                    .then_with(|| string(&a["evidenceRef"]).cmp(string(&b["evidenceRef"])))
            });
            let mut summary = serde_json::Map::new();
            for record in records {
                let capability = string(&record["capability"]).to_owned();
                if summary.get(&capability).is_none_or(|current| {
                    (status_priority(&record), source_priority(&record))
                        >= (status_priority(current), source_priority(current))
                }) {
                    summary.insert(capability, record);
                }
            }
            Ok(Value::Object(summary))
        }
        _ => Err("unknown evidence operation".into()),
    }
}
