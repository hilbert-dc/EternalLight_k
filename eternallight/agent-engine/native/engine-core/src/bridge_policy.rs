use crate::Result;
use serde_json::{Value, json};
use std::collections::HashMap;

fn metadata(value: &Value) -> Result<Value> {
    let text = value
        .as_str()
        .filter(|s| s.len() <= 16_384)
        .ok_or("trusted Codex turn metadata is required")?;
    let metadata: Value =
        serde_json::from_str(text).map_err(|_| "trusted Codex turn metadata is invalid")?;
    if metadata["thread_id"].as_str().is_none_or(str::is_empty) {
        return Err("trusted Codex thread identity is required".into());
    }
    for field in ["parent_thread_id", "turn_id", "root_turn_id", "session_id"] {
        if !metadata[field].is_null() && metadata[field].as_str().is_none_or(str::is_empty) {
            return Err(format!("trusted Codex {field} is invalid"));
        }
    }
    let mut identity = json!({"threadId":metadata["thread_id"],"parentThreadId":metadata["parent_thread_id"],"turnId":metadata["turn_id"],"rootTurnId":metadata["root_turn_id"],"sessionId":metadata["session_id"]});
    if let Some(kind) = metadata.get("request_kind") {
        if !matches!(
            kind.as_str(),
            Some("turn" | "prewarm" | "compaction" | "memory")
        ) {
            return Err("trusted Codex request kind is invalid".into());
        }
        identity["requestKind"] = kind.clone();
    }
    Ok(identity)
}

/// Per-execution request policy. The embedding supplies file-presence facts,
/// never the resulting authorization decision or child request counter.
#[derive(Default)]
pub struct BridgePolicy {
    child_requests: HashMap<String, u64>,
    discovery: crate::tool_discovery::PortableDiscovery,
}

impl BridgePolicy {
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "identity" => metadata(input),
            "reset" => {
                self.child_requests.clear();
                self.discovery.reset();
                Ok(Value::Null)
            }
            "route" => {
                let model = input["body"]["model"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or(input["sessionPlan"]["defaultModel"].as_str())
                    .ok_or("default model is missing")?;
                input["sessionPlan"]["routes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|route| route["logicalModel"] == model)
                    .cloned()
                    .ok_or_else(|| format!("model route is not authorized: {model}"))
            }
            "prepare" => self.prepare(input),
            "discoverySnapshot" => self.discovery.snapshot(&input["identity"]),
            "observeDiscovery" => {
                self.discovery.observe(
                    &input["response"],
                    &input["toolDiscovery"],
                    &input["identity"],
                    &input["catalog"],
                )?;
                Ok(Value::Null)
            }
            _ => Err("unknown bridge policy operation".into()),
        }
    }

    fn prepare(&mut self, input: &Value) -> Result<Value> {
        let mut body = input["body"].clone();
        if !body.is_object() {
            return Err("bridge request must be an object".into());
        }
        let maximum = input["outputBudget"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("invalid route output budget")?;
        body["max_output_tokens"] = body["max_output_tokens"]
            .as_u64()
            .filter(|n| *n > 0)
            .unwrap_or(maximum)
            .min(maximum)
            .into();
        let policy = &input["policy"];
        if policy.is_null() {
            return self
                .discovery
                .prepare(body, &input["toolDiscovery"], &input["identity"]);
        }
        let child = !input["identity"]["parentThreadId"].is_null();
        let complete = child && input["childComplete"] == true;
        let mut advance = None;
        let allowed = if complete {
            Value::Array(Vec::new())
        } else if !child {
            policy["rootAllowed"].clone()
        } else if policy["childPhaseFile"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            let field = if input["childPhaseReached"] == true {
                "childAfterPhaseAllowed"
            } else {
                "childBeforePhaseAllowed"
            };
            policy
                .get(field)
                .filter(|v| !v.is_null())
                .unwrap_or(&policy["childAllowed"])
                .clone()
        } else {
            let thread = input["identity"]["threadId"]
                .as_str()
                .ok_or("child thread identity is missing")?;
            let count = self.child_requests.get(thread).copied().unwrap_or(0);
            if count == 0 && self.child_requests.len() >= 10_000 {
                return Err("bridge child policy limit reached".into());
            }
            advance = Some((
                thread.to_owned(),
                count
                    .checked_add(1)
                    .ok_or("bridge child request counter exhausted")?,
            ));
            let field = if count == 0 {
                "childFirstAllowed"
            } else {
                "childSubsequentAllowed"
            };
            policy
                .get(field)
                .filter(|v| !v.is_null())
                .unwrap_or(&policy["childAllowed"])
                .clone()
        };
        let permits = |name: &str| allowed.as_array().into_iter().flatten().any(|v| v == name);
        let tools = body["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                if tool["type"] != "namespace" {
                    return tool["name"]
                        .as_str()
                        .filter(|name| permits(name))
                        .map(|_| tool.clone());
                }
                let namespace = tool["name"].as_str().unwrap_or("");
                let nested: Vec<_> = tool["tools"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|entry| {
                        entry["name"].as_str().is_some_and(|name| {
                            permits(name) || permits(&format!("{namespace}.{name}"))
                        })
                    })
                    .cloned()
                    .collect();
                if nested.is_empty() {
                    return None;
                }
                let mut tool = tool.clone();
                tool["tools"] = nested.into();
                Some(tool)
            })
            .collect::<Vec<_>>();
        if child && !complete && policy["childToolChoiceRequired"] == true && !tools.is_empty() {
            body["tool_choice"] = "required".into();
        }
        if complete {
            body["tool_choice"] = "none".into();
        }
        if body["tool_choice"].is_object() {
            let name = body["tool_choice"]["name"].as_str().unwrap_or("");
            let selected = body["tool_choice"]["namespace"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map_or_else(
                    || name.to_owned(),
                    |namespace| format!("{namespace}.{name}"),
                );
            if !permits(&selected) && !permits(name) {
                return Err(format!(
                    "required tool is outside the execution tool policy: {selected}"
                ));
            }
        }
        body["tools"] = tools.into();
        let body = self
            .discovery
            .prepare(body, &input["toolDiscovery"], &input["identity"])?;
        if let Some((thread, count)) = advance {
            self.child_requests.insert(thread, count);
        }
        Ok(body)
    }
}
