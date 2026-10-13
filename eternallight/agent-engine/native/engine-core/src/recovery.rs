use crate::{Result, storage};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum State {
    Prepared,
    Streaming,
    ToolDispatched,
    ReceiptObserved,
    Uncertain,
    Terminal,
}

fn state(value: &Value) -> Result<State> {
    serde_json::from_value(value.clone()).map_err(|_| format!("invalid request state: {value}"))
}

fn unresolved(request: &Value) -> bool {
    request["kind"] == "write"
        && matches!(
            request["state"].as_str(),
            Some("tool-dispatched" | "uncertain")
        )
        && request["receiptRef"].as_str().is_none_or(str::is_empty)
}

fn requests(run: &Value) -> impl Iterator<Item = &Value> {
    let mut requests: Vec<_> = run["requests"]
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::values)
        .collect();
    requests.sort_by_key(|request| request["sequence"].as_u64().unwrap_or(0));
    requests.into_iter()
}

fn run_mut<'a>(document: &'a mut Value, input: &Value) -> Result<&'a mut Value> {
    let id = input["runId"]
        .as_str()
        .ok_or("recovery run id is required")?;
    document["runs"]
        .get_mut(id)
        .ok_or_else(|| format!("unknown run: {id}"))
}

fn request_mut<'a>(document: &'a mut Value, input: &Value) -> Result<&'a mut Value> {
    let id = input["requestId"]
        .as_str()
        .ok_or("recovery request id is required")?;
    run_mut(document, input)?["requests"]
        .get_mut(id)
        .ok_or_else(|| format!("unknown request: {id}"))
}

/// Serializes updates across instances and processes; no caller-supplied document is trusted.
pub fn execute(path: &Path, method: &str, input: &Value) -> Result<Value> {
    let mut lock_path = path.as_os_str().to_owned();
    lock_path.push(".lock");
    let lock = storage::lock_file(Path::new(&lock_path))?;
    FileExt::lock_exclusive(&lock).map_err(|e| e.to_string())?;
    let mut document = storage::read_json(path)?.unwrap_or_else(|| json!({"version":2,"runs":{}}));
    if document["version"] != 2
        || document["runs"].as_object().is_none_or(|runs| {
            runs.values().any(|run| {
                run["requests"].as_object().is_none_or(|entries| {
                    entries
                        .values()
                        .any(|entry| !entry.is_object() || state(&entry["state"]).is_err())
                })
            })
        })
    {
        return Err("unsupported recovery index".into());
    }
    if matches!(method, "read" | "candidates" | "unfinished" | "report") {
        return query(&document, method, input);
    }
    let result = update(&mut document, method, input)?;
    storage::durable_replace_json(path, &document)?;
    Ok(result.unwrap_or(document))
}

