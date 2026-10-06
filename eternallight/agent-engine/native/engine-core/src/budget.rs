use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Budget {
    id: String,
    max_requests: Option<f64>,
    max_tokens: Option<f64>,
    max_duration_ms: Option<f64>,
    started_at: f64,
    requests: u64,
    tokens: f64,
    unknown_tokens: f64,
    reserved_tokens: f64,
    reservations: BTreeMap<String, f64>,
    settlements: BTreeMap<String, Value>,
    overrun: bool,
}

/// Owns budget transitions; only persistence snapshots cross the binding boundary.
pub struct TaskBudget {
    state: Value,
}

impl TaskBudget {
    pub fn new(input: Value) -> Result<Self> {
        Ok(Self {
            state: dispatch("budget.create", input)?,
        })
    }

    pub fn apply(&mut self, method: &str, mut input: Value) -> Result<Value> {
        if !input.is_object() {
            return Err("budget parameters must be an object".into());
        }
        input["state"] = self.state.clone();
        let result = dispatch(method, input)?;
        self.state = result["state"].clone();
        Ok(result["result"].clone())
    }

    pub fn inspect(&self) -> Value {
        self.state.clone()
    }
}

impl Budget {
    fn snapshot(&self) -> Value {
        json!({"id":self.id,"requests":self.requests,"tokens":self.tokens,"unknownTokens":self.unknown_tokens,"reservedTokens":self.reserved_tokens,"maxRequests":self.max_requests,"maxTokens":self.max_tokens,"maxDurationMs":self.max_duration_ms,"startedAt":self.started_at,"overrun":self.overrun})
    }
    fn settle(&mut self, id: &str, usage: &Value, outcome: &str) -> Value {
        if let Some(previous) = self.settlements.get(id) {
            return previous.clone();
        }
        let Some(reserved) = self.reservations.remove(id) else {
            return json!({"status":"unknown-reservation","reservationId":id,"overrun":self.overrun});
        };
        self.reserved_tokens -= reserved;
        if let Some(reported) = usage["total_tokens"]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
        {
            self.tokens += reported;
        } else if outcome == "not-sent" {
            self.requests -= 1;
        } else {
            self.unknown_tokens += reserved;
        }
        self.overrun |= self
            .max_tokens
            .is_some_and(|limit| self.tokens + self.unknown_tokens + self.reserved_tokens > limit);
        let result = json!({"status":if self.overrun {"overrun"} else {"settled"},"reservationId":id,"overrun":self.overrun});
        self.settlements.insert(id.into(), result.clone());
        result
    }
}

pub fn dispatch(method: &str, input: Value) -> Result<Value> {
    if method == "budget.create" {
        let options = &input["options"];
        let limit = |key: &str| -> Result<Option<f64>> {
            let Some(raw) = options.get(key) else {
                return Ok(None);
            };
            if raw.is_null() {
                return Ok(None);
            }
            let value = raw.as_f64().unwrap_or(0.0);
            if !value.is_finite() || value < 1.0 {
                return Err(format!("{key} must be positive"));
            }
            Ok(Some(value))
        };
        let id = match options.get("id") {
            None => Uuid::new_v4().to_string(),
            Some(v) => v
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("task budget id is required")?
                .into(),
        };
        return serde_json::to_value(Budget {
            id,
            max_requests: limit("maxRequests")?,
            max_tokens: limit("maxTokens")?,
            max_duration_ms: limit("maxDurationMs")?,
            started_at: options["startedAt"]
                .as_f64()
                .or(input["now"].as_f64())
                .ok_or("budget timestamp required")?,
            requests: 0,
            tokens: 0.0,
            unknown_tokens: 0.0,
            reserved_tokens: 0.0,
            reservations: BTreeMap::new(),
            settlements: BTreeMap::new(),
            overrun: false,
        })
        .map_err(|e| e.to_string());
    }
    if ["maxRequests", "maxTokens", "maxDurationMs"]
        .iter()
        .any(|key| input["state"].get(key).is_none())
    {
        return Err("invalid budget state".into());
    }
    let mut state: Budget =
        serde_json::from_value(input["state"].clone()).map_err(|_| "invalid budget state")?;
    let result = match method {
        "budget.reserve" => {
            let parameters = &input["parameters"];
            let integer = |key: &str| -> Result<f64> {
                let value = if parameters.get(key).is_none() {
                    0.0
                } else {
                    parameters[key].as_f64().unwrap_or(-1.0)
                };
                if value < 0.0 || value.fract() != 0.0 {
                    Err(format!("{key} must be a non-negative integer"))
                } else {
                    Ok(value)
                }
            };
            let tokens = integer("estimatedInputTokens")? + integer("maxOutputTokens")?;
            let now = input["now"].as_f64().ok_or("budget timestamp required")?;
            if state
                .max_duration_ms
                .is_some_and(|limit| now - state.started_at >= limit)
            {
                return Err("task duration budget exhausted".into());
            }
            if state.overrun {
                return Err("task token budget exhausted".into());
            }
            if state
                .max_requests
                .is_some_and(|limit| state.requests as f64 >= limit)
            {
                if parameters["deferIfBusy"] == true && !state.reservations.is_empty()
                    && state.max_requests.is_some_and(|limit| (state.requests as f64 - state.reservations.len() as f64) < limit)
                    && state.max_tokens.is_none_or(|limit| state.tokens + state.unknown_tokens + tokens <= limit) {
                    return Ok(json!({"state":state,"result":{"waiting":true}}));
                }
                return Err("task request budget exhausted".into());
            }
            if state.max_tokens.is_some_and(|limit| {
                state.tokens + state.unknown_tokens + state.reserved_tokens + tokens > limit
            }) {
                if parameters["deferIfBusy"] == true && state.reserved_tokens > 0.0
                    && state.max_tokens.is_some_and(|limit| state.tokens + state.unknown_tokens + tokens <= limit) {
                    return Ok(json!({"state":state,"result":{"waiting":true}}));
                }
                return Err("task token budget exhausted".into());
            }
            let id = Uuid::new_v4().to_string();
            state.requests += 1;
            state.reserved_tokens += tokens;
            state.reservations.insert(id.clone(), tokens);
            id.into()
        }
        "budget.settle" => state.settle(
            input["id"].as_str().ok_or("reservation id required")?,
            &input["usage"],
            input["outcome"].as_str().unwrap_or("completed"),
        ),
        "budget.observe" => {
            if let Some(tokens) = input["usage"]["total_tokens"]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
            {
                state.tokens += tokens;
                state.overrun |= state.max_tokens.is_some_and(|limit| {
                    state.tokens + state.unknown_tokens + state.reserved_tokens > limit
                });
                json!({"status":if state.overrun {"overrun"} else {"observed"},"overrun":state.overrun})
            } else {
                Value::Null
            }
        }
        "budget.unknown" => {
            for id in state.reservations.keys().cloned().collect::<Vec<_>>() {
                state.settle(&id, &Value::Null, "unknown");
            }
            state.snapshot()
        }
        "budget.remaining" => {
            let now = input["now"].as_f64().ok_or("budget timestamp required")?;
            state
                .max_duration_ms
                .map(|limit| (state.started_at + limit - now).max(0.0))
                .map_or(Value::Null, Value::from)
        }
        "budget.snapshot" => state.snapshot(),
        _ => return Err("unknown budget operation".into()),
    };
    Ok(json!({"state":state,"result":result}))
}
