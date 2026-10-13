use crate::{
    Result, behavior,
    canonical::{array, string},
    digest::digest,
};
use serde_json::{Value, json};

fn positive(input: &Value, label: &str, fallback: u64) -> Result<u64> {
    if input.is_null() {
        return Ok(fallback);
    }
    input
        .as_u64()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
        .ok_or_else(|| format!("{label} must be a positive integer"))
}
fn dependencies(input: &Value) -> Result<Value> {
    let entries = input
        .as_object()
        .ok_or("behavior runtimeDependencies must be an object")?;
    if entries.len() > 32 {
        return Err("behavior runtimeDependencies exceed the entry limit".into());
    }
    let mut output = json!({});
    for (name, value) in entries {
        if name.len() > 80
            || !name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(format!(
                "behavior runtime dependency name is invalid: {name}"
            ));
        }
        let hash = string(&value["digest"]);
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(format!(
                "behavior runtime dependency digest is invalid: {name}"
            ));
        }
        output[name] = json!({"digest":hash});
        if let Some(reference) = value.get("ref") {
            let text = reference
                .as_str()
                .filter(|s| !s.is_empty() && s.encode_utf16().count() <= 500)
                .ok_or_else(|| format!("behavior runtime dependency ref is invalid: {name}"))?;
            output[name]["ref"] = text.into();
        }
    }
    Ok(output)
}
pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "evaluation.prepare" => {
            let deps = dependencies(input.get("runtimeDependencies").unwrap_or(&json!({})))?;
            if deps["engineBuild"].is_null() {
                return Err("behavior runtime dependency is required: engineBuild".into());
            }
            let executor = &input["executor"];
            if !executor.is_object() {
                return Err("behavior command executor is required".into());
            }
            if string(&executor["command"]).trim().is_empty() {
                return Err("behavior executor command is required".into());
            }
            let args = executor.get("args").cloned().unwrap_or_else(|| json!([]));
            if !args.is_array() || array(&args).iter().any(|v| !v.is_string()) {
                return Err("behavior executor args are invalid".into());
            }
            let cwd = executor.get("cwd").unwrap_or(&input["cwd"]);
            if cwd.as_str().is_none_or(str::is_empty) {
                return Err("behavior executor cwd is invalid".into());
            }
            let env = executor
                .get("environment")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if env.as_object().is_none_or(|e| {
                e.iter().any(|(k, v)| {
                    !v.is_string()
                        || !k
                            .as_bytes()
                            .first()
                            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                        || !k.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                })
            }) {
                return Err("behavior executor environment is invalid".into());
            }
            let frozen = json!({"command":executor["command"],"args":args,"cwd":cwd,"environment":env,"timeoutMs":positive(&executor["timeoutMs"],"behavior executor timeoutMs",120_000)?,"maxOutputBytes":positive(&executor["maxOutputBytes"],"behavior executor maxOutputBytes",1_048_576)?});
            Ok(
                json!({"executor":frozen,"taskSet":behavior::task_set(&input["taskSet"])?,"runtimeDependencies":deps,"repetitions":positive(&input["repetitions"],"behavior repetitions",1)?}),
            )
        }
        "evaluation.report" => {
            if !matches!(input["mode"].as_str(), Some("execute" | "samples")) {
                return Err("behavior evaluation mode is invalid".into());
            }
            chrono::DateTime::parse_from_rfc3339(string(&input["generatedAt"]))
                .map_err(|_| "invalid evaluation timestamp")?;
            let task = if input["taskSet"].is_null() {
                Value::Null
            } else {
                let task = behavior::task_set(&input["taskSet"])?;
                json!({"id":task["taskSetId"],"version":task["taskSetVersion"],"digest":digest(&task)})
            };
            let policy = input.get("policy").cloned().unwrap_or_else(|| json!({}));
            let mut evaluation = behavior::dispatch(
                "behavior.evaluate",
                &json!({"samples":input["samples"],"options":policy}),
            )?;
            evaluation["evaluationId"] = digest(&evaluation).into();
            let executor = &input["executor"];
            let mut body = json!({"version":2,"mode":input["mode"],"generatedAt":input["generatedAt"],"authorizationEffect":"none","taskSet":task,"scorer":{"id":"execution-facts-v2","digest":digest(&json!({"id":"execution-facts-v2","semantics":"content-assertions-plus-authoritative-tool-and-lineage-facts"}))},"executor":if executor.is_null(){Value::Null}else{json!({"digest":digest(&json!({"command":executor["command"],"args":executor["args"],"cwd":executor["cwd"],"timeoutMs":executor["timeoutMs"],"maxOutputBytes":executor["maxOutputBytes"],"environment":executor["environment"]})),"command":executor["command"],"args":executor["args"]})},"runtimeDependencies":dependencies(input.get("runtimeDependencies").unwrap_or(&json!({})))?,"samples":input["samples"],"budget":input["budget"],"policy":policy,"evaluation":evaluation});
            body["reportId"] = digest(&body).into();
            Ok(body)
        }
        "evaluation.mode" => match input["mode"].as_str() {
            Some("execute") => Ok("execute".into()),
            Some("samples") | None => {
                if !input["samples"].is_array() {
                    return Err("sample report mode requires samples[]".into());
                }
                Ok("samples".into())
            }
            _ => Err("behavior evaluation mode must be execute or samples".into()),
        },
        _ => Err("unknown evaluation report operation".into()),
    }
}