fn update(document: &mut Value, method: &str, input: &Value) -> Result<Option<Value>> {
    match method {
        "begin" => {
            let id = input["runId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or("recovery run id is required")?;
            if document["runs"].get(id).is_some() {
                return Err(format!("run already exists: {id}"));
            }
            let sequence = document["runs"].as_object().unwrap().len();
            document["runs"][id] = json!({"runId":id,"planId":input["planId"],"threadId":input["threadId"],"projectId":input["projectId"],"operation":input["operation"],"createdAt":input["createdAt"],"terminalAt":null,"runtimeStatus":null,"reconciledAt":null,"recoveryFence":null,"reconciliationRequired":null,"requests":{}});
            document["runs"][id]["sequence"] = sequence.into();
        }
        "record" => {
            state(&input["state"])?;
            if !matches!(input["kind"].as_str(), Some("model" | "read" | "write")) {
                return Err(format!("invalid request kind: {}", input["kind"]));
            }
            let id = input["requestId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or("recovery request id is required")?;
            let run = run_mut(document, input)?;
            if run["requests"].get(id).is_some() {
                return Err(format!("request already exists: {id}"));
            }
            let sequence = run["requests"].as_object().unwrap().len();
            run["requests"][id] = json!({"requestId":id,"turnId":input["turnId"],"kind":input["kind"],"operation":input["operation"],"idempotencyRef":input["idempotencyRef"],"requestHash":input["requestHash"],"state":input["state"],"updatedAt":input["updatedAt"],"receiptRef":null,"outcome":null});
            run["requests"][id]["sequence"] = sequence.into();
        }
        "transition" => {
            let next = state(&input["state"])?;
            let request = request_mut(document, input)?;
            let allowed = match state(&request["state"])? {
                State::Prepared => matches!(next, State::Streaming | State::Terminal),
                State::Streaming => matches!(next, State::ToolDispatched | State::Terminal),
                State::ToolDispatched => matches!(
                    next,
                    State::ReceiptObserved | State::Uncertain | State::Terminal
                ),
                State::ReceiptObserved => next == State::Terminal,
                State::Uncertain => matches!(next, State::ReceiptObserved | State::Terminal),
                State::Terminal => false,
            };
            if !allowed {
                return Err(format!(
                    "invalid request transition: {} -> {}",
                    request["state"], input["state"]
                ));
            }
            request["state"] = input["state"].clone();
            request["updatedAt"] = input["updatedAt"].clone();
            if !input["receiptRef"].is_null() {
                request["receiptRef"] = input["receiptRef"].clone();
            }
        }
        "terminal" => {
            let run = run_mut(document, input)?;
            if requests(run).any(unresolved) {
                return Ok(Some(false.into()));
            }
            run["terminalAt"] = input["terminalAt"].clone();
            return Ok(Some(true.into()));
        }
        "resolve" => {
            if input["resolution"] != "manual-reviewed" {
                return Err("invalid recovery resolution".into());
            }
            let run = run_mut(document, input)?;
            if !run["terminalAt"].is_null() {
                return Err("run is already terminal".into());
            }
            if !requests(run).any(unresolved) {
                return Err("run does not require manual review".into());
            }
            run["resolution"] = input["resolution"].clone();
            run["terminalAt"] = input["terminalAt"].clone();
        }
        "reconcile" => {
            let run = run_mut(document, input)?;
            run["runtimeStatus"] = input["runtimeStatus"].clone();
            run["reconciledAt"] = input["reconciledAt"].clone();
            run["recoveryFence"] = input["fence"].clone();
            run["reconciliationRequired"] = Value::Null;
            if matches!(
                input["runtimeStatus"].as_str(),
                Some("completed" | "failed" | "cancelled")
            ) && !requests(run).any(unresolved)
            {
                run["terminalAt"] = input["reconciledAt"].clone();
            }
        }
        "required" => {
            let ids = input["runIds"]
                .as_array()
                .ok_or("invalid recovery run ids")?;
            if ids.iter().any(|id| !id.is_string()) {
                return Err("invalid recovery run ids".into());
            }
            for id in ids {
                if let Some(run) = document["runs"]
                    .get_mut(id.as_str().unwrap())
                    .filter(|run| run["terminalAt"].is_null())
                {
                    let reason: String = input["reason"]
                        .as_str()
                        .unwrap_or("reconciliation required")
                        .chars()
                        .take(500)
                        .collect();
                    run["reconciliationRequired"] =
                        json!({"reason":reason,"updatedAt":input["updatedAt"]});
                }
            }
        }
        "settle" => {
            if !matches!(
                input["outcome"].as_str(),
                Some("committed" | "rejected" | "cancelled-before-execution" | "unknown")
            ) {
                return Err("invalid recovery outcome".into());
            }
            if input["outcome"] == "committed"
                && input["receiptRef"].as_str().is_none_or(str::is_empty)
            {
                return Err("committed recovery outcome requires a receipt reference".into());
            }
            let request = request_mut(document, input)?;
            if request["kind"] != "write"
                || !matches!(
                    request["state"].as_str(),
                    Some("tool-dispatched" | "uncertain")
                )
            {
                return Err("request is not awaiting write reconciliation".into());
            }
            request["state"] = if input["outcome"] == "unknown" {
                "uncertain"
            } else {
                "terminal"
            }
            .into();
            request["receiptRef"] = input["receiptRef"].clone();
            request["updatedAt"] = input["updatedAt"].clone();
            request["outcome"] = input["outcome"].clone();
        }
        "observe" => observe(run_mut(document, input)?, input)?,
        _ => return Err("unknown recovery operation".into()),
    }
    Ok(None)
}

fn observe(run: &mut Value, input: &Value) -> Result<()> {
    let operations = input["operations"]
        .as_array()
        .ok_or("runtime operations must be an array")?;
    for operation in operations {
        if operation["run_id"] != run["runId"]
            || !operation["tool_call_id"].is_string()
            || !operation["tool_name"].is_string()
        {
            return Err("runtime operation is not bound to the recovery run".into());
        }
        let id = format!("mcp:{}", operation["tool_call_id"].as_str().unwrap());
        if run["requests"].get(&id).is_none() {
            let sequence = run["requests"].as_object().unwrap().len();
            run["requests"][&id] = json!({"sequence":sequence,"requestId":id,"turnId":operation["turn_id"],"kind":"write","operation":operation["tool_name"],"idempotencyRef":operation["idempotency_ref"],"requestHash":operation["request_hash"],"state":"tool-dispatched","updatedAt":input["updatedAt"],"receiptRef":null,"outcome":null});
        }
        let wrong_thread =
            !operation["thread_id"].is_null() && operation["thread_id"] != run["threadId"];
        let request = &mut run["requests"][&id];
        if request["kind"] != "write"
            || request["operation"] != operation["tool_name"]
            || (!request["requestHash"].is_null()
                && request["requestHash"] != operation["request_hash"])
            || (!request["idempotencyRef"].is_null()
                && !operation["idempotency_ref"].is_null()
                && request["idempotencyRef"] != operation["idempotency_ref"])
            || wrong_thread
        {
            return Err(format!(
                "runtime operation conflicts with recovery request: {id}"
            ));
        }
        request["updatedAt"] = operation
            .get("updated_at")
            .filter(|value| !value.is_null())
            .unwrap_or(&input["updatedAt"])
            .clone();
        if request["requestHash"].is_null() {
            request["requestHash"] = operation["request_hash"].clone();
        }
        let operation_id = operation["operation_id"]
            .as_str()
            .ok_or("runtime operation id is required")?;
        match operation["state"].as_str() {
            Some("committed") => {
                let receipt = operation["receipt_ref"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or("committed runtime operation requires a domain receipt")?;
                request["state"] = "terminal".into();
                request["outcome"] = "committed".into();
                request["receiptRef"] = receipt.into();
            }
            Some(status @ ("rejected" | "cancelled-before-execution")) => {
                request["state"] = "terminal".into();
                request["outcome"] = status.into();
                request["receiptRef"] = format!("runtime:operation:{operation_id}:{status}").into();
            }
            Some("accepted" | "outcome-unknown") => {
                request["state"] = "uncertain".into();
                request["outcome"] = operation["state"].clone();
                request["receiptRef"] = Value::Null;
            }
            _ => {
                return Err(format!(
                    "unsupported runtime operation state: {}",
                    operation["state"]
                ));
            }
        }
    }
    Ok(())
}

fn query(document: &Value, method: &str, input: &Value) -> Result<Value> {
    if method == "read" {
        return Ok(document.clone());
    }
    let mut result = Vec::new();
    let mut runs: Vec<_> = document["runs"].as_object().unwrap().values().collect();
    runs.sort_by_key(|run| run["sequence"].as_u64().unwrap_or(0));
    for run in runs {
        if !run["terminalAt"].is_null() {
            continue;
        }
        match method {
            "unfinished" => {
                if run["projectId"].as_str().is_some_and(|v| !v.is_empty()) {
                    result.push(json!({"runId":run["runId"],"projectId":run["projectId"]}));
                }
            }
            "candidates" => {
                if !input["runId"].is_null() && input["runId"] != run["runId"] {
                    continue;
                }
                for request in requests(run)
                    .filter(|request| unresolved(request) && !request["requestHash"].is_null())
                {
                    result.push(json!({"runId":run["runId"],"projectId":run["projectId"],"requestId":request["requestId"],"operation":request["operation"],"idempotencyRef":request["idempotencyRef"],"requestHash":request["requestHash"]}));
                }
            }
            "report" => {
                let entries: Vec<_> = requests(run)
                    .map(|request| {
                        let mut entry = request.clone();
                        let object = entry.as_object_mut().unwrap();
                        object.remove("sequence");
                        object.remove("idempotencyRef");
                        object.remove("requestHash");
                        if request["outcome"].is_null() {
                            object.remove("outcome");
                        }
                        entry["hasIdempotencyRef"] = (!request["idempotencyRef"].is_null()).into();
                        entry["hasRequestHash"] = (!request["requestHash"].is_null()).into();
                        entry["recovery"] = if unresolved(request) {
                            "manual-reconciliation"
                        } else if request["state"] == "terminal" {
                            "none"
                        } else {
                            "resume-core"
                        }
                        .into();
                        entry
                    })
                    .collect();
                let mut entry = run.clone();
                entry.as_object_mut().unwrap().remove("sequence");
                entry.as_object_mut().unwrap().remove("createdAt");
                entry.as_object_mut().unwrap().remove("terminalAt");
                entry["requests"] = entries.into();
                result.push(entry);
            }
            _ => return Err("unknown recovery query".into()),
        }
    }
    Ok(result.into())
}
