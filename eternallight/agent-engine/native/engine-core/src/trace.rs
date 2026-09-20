use crate::{Result, canonical::array};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct ExecutionTrace {
    run_id: String,
    max_events: usize,
    max_string: usize,
    events: Vec<Value>,
    dropped: u64,
    sequences: BTreeMap<String, u64>,
}

fn required<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{label} is required"))
}

fn truncate(text: &str, maximum: usize) -> String {
    let mut length = 0;
    text.chars()
        .take_while(|c| {
            length += c.len_utf16();
            length <= maximum
        })
        .collect()
}

fn bounded(value: &Value, maximum: usize, depth: usize) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(text) => truncate(text, maximum).into(),
        Value::Array(_) | Value::Object(_) if depth >= 4 => "[DEPTH_LIMIT]".into(),
        Value::Array(values) => values
            .iter()
            .take(80)
            .map(|v| bounded(v, maximum, depth + 1))
            .collect(),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .take(48)
                .map(|(k, v)| (truncate(k, maximum), bounded(v, maximum, depth + 1)))
                .collect(),
        ),
    }
}

fn delta(kind: &str, details: &Value) -> bool {
    kind == "agent.event"
        && details["method"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("delta")
}

impl ExecutionTrace {
    pub fn new(options: &Value) -> Result<Self> {
        let run_id = required(&options["runId"], "trace runId")?.to_owned();
        let max_events = options
            .get("maxEvents")
            .unwrap_or(&json!(256))
            .as_u64()
            .filter(|n| *n > 0 && *n <= 65536)
            .ok_or("trace maxEvents must be positive and at most 65536")?
            as usize;
        let max_string = options
            .get("maxStringLength")
            .unwrap_or(&json!(256))
            .as_u64()
            .filter(|n| *n >= 32 && *n <= 65536)
            .ok_or("trace maxStringLength must be at least 32 and at most 65536")?
            as usize;
        Ok(Self {
            run_id,
            max_events,
            max_string,
            events: Vec::new(),
            dropped: 0,
            sequences: BTreeMap::new(),
        })
    }

    pub fn record(&mut self, input: &Value) -> Result<Value> {
        let source = required(&input["source"], "trace source")?;
        let kind = required(&input["type"], "trace event type")?;
        // Cap both source count and keys: bounded events alone do not bound sequence state.
        let source = truncate(source, self.max_string);
        if self.sequences.len() >= 1024 && !self.sequences.contains_key(&source) {
            return Err("trace source limit exceeded".into());
        }
        let sequence = self.sequences.entry(source.clone()).or_default();
        *sequence += 1;
        let event = json!({"version":1,"runId":self.run_id,"source":source,"sourceSequence":sequence,"occurredAtMs":input["now"],"type":truncate(kind,self.max_string),"details":bounded(&input["details"],self.max_string,0)});
        if self.events.len() >= self.max_events {
            self.dropped += 1;
            let milestone = source == "mcp"
                || source == "bridge"
                || (kind == "agent.event" && !delta(kind, &input["details"]));
            if !milestone {
                return Ok(Value::Null);
            }
            let Some(index) = self
                .events
                .iter()
                .position(|e| delta(e["type"].as_str().unwrap_or(""), &e["details"]))
            else {
                return Ok(Value::Null);
            };
            self.events.remove(index);
        }
        self.events.push(event.clone());
        Ok(event)
    }

    pub fn snapshot(&self) -> Value {
        json!({"version":1,"runId":self.run_id,"events":self.events,"droppedCount":self.dropped,"incomplete":self.dropped>0})
    }
}

fn details<'a>(trace: &'a Value, kind: &'a str) -> impl Iterator<Item = &'a Value> {
    array(&trace["events"])
        .iter()
        .filter(move |e| e["type"] == kind)
        .map(|e| &e["details"])
}

fn matches_tool(tool: &Value, expected: &str) -> bool {
    if tool.as_str() == Some(expected) || tool["name"].as_str() == Some(expected) {
        return true;
    }
    match (tool["namespace"].as_str(), tool["name"].as_str()) {
        (Some(namespace), Some(name)) => format!("{namespace}.{name}") == expected,
        _ => false,
    }
}

pub fn classify(input: &Value) -> Value {
    let trace = &input["trace"];
    let options = &input["options"];
    let expected = options["expectedTool"].as_str().unwrap_or("");
    let timeout = options["timedOut"] == true || options["budgetExhausted"] == true;
    let fallback = if timeout {
        "budget-or-timeout"
    } else {
        "inconclusive"
    };
    for (reason, category) in [
        ("identity", "identity-rejected"),
        ("arguments", "argument-rejected"),
    ] {
        if details(trace, "mcp.call.rejected").any(|e| e["reason"] == reason) {
            return category.into();
        }
    }
    let failed = details(trace, "agent.event")
        .any(|e| e["method"] == "error" || e["status"] == "systemError");
    let in_scope = |e: &&Value| {
        (options["requestScope"] == "root")
            == e["parentThreadId"].as_str().is_none_or(str::is_empty)
    };
    let requests: Vec<_> = details(trace, "bridge.request.prepared")
        .filter(in_scope)
        .collect();
    if requests.is_empty() {
        return if failed || details(trace, "agent.lifecycle.failed").next().is_some() {
            "lifecycle-error"
        } else {
            fallback
        }
        .into();
    }
    if !requests.iter().any(|e| {
        ["tools", "encodedTools"]
            .iter()
            .any(|field| array(&e[field]).iter().any(|t| matches_tool(t, expected)))
    }) {
        return if failed {
            "lifecycle-error"
        } else {
            "tool-not-exposed"
        }
        .into();
    }
    let responses: Vec<_> = details(trace, "bridge.response.completed")
        .filter(in_scope)
        .collect();
    if !responses.iter().any(|e| {
        array(&e["toolCalls"])
            .iter()
            .any(|c| matches_tool(c, expected))
    }) {
        return if responses.iter().any(|e| e["textLooksLikeToolCall"] == true) {
            "tool-call-as-text"
        } else if timeout {
            "budget-or-timeout"
        } else {
            "tool-not-selected"
        }
        .into();
    }
    let Some(completed) = details(trace, "mcp.call.completed").find(|e| e["toolName"] == expected)
    else {
        return if details(trace, "bridge.response.failed").any(|e| {
            array(&e["toolCalls"])
                .iter()
                .any(|c| matches_tool(c, expected))
                || (!expected.is_empty()
                    && e["errorMessage"].as_str().unwrap_or("").contains(expected))
        }) {
            "conversion-error"
        } else {
            "execution-error"
        }
        .into();
    };
    if !requests
        .iter()
        .any(|e| array(&e["toolResultCallIds"]).contains(&completed["callId"]))
    {
        return "transport-result-missing".into();
    }
    if options["resultConsumed"] == false {
        return "result-consumption-failed".into();
    }
    fallback.into()
}
