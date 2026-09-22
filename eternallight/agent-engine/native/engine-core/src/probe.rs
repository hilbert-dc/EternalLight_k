use crate::{
    Result,
    canonical::{array, string},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const PROTOCOLS: [&str; 4] = [
    "responses",
    "chat-completions",
    "messages",
    "generate-content",
];

fn classify(input: &Value) -> &str {
    if input["status"] == 401 {
        return "authentication";
    }
    if input["status"] == 429 {
        return "rateLimited";
    }
    if let Some(category) = input["category"].as_str() {
        return category;
    }
    if matches!(input["name"].as_str(), Some("AbortError" | "TimeoutError")) {
        return "timeout";
    }
    match input["status"].as_u64() {
        Some(404 | 405 | 415) => "unsupportedProtocol",
        Some(429) => "rateLimited",
        Some(400..=499) => "invalidRequest",
        _ => "transport",
    }
}

fn status_category(input: &Value) -> Value {
    if input["status"] == 401 {
        return "authentication".into();
    }
    if input["status"] != 403 {
        return Value::Null;
    }
    let payload = &input["payload"];
    let marker = format!(
        "{} {} {}",
        string(&payload["error"]["type"]),
        string(&payload["error"]["code"]),
        string(&payload["code"])
    )
    .to_lowercase();
    if [
        "auth",
        "api_key",
        "api-key",
        "api key",
        "apikey",
        "credential",
    ]
    .iter()
    .any(|part| marker.contains(part))
    {
        "authentication".into()
    } else {
        "invalidRequest".into()
    }
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "probe.classify" => Ok(classify(input).into()),
        "probe.statusCategory" => Ok(status_category(input)),
        "probe.retryToolChoice" => {
            let payload = &input["payload"];
            let message = payload["error"]["message"]
                .as_str()
                .or_else(|| payload["message"].as_str())
                .unwrap_or("")
                .to_lowercase();
            Ok(json!(
                matches!(input["status"].as_u64(), Some(400 | 422))
                    && ["tool_choice", "tool-choice", "tool choice", "toolchoice"]
                        .iter()
                        .any(|part| message.contains(part))
                    && ["not support", "unsupported", "invalid"]
                        .iter()
                        .any(|part| message.contains(part))
            ))
        }
        "probe.request" => {
            let connection = &input["connection"];
            let mut request = json!({"model":connection["id"],"instructions":"This is a protocol conformance probe. Call the engine_probe tool exactly once with {\"nonce\":\"agent-engine\"}. Do not include user data.","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Run the synthetic protocol probe."}]}],"tools":[{"type":"function","name":"engine_probe","description":"Synthetic side-effect-free protocol probe.","parameters":{"type":"object","properties":{"nonce":{"type":"string","const":"agent-engine"}},"required":["nonce"],"additionalProperties":false},"strict":false}],"stream":input["stream"]==true,"max_output_tokens":connection["declaredLimits"]["outputTokens"].as_u64().unwrap_or(512).min(512)});
            if input["forceToolChoice"] != false {
                request["tool_choice"] = "required".into();
            }
            Ok(request)
        }
        "probe.roundTrip" => {
            let calls: Vec<_> = array(&input["output"])
                .iter()
                .filter(|item| {
                    matches!(
                        item["type"].as_str(),
                        Some("function_call" | "custom_tool_call")
                    )
                })
                .collect();
            if calls.len() != 1
                || calls[0]["type"] != "function_call"
                || calls[0]["name"] != "engine_probe"
            {
                return Err(
                    "probe must return exactly one engine_probe structured tool call".into(),
                );
            }
            let arguments: Value = serde_json::from_str(string(&calls[0]["arguments"]))
                .map_err(|_| "probe engine_probe arguments are not valid JSON")?;
            if arguments != json!({"nonce":"agent-engine"}) {
                return Err("probe engine_probe arguments failed round-trip validation".into());
            }
            Ok(Value::Null)
        }
        "probe.streamingFailure" => {
            let category = classify(input);
            Ok(
                json!({"category":category,"fatal":matches!(category,"authentication"|"rateLimited"),"status":if matches!(category,"unsupportedProtocol"|"invalidRequest") {"failed"} else {"unknown"}}),
            )
        }
        _ => Err("unknown probe operation".into()),
    }
}

/// Owns ordered protocol attempts. A fatal access failure or success seals the
/// session; a caller cannot advance or submit results for a different attempt.
pub struct ProbeSession {
    protocols: Vec<String>,
    index: usize,
    active: bool,
    terminal: bool,
    attempts: Vec<Value>,
}

impl ProbeSession {
    pub fn new(input: &Value) -> Result<Self> {
        let protocols = if !input["protocols"].is_null() {
            input["protocols"].clone()
        } else if input["protocol"] == "auto" {
            json!(PROTOCOLS)
        } else {
            json!([input["protocol"]])
        };
        let protocols = protocols
            .as_array()
            .filter(|p| !p.is_empty())
            .ok_or("verification protocols are required")?;
        let maximum = input["maxAttempts"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 4)
            .ok_or("invalid verification protocol budget")?;
        if protocols.len() as u64 > maximum {
            return Err("verification protocol budget exceeded".into());
        }
        let mut unique = BTreeSet::new();
        for protocol in protocols {
            if !PROTOCOLS.contains(&string(protocol)) || !unique.insert(string(protocol)) {
                return Err("invalid or duplicate verification protocol".into());
            }
        }
        Ok(Self {
            protocols: protocols.iter().map(|p| string(p).to_owned()).collect(),
            index: 0,
            active: false,
            terminal: false,
            attempts: Vec::new(),
        })
    }

    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "next" => {
                if self.active {
                    return Err("verification attempt is already active".into());
                }
                if self.terminal || self.index == self.protocols.len() {
                    return Ok(Value::Null);
                }
                self.active = true;
                Ok(self.protocols[self.index].clone().into())
            }
            "success" | "failure" => {
                if !self.active || input["protocol"] != self.protocols[self.index] {
                    return Err("verification attempt identity mismatch".into());
                }
                let protocol = &self.protocols[self.index];
                let result = if method == "success" {
                    self.terminal = true;
                    self.attempts
                        .push(json!({"protocol":protocol,"status":"verified"}));
                    json!({"attempts":self.attempts})
                } else {
                    let category = classify(&input["error"]);
                    let message: String = string(&input["error"]["message"])
                        .chars()
                        .take(500)
                        .collect();
                    self.terminal = matches!(category, "authentication" | "rateLimited");
                    self.attempts.push(json!({"protocol":protocol,"status":"failed","category":category,"message":message}));
                    json!({"category":category,"message":message,"recordFailure":matches!(category,"unsupportedProtocol"|"invalidRequest"|"authentication")})
                };
                self.active = false;
                self.index += 1;
                Ok(result)
            }
            "result" => {
                if self.active || (!self.terminal && self.index < self.protocols.len()) {
                    return Err("verification is not complete".into());
                }
                Ok(json!({"attempts":self.attempts}))
            }
            _ => Err("unknown verification session operation".into()),
        }
    }
}
