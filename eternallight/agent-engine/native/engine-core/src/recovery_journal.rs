use crate::{Result, recovery_contract};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
enum State {
    Streaming,
    Dispatched,
    Uncertain,
    Terminal,
}
struct Tracked {
    classification: Value,
    state: State,
}
enum Pending {
    Begin(String),
    Request(String, Tracked),
}

/// Proposes persistence operations and advances only after the binding confirms
/// successful IO. Model events never mark a whole run terminal.
#[derive(Default)]
pub struct RecoveryJournal {
    run: Option<String>,
    requests: BTreeMap<String, Tracked>,
    pending: Option<Pending>,
}

impl RecoveryJournal {
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        if method == "commit" {
            match self
                .pending
                .take()
                .ok_or("no pending recovery journal operation")?
            {
                Pending::Begin(run) => {
                    self.run = Some(run);
                    self.requests.clear();
                }
                Pending::Request(id, tracked) => {
                    self.requests.insert(id, tracked);
                }
            }
            return Ok(Value::Null);
        }
        if method == "abort" {
            self.pending = None;
            return Ok(Value::Null);
        }
        if self.pending.is_some() {
            return Err("recovery journal persistence is pending".into());
        }
        match method {
            "begin" => {
                if let Some(run) = &self.run {
                    return Err(format!("recovery run is already active: {run}"));
                }
                let run = input["runId"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 500)
                    .ok_or("invalid recovery run id")?;
                self.pending = Some(Pending::Begin(run.into()));
                Ok(json!({"kind":"persist","method":"beginRun","args":[input]}))
            }
            "end" => {
                self.run = None;
                self.requests.clear();
                Ok(false.into())
            }
            "observe" => self.observe(input),
            _ => Err("unknown recovery journal operation".into()),
        }
    }

    fn observe(&mut self, input: &Value) -> Result<Value> {
        let Some(run) = &self.run else {
            return Ok(Value::Null);
        };
        let event = &input["event"];
        let turn = event["params"]["turn"]["id"]
            .as_str()
            .or_else(|| event["params"]["turnId"].as_str());
        if let Some(turn) = turn.filter(|s| !s.is_empty()) {
            let id = format!("model:{turn}");
            match event["method"].as_str() {
                Some("turn/started") => {
                    if self.requests.contains_key(&id) {
                        return Ok(Value::Null);
                    }
                    if self.requests.len() >= 10_000 {
                        return Err("recovery journal request limit exceeded".into());
                    }
                    self.pending = Some(Pending::Request(
                        id.clone(),
                        Tracked {
                            classification: Value::Null,
                            state: State::Streaming,
                        },
                    ));
                    return Ok(
                        json!({"kind":"persist","method":"recordRequest","args":[run,{"requestId":id,"turnId":turn,"kind":"model","operation":"model-response","state":"streaming"}]}),
                    );
                }
                Some("turn/completed") => {
                    if self
                        .requests
                        .get(&id)
                        .is_none_or(|t| t.state == State::Terminal)
                    {
                        return Ok(Value::Null);
                    }
                    self.pending = Some(Pending::Request(
                        id.clone(),
                        Tracked {
                            classification: Value::Null,
                            state: State::Terminal,
                        },
                    ));
                    return Ok(
                        json!({"kind":"persist","method":"transitionRequest","args":[run,id,"terminal"]}),
                    );
                }
                _ => {}
            }
        }
        let item = &event["params"]["item"];
        let Some(id) = item["id"]
            .as_str()
            .filter(|_| item["type"] == "mcpToolCall")
        else {
            return Ok(Value::Null);
        };
        let id = format!("mcp:{id}");
        let Some(tracked) = self.requests.get(&id) else {
            if self.requests.len() >= 10_000 {
                return Err("recovery journal request limit exceeded".into());
            }
            if !input["classification"].is_object() {
                return Ok(json!({"kind":"classify","toolCall":item}));
            }
            let classification =
                recovery_contract::classification(&input["classification"], &item["tool"])?;
            let hash = if classification["effect"] == "write" {
                recovery_contract::dispatch("recoveryContract.hash", &input["requestHash"])?
            } else {
                Value::Null
            };
            let result = json!({"kind":"persist","method":"recordRequest","args":[run,{"requestId":id,"turnId":turn,"kind":classification["effect"],"operation":classification["operation"],"idempotencyRef":classification["idempotencyRef"],"requestHash":hash,"state":"tool-dispatched"}]});
            self.pending = Some(Pending::Request(
                id,
                Tracked {
                    classification,
                    state: State::Dispatched,
                },
            ));
            return Ok(result);
        };
        if event["method"] != "item/completed" || tracked.state != State::Dispatched {
            return Ok(Value::Null);
        }
        if item["status"] == "completed"
            && !input
                .as_object()
                .is_some_and(|v| v.contains_key("resolution"))
        {
            return Ok(
                json!({"kind":"receipt","classification":tracked.classification,"toolCall":item}),
            );
        }
        let resolution = if item["status"] == "completed" {
            recovery_contract::resolution(&input["resolution"], "receipt")?
        } else {
            Value::Null
        };
        let write = tracked.classification["effect"] == "write";
        let state = if write && (resolution.is_null() || resolution["outcome"] == "unknown") {
            State::Uncertain
        } else {
            State::Terminal
        };
        let command = if write && !resolution.is_null() {
            json!({"kind":"persist","method":"settleRequest","args":[run,id,resolution]})
        } else {
            json!({"kind":"persist","method":"transitionRequest","args":[run,id,if state==State::Uncertain {"uncertain"} else {"terminal"}]})
        };
        self.pending = Some(Pending::Request(
            id,
            Tracked {
                classification: tracked.classification.clone(),
                state,
            },
        ));
        Ok(command)
    }
}
