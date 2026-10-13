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
    reservation_definitions: BTreeMap<String, Value>,
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
    fn settle(&mut self, id: &str, usage: &Value, outcome: &str) -> Result<Value> {
        let reported = usage["total_tokens"].as_f64().filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= 9_007_199_254_740_991.0);
        if let Some(previous) = self.settlements.get(id) {
            return Ok(previous.clone());
        }
        let Some(reserved) = self.reservations.remove(id) else {
            return Ok(json!({"status":"unknown-reservation","reservationId":id,"overrun":self.overrun}));
        };
        self.reserved_tokens -= reserved;
        if let Some(reported) = reported {
            self.tokens += reported;
        } else if outcome == "not-sent" {
            self.requests -= 1;
            if let Some(definition) = self.reservation_definitions.get_mut(id) {
                definition["notSent"] = true.into();
            }
        } else {
            self.unknown_tokens += reserved;
        }
        self.overrun |= self
            .max_tokens
            .is_some_and(|limit| self.tokens + self.unknown_tokens + self.reserved_tokens > limit);
        let result = json!({"status":if self.overrun {"overrun"} else {"settled"},"reservationId":id,"overrun":self.overrun,"usageState":if reported.is_some(){"known"}else if outcome=="not-sent"{"not-sent"}else{"unknown"},"reportedTokens":reported,"unknownTokens":if reported.is_none() && outcome!="not-sent"{reserved}else{0.0}});
        self.settlements.insert(id.into(), result.clone());
        Ok(result)
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
            reservation_definitions: BTreeMap::new(),
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
            // Unknown cost blocks finite token authority. Metering-only budgets
            // retain it while admitting separately charged new requests.
            if state.unknown_tokens > 0.0 && state.max_tokens.is_some() {
                return Err("task provider usage is unknown".into());
            }
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
            let input_tokens = integer("estimatedInputTokens")?;
            let output_tokens = integer("maxOutputTokens")?;
            let tokens = input_tokens + output_tokens;
            let explicit_id = parameters
                .get("reservationId")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|id| {
                            !id.is_empty() && id.len() <= 200 && !id.chars().any(char::is_control)
                        })
                        .ok_or("invalid trusted reservation id")
                })
                .transpose()?;
            let request_identity = parameters.get("requestIdentity").map(|value| {
                value.as_str().filter(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
                    .ok_or("invalid trusted request identity")
            }).transpose()?;
            if let Some(identity) = request_identity {
                for (id, previous) in &state.reservation_definitions {
                    if previous["requestIdentity"] != identity || previous["notSent"] == true { continue; }
                    if state.settlements.get(id).is_some_and(|receipt| receipt["usageState"] == "unknown") {
                        return Err("task provider usage is unknown".into());
                    }
                    if state.reservations.contains_key(id) {
                        return Err("task provider request already dispatched".into());
                    }
                }
            }
            let mut definition = json!({"estimatedInputTokens":input_tokens,"maxOutputTokens":output_tokens,"notSent":false});
            if let Some(identity) = request_identity { definition["requestIdentity"] = identity.into(); }
            if let Some(id) = explicit_id {
                // An unknown dispatch cannot be replayed as the same charge,
                // even when future requests have no token ceiling.
                if state.settlements.get(id).is_some_and(|s| s["usageState"] == "unknown") {
                    return Err("task provider usage is unknown".into());
                }
                if let Some(previous) = state.reservation_definitions.get(id) {
                    if previous["notSent"] == true {
                        return Err("trusted reservation id was retired without dispatch".into());
                    }
                    if *previous != definition {
                        return Err("trusted reservation id parameter conflict".into());
                    }
                    return Ok(json!({"state":state,"result":id}));
                }
                if state.reservations.contains_key(id) || state.settlements.contains_key(id) {
                    return Err("trusted reservation id conflicts with another reservation".into());
                }
            }
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
                if parameters["deferIfBusy"] == true
                    && !state.reservations.is_empty()
                    && state.max_requests.is_some_and(|limit| {
                        (state.requests as f64 - state.reservations.len() as f64) < limit
                    })
                    && state
                        .max_tokens
                        .is_none_or(|limit| state.tokens + state.unknown_tokens + tokens <= limit)
                {
                    return Ok(json!({"state":state,"result":{"waiting":true}}));
                }
                return Err("task request budget exhausted".into());
            }
            if state.max_tokens.is_some_and(|limit| {
                state.tokens + state.unknown_tokens + state.reserved_tokens + tokens > limit
            }) {
                if parameters["deferIfBusy"] == true
                    && state.reserved_tokens > 0.0
                    && state
                        .max_tokens
                        .is_some_and(|limit| state.tokens + state.unknown_tokens + tokens <= limit)
                {
                    return Ok(json!({"state":state,"result":{"waiting":true}}));
                }
                return Err("task token budget exhausted".into());
            }
            let id = explicit_id
                .map(str::to_owned)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            state.requests += 1;
            state.reserved_tokens += tokens;
            state.reservations.insert(id.clone(), tokens);
            if explicit_id.is_some() || request_identity.is_some() {
                state.reservation_definitions.insert(id.clone(), definition);
            }
            id.into()
        }
        "budget.settle" => state.settle(
            input["id"].as_str().ok_or("reservation id required")?,
            &input["usage"],
            input["outcome"].as_str().unwrap_or("completed"),
        )?,
        "budget.reconcile" => {
            let id = input["id"].as_str().ok_or("reservation id required")?;
            let receipt_ref = input["receiptRef"].as_str().filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)).ok_or("trusted usage receipt reference required")?;
            let actual = input["usage"]["total_tokens"].as_f64().filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= 9_007_199_254_740_991.0).ok_or("invalid reported usage")?;
            let previous = state.settlements.get(id).ok_or("unknown reservation receipt")?;
            if previous["usageState"] == "known" && previous["receiptRef"] == receipt_ref && previous["reportedTokens"].as_f64() == Some(actual) {
                previous.clone()
            } else {
                if previous["usageState"] != "unknown" { return Err("conflicting usage reconciliation".into()); }
                let reserved = previous["unknownTokens"].as_f64().ok_or("unknown settlement amount missing")?;
                let before = state.snapshot();
                state.unknown_tokens -= reserved;
                state.tokens += actual;
                state.overrun = state.max_tokens.is_some_and(|limit| state.tokens + state.unknown_tokens + state.reserved_tokens > limit);
                let result = json!({"status":if state.overrun {"overrun"} else {"settled"},"reservationId":id,"overrun":state.overrun,"usageState":"known","reportedTokens":actual,"unknownTokens":0,"receiptRef":receipt_ref,"reconciliation":{"before":before,"after":state.snapshot(),"receiptRef":receipt_ref}});
                state.settlements.insert(id.into(),result.clone());
                result
            }
        }
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
                state.settle(&id, &Value::Null, "unknown")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PersistentBudget;

    fn reserve(budget: &mut TaskBudget, parameters: Value) -> Value {
        budget
            .apply(
                "budget.reserve",
                json!({"parameters":parameters,"now":1010}),
            )
            .unwrap()
    }

    #[test]
    fn logical_identity_fences_unknown_and_inflight_without_freezing_new_metered_requests() {
        let mut budget = TaskBudget::new(json!({"options":{},"now":1000})).unwrap();
        let parameters = json!({"requestIdentity":"a".repeat(64),"estimatedInputTokens":3,"maxOutputTokens":4});
        let first = reserve(&mut budget, parameters.clone());
        let before = budget.state.clone();
        assert_eq!(budget.apply("budget.reserve", json!({"parameters":parameters,"now":1010})).unwrap_err(), "task provider request already dispatched");
        assert_eq!(budget.state, before);
        budget.apply("budget.settle", json!({"id":first,"usage":{"total_tokens":0}})).unwrap();
        let second = reserve(&mut budget, parameters.clone());
        assert_ne!(first, second, "known zero usage retry must receive a new metered reservation");
        assert_eq!(budget.state["requests"], 2);
        budget.apply("budget.settle", json!({"id":second,"usage":{"total_tokens":5}})).unwrap();
        let third = reserve(&mut budget, parameters.clone());
        assert_ne!(second, third, "known success must never provide free dispatch on its old charge");
        assert_eq!(budget.state["requests"], 3);
        budget.apply("budget.settle", json!({"id":third,"usage":null,"outcome":"unknown"})).unwrap();
        let persisted = budget.state.clone();
        let mut recovered = TaskBudget { state: persisted.clone() };
        assert_eq!(recovered.apply("budget.reserve", json!({"parameters":parameters,"now":1020})).unwrap_err(), "task provider usage is unknown");
        assert_eq!(recovered.state, persisted);
        let other = reserve(&mut recovered, json!({"requestIdentity":"b".repeat(64),"estimatedInputTokens":3,"maxOutputTokens":4}));
        assert_ne!(third, other);
        assert_eq!(recovered.state["requests"], 4);
        let unsent_parameters = json!({"requestIdentity":"c".repeat(64),"estimatedInputTokens":3});
        let unsent = reserve(&mut recovered, unsent_parameters.clone());
        recovered.apply("budget.settle", json!({"id":unsent,"usage":null,"outcome":"not-sent"})).unwrap();
        let retried = reserve(&mut recovered, unsent_parameters);
        assert_ne!(unsent, retried);
        assert_eq!(recovered.state["requests"], 5);
        assert_eq!(recovered.state["settlements"][unsent.as_str().unwrap()]["usageState"], "not-sent");
    }

    #[test]
    fn finite_token_authority_blocks_every_unknown_usage_reservation_path() {
        for options in [
            json!({"maxRequests":8,"maxTokens":100,"startedAt":1000}),
            json!({"maxRequests":null,"maxTokens":100,"startedAt":1000}),
        ] {
            let mut budget = TaskBudget::new(json!({"options":options,"now":1000})).unwrap();
            let known = reserve(
                &mut budget,
                json!({"reservationId":"known","estimatedInputTokens":1,"maxOutputTokens":1}),
            );
            budget
                .apply(
                    "budget.settle",
                    json!({"id":known,"usage":{"total_tokens":2}}),
                )
                .unwrap();
            let unknown = reserve(
                &mut budget,
                json!({"reservationId":"unknown","estimatedInputTokens":3,"maxOutputTokens":4}),
            );
            let sent = reserve(
                &mut budget,
                json!({"reservationId":"sent","estimatedInputTokens":3,"maxOutputTokens":2}),
            );
            let unsent = reserve(
                &mut budget,
                json!({"reservationId":"unsent","estimatedInputTokens":3,"maxOutputTokens":3}),
            );
            let unknown_receipt = budget
                .apply(
                    "budget.settle",
                    json!({"id":unknown,"usage":null,"outcome":"unknown"}),
                )
                .unwrap();
            let before = budget.inspect();
            for parameters in [
                json!({"estimatedInputTokens":1,"maxOutputTokens":1}),
                json!({"estimatedInputTokens":100,"maxOutputTokens":1,"deferIfBusy":true}),
                json!({"reservationId":"new","estimatedInputTokens":1,"maxOutputTokens":1}),
                json!({"reservationId":"known","estimatedInputTokens":1,"maxOutputTokens":1}),
                json!({"reservationId":"unknown","estimatedInputTokens":3,"maxOutputTokens":4}),
                json!({"reservationId":"sent","estimatedInputTokens":3,"maxOutputTokens":2}),
            ] {
                assert_eq!(
                    budget.apply(
                        "budget.reserve",
                        json!({"parameters":parameters,"now":1020})
                    ),
                    Err("task provider usage is unknown".into())
                );
                assert_eq!(
                    budget.inspect(),
                    before,
                    "a denial changes no accounting fact"
                );
            }
            budget
                .apply(
                    "budget.settle",
                    json!({"id":sent,"usage":{"total_tokens":3}}),
                )
                .unwrap();
            budget
                .apply(
                    "budget.settle",
                    json!({"id":unsent,"usage":null,"outcome":"not-sent"}),
                )
                .unwrap();
            let final_state = budget.inspect();
            assert_eq!(final_state["requests"], 3);
            assert_eq!(final_state["tokens"].as_f64(), Some(5.0));
            assert_eq!(final_state["unknownTokens"].as_f64(), Some(7.0));
            assert_eq!(final_state["reservedTokens"].as_f64(), Some(0.0));
            assert_eq!(
                budget
                    .apply(
                        "budget.settle",
                        json!({"id":unknown,"usage":{"total_tokens":99}})
                    )
                    .unwrap(),
                unknown_receipt,
                "a replay is a receipt read, never a replacement for unknown usage"
            );
            assert_eq!(budget.inspect(), final_state);
        }
    }

    #[test]
    fn metering_only_unknown_cost_allows_new_charges_but_never_unknown_replay() {
        for requests in [json!(null), json!(2)] {
            let mut budget = TaskBudget::new(json!({"options":{"maxRequests":requests,"maxTokens":null,"maxDurationMs":100,"startedAt":1000},"now":1000})).unwrap();
            let original = json!({"reservationId":"unknown","estimatedInputTokens":3,"maxOutputTokens":4});
            let id = reserve(&mut budget, original.clone());
            let receipt = budget.apply("budget.settle", json!({"id":id,"usage":null,"outcome":"unknown"})).unwrap();
            let before = budget.inspect();
            assert_eq!(budget.apply("budget.reserve", json!({"parameters":original,"now":1020})), Err("task provider usage is unknown".into()));
            assert_eq!(budget.inspect(), before);
            let next = json!({"reservationId":"new-charge","estimatedInputTokens":1,"maxOutputTokens":2});
            let id = reserve(&mut budget, next.clone());
            budget.apply("budget.settle", json!({"id":id,"usage":{"total_tokens":2}})).unwrap();
            let settled = budget.inspect();
            assert_eq!(reserve(&mut budget, next), id);
            assert_eq!(budget.inspect(), settled, "known replay must not charge again");
            assert_eq!(settled["requests"], 2);
            assert_eq!(settled["tokens"].as_f64(), Some(2.0));
            assert_eq!(settled["unknownTokens"].as_f64(), Some(7.0));
            assert_eq!(budget.apply("budget.settle", json!({"id":"unknown","usage":{"total_tokens":99}})).unwrap(), receipt);
            if !requests.is_null() {
                assert_eq!(budget.apply("budget.reserve", json!({"parameters":{},"now":1020})), Err("task request budget exhausted".into()));
                assert_eq!(budget.inspect(), settled);
            }
            assert_eq!(budget.apply("budget.reserve", json!({"parameters":{},"now":1100})), Err("task duration budget exhausted".into()));
            assert_eq!(budget.inspect(), settled);
        }
    }

    #[test]
    fn known_usage_preserves_trusted_id_replay_and_new_reservations() {
        for options in [
            json!({"maxRequests":8,"maxTokens":100,"startedAt":1000}),
            json!({"maxRequests":null,"maxTokens":null,"startedAt":1000}),
        ] {
            let mut budget = TaskBudget::new(json!({"options":options,"now":1000})).unwrap();
            let parameters =
                json!({"reservationId":"known","estimatedInputTokens":1,"maxOutputTokens":3});
            let id = reserve(&mut budget, parameters.clone());
            budget
                .apply("budget.settle", json!({"id":id,"usage":{"total_tokens":2}}))
                .unwrap();
            let before = budget.inspect();
            assert_eq!(reserve(&mut budget, parameters), id);
            assert_eq!(budget.inspect(), before);
            assert!(reserve(&mut budget, json!({"estimatedInputTokens":1})).is_string());
            assert_eq!(budget.inspect()["requests"], 2);
        }
    }

    #[test]
    fn persistent_terminal_receipt_retains_unknown_cost_after_other_requests_settle() {
        for limits in [
            json!({"maxRequests":8,"maxTokens":100}),
            json!({"maxRequests":null,"maxTokens":null}),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("budget.sqlite3");
            let mut budget = PersistentBudget::new(
                &path,
                "unknown-run",
                json!({"options":{"id":"unknown-budget","maxRequests":limits["maxRequests"],"maxTokens":limits["maxTokens"],"startedAt":1000},"now":1000}),
            ).unwrap();
            let unknown = budget
                .apply(
                    "budget.reserve",
                    json!({"parameters":{"estimatedInputTokens":3,"maxOutputTokens":4},"now":1010}),
                )
                .unwrap();
            let sent = budget
                .apply(
                    "budget.reserve",
                    json!({"parameters":{"estimatedInputTokens":3,"maxOutputTokens":2},"now":1010}),
                )
                .unwrap();
            let abandoned = budget
                .apply(
                    "budget.reserve",
                    json!({"parameters":{"estimatedInputTokens":2,"maxOutputTokens":2},"now":1010}),
                )
                .unwrap();
            budget
                .apply(
                    "budget.settle",
                    json!({"id":unknown,"usage":null,"outcome":"unknown"}),
                )
                .unwrap();
            if limits["maxTokens"].is_null() {
                let next = budget.apply("budget.reserve", json!({"parameters":{"reservationId":"next","estimatedInputTokens":1,"maxOutputTokens":1},"now":1020})).unwrap();
                budget.apply("budget.settle", json!({"id":next,"usage":{"total_tokens":2}})).unwrap();
            } else {
                assert_eq!(budget.apply("budget.reserve", json!({"parameters":{},"now":1020})), Err("task provider usage is unknown".into()));
            }
            budget
                .apply(
                    "budget.settle",
                    json!({"id":sent,"usage":{"total_tokens":3}}),
                )
                .unwrap();
            // Finalization accounts the remaining dispatched request; it cannot
            // erase the already unknown charge or allocate another request.
            assert!(abandoned.is_string());
            budget.complete("completed").unwrap();
            let receipt = PersistentBudget::receipt(&path, "unknown-run").unwrap();
            assert_eq!(receipt["status"], "completed");
            assert_eq!(receipt["usage"]["requests"], if limits["maxTokens"].is_null() { 4 } else { 3 });
            assert_eq!(receipt["usage"]["tokens"].as_f64(), Some(if limits["maxTokens"].is_null() { 5.0 } else { 3.0 }));
            assert_eq!(receipt["usage"]["unknownTokens"].as_f64(), Some(11.0));
            assert_eq!(receipt["usage"]["overrun"], false);
            assert_eq!(
                PersistentBudget::receipt(&path, "unknown-run").unwrap(),
                receipt
            );
        }
    }
}
