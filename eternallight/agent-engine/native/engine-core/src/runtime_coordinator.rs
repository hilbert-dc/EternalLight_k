use crate::{Result, SessionRuntime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeFailure {
    code: Option<String>,
    message: String,
}

type RuntimeResult<T> = std::result::Result<T, RuntimeFailure>;

impl RuntimeFailure {
    fn parse(value: &Value) -> Result<Self> {
        let object = value.as_object().ok_or("invalid runtime failure")?;
        if !object.contains_key("code") || !object.contains_key("message") {
            return Err("invalid runtime failure".into());
        }
        let failure: Self = serde_json::from_value(value.clone())
            .map_err(|_| "invalid runtime failure".to_owned())?;
        if failure.message.chars().count() > 1000
            || failure.code.as_ref().is_some_and(|code| {
                code.is_empty()
                    || code.len() > 80
                    || !code.as_bytes()[0].is_ascii_alphabetic()
                    || !code
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            })
        {
            return Err("invalid runtime failure".into());
        }
        Ok(failure)
    }

    fn cancelled() -> Self {
        Self {
            code: Some("TASK_CANCELLED".into()),
            message: "Codex runtime execution was revoked".into(),
        }
    }
}

impl From<String> for RuntimeFailure {
    fn from(message: String) -> Self {
        Self {
            code: None,
            message: message.chars().take(1000).collect(),
        }
    }
}

impl From<&str> for RuntimeFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

#[derive(Clone, Copy)]
enum Effect {
    Acquire,
    Turn,
    Cancel,
    Revoke,
    Interrupt,
    Cleanup,
    Close,
    Renew,
}
struct Pending {
    effect: Effect,
    step: String,
}

/// Orders embedding-owned runtime IO, including late acquisitions and cleanup.
/// Callback identities are consumed exactly once; host handles carry no authority.
#[derive(Default)]
pub struct RuntimeCoordinator {
    state: SessionRuntime,
    generation: Value,
    lease: Value,
    plan: Value,
    bind: Option<u64>,
    turn: Option<u64>,
    turn_result: Value,
    turn_error: Option<RuntimeFailure>,
    binding_error: Option<RuntimeFailure>,
    revoke: Value,
    revokers: Vec<u64>,
    cancelled: bool,
    cancel_done: bool,
    app_revoking: bool,
    app_revoked: bool,
    revoke_result: Value,
    revoke_error: Option<RuntimeFailure>,
    closing: bool,
    owned: bool,
    interrupt_started: bool,
    interrupt_done: bool,
    closers_started: bool,
    closes: Vec<u64>,
    close_error: Option<RuntimeFailure>,
    cleanup_started: bool,
    cleanup: Vec<String>,
    cleanup_error: Option<RuntimeFailure>,
    pending: BTreeMap<u64, Pending>,
    callers: BTreeSet<u64>,
    next: u64,
    effects: Vec<Value>,
    renewal: Option<(u64, Value, Value)>,
}

impl RuntimeCoordinator {
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "snapshot" | "open" => return self.state.apply(method, input),
            "steer" => {
                if ["runId", "leaseId", "epoch"].iter().any(|key| input[key].is_null() || input[key] != self.lease[key]) {
                    return Err("steer execution lease mismatch".into());
                }
                return self.state.apply("steer", &json!({"generation": self.generation}));
            }
            "command" => {
                let id = input["id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .ok_or("invalid runtime command id")?;
                if self.callers.len() >= 1024 || !self.callers.insert(id) {
                    return Err("runtime caller limit or duplicate identity".into());
                }
                if let Err(error) =
                    self.command(id, input["method"].as_str().unwrap_or(""), &input["params"])
                {
                    self.reply(id, Value::Null, Some(error));
                }
            }
            "callback" => self.callback(input)?,
            _ => return Err("unknown runtime coordinator operation".into()),
        }
        self.advance()?;
        Ok(std::mem::take(&mut self.effects).into())
    }

    fn command(&mut self, id: u64, method: &str, input: &Value) -> RuntimeResult<()> {
        match method {
            "bind" => {
                let generation = self.state.apply("begin", input)?;
                self.generation = generation;
                self.lease = input["lease"].clone();
                self.plan = input["sessionPlan"].clone();
                self.bind = Some(id);
                self.binding_error = None;
                self.turn_error = None;
                self.cleanup_error = None;
                self.cleanup_started = false;
                self.cancelled = false;
                self.cancel_done = false;
                self.app_revoking = false;
                self.app_revoked = false;
                self.revoke = Value::Null;
                self.revoke_result = Value::Null;
                self.revoke_error = None;
                self.acquire()?;
            }
            "turn" => {
                if let Err(error) = self
                    .state
                    .apply("dispatch", &json!({"generation":self.generation}))
                {
                    let error = self.execution_failure(error);
                    if error.code.as_deref() == Some("TASK_CANCELLED")
                        && !self.generation.is_null()
                        && self.turn.is_none()
                    {
                        self.turn = Some(id);
                        self.turn_error = Some(error);
                        return Ok(());
                    }
                    return Err(error);
                }
                self.turn = Some(id);
                self.turn_result = Value::Null;
                self.turn_error = None;
                self.effect(
                    Effect::Turn,
                    "sendTurn",
                    json!({"input":input["input"],"commandId":id}),
                );
            }
            "revoke" => {
                self.state.apply("revoke", &json!({"request":input}))?;
                self.revokers.push(id);
                if self.revoke.is_null() {
                    self.revoke = input.clone();
                    self.cancelled = true;
                    self.effect(Effect::Cancel, "cancel", Value::Null);
                }
            }
            "renew" => {
                self.state
                    .apply("current", &json!({"generation":self.generation}))
                    .map_err(|error| self.execution_failure(error))?;
                if self.bind.is_some() || self.cleanup_started || self.renewal.is_some() {
                    return Err(
                        "runtime renewal requires a bound live execution without pending renewal"
                            .into(),
                    );
                }
                crate::renewal::execution(
                    &json!({"currentLease":self.lease,"previousPlan":self.plan,"sessionPlan":input["sessionPlan"],"lease":input["lease"],"epoch":self.lease["epoch"],"now":crate::renewal::now_ms()?}),
                )?;
                self.renewal = Some((id, input["sessionPlan"].clone(), input["lease"].clone()));
                self.effect(Effect::Renew, "leaseRenewed", json!({"lease":input["lease"],"sessionPlan":input["sessionPlan"],"commandId":id}));
            }
            "close" => {
                self.closes.push(id);
                if !self.closing {
                    self.closing = true;
                    self.owned = input["ownership"] == "owned";
                    self.state.apply("close", &Value::Null)?;
                    if !self.generation.is_null() && !self.cancelled {
                        self.cancelled = true;
                        self.effect(Effect::Cancel, "cancel", Value::Null);
                    }
                }
            }
            _ => return Err("unknown runtime command".into()),
        }
        Ok(())
    }

    fn effect(&mut self, effect: Effect, step: &str, params: Value) {
        self.next += 1;
        self.pending.insert(
            self.next,
            Pending {
                effect,
                step: step.into(),
            },
        );
        self.effects
            .push(json!({"kind":"callback","id":self.next,"method":step,"params":params}));
    }
    fn execution_failure(&mut self, error: String) -> RuntimeFailure {
        if !self.generation.is_null()
            && self
                .state
                .apply("execution", &json!({"generation":self.generation}))
                .is_ok_and(|state| state["revoked"] == true)
        {
            RuntimeFailure::cancelled()
        } else {
            error.into()
        }
    }

    fn reply(&mut self, id: u64, result: Value, error: Option<RuntimeFailure>) {
        if self.callers.remove(&id) {
            self.effects
                .push(json!({"kind":"reply","id":id,"result":result,"error":error}));
        }
    }
    fn acquire(&mut self) -> Result<()> {
        let step = self
            .state
            .apply("next", &json!({"generation":self.generation}))?;
        if let Some(step) = step.as_str() {
            self.effect(
                Effect::Acquire,
                step,
                json!({"lease":self.lease,"sessionPlan":self.plan}),
            );
        } else if let Some(id) = self.bind.take() {
            self.reply(id, Value::Null, None);
        }
        Ok(())
    }
    fn callback(&mut self, input: &Value) -> Result<()> {
        let Some(id) = input["id"]
            .as_u64()
            .filter(|id| self.pending.contains_key(id))
        else {
            return Ok(());
        };
        let error = input
            .get("error")
            .filter(|value| !value.is_null())
            .map(RuntimeFailure::parse)
            .transpose()?;
        let p = self.pending.remove(&id).ok_or("missing runtime callback")?;
        match p.effect {
            Effect::Acquire => {
                if let Some(error) = error {
                    self.binding_error = Some(error);
                } else {
                    self.state
                        .apply("ack", &json!({"generation":self.generation,"step":p.step}))?;
                    match self
                        .state
                        .apply("current", &json!({"generation":self.generation}))
                    {
                        Ok(_) => self.acquire()?,
                        Err(error) => {
                            self.binding_error = Some(self.execution_failure(error));
                        }
                    }
                }
            }
            Effect::Turn => {
                self.turn_result = input["result"].clone();
                self.turn_error = error;
            }
            Effect::Cancel => {
                self.cancel_done = true;
                self.revoke_error = error;
            }
            Effect::Revoke => {
                self.app_revoked = true;
                self.revoke_result = input["result"].clone();
                if error.is_some() {
                    self.revoke_error = error;
                }
            }
            Effect::Interrupt => {
                self.interrupt_done = true;
            }
            Effect::Cleanup => {
                if self.cleanup_error.is_none() {
                    self.cleanup_error = error;
                }
            }
            Effect::Close => {
                if self.close_error.is_none() {
                    self.close_error = error;
                }
            }
            Effect::Renew => {
                let (id, plan, lease) = self.renewal.clone().ok_or("missing runtime renewal")?;
                let result = (|| -> RuntimeResult<()> {
                    if let Some(error) = error {
                        return Err(error);
                    }
                    self.state
                        .apply("current", &json!({"generation":self.generation}))
                        .map_err(|error| self.execution_failure(error))?;
                    if input["result"] != lease {
                        return Err("bridge returned an invalid renewed lease".into());
                    }
                    crate::renewal::execution(
                        &json!({"currentLease":self.lease,"previousPlan":self.plan,"sessionPlan":plan,"lease":lease,"epoch":self.lease["epoch"],"now":crate::renewal::now_ms()?}),
                    )?;
                    if p.step == "leaseRenewed" {
                        self.effect(
                            Effect::Renew,
                            "appRenewed",
                            json!({"lease":lease,"sessionPlan":plan,"commandId":id}),
                        );
                        return Ok(());
                    }
                    self.state
                        .apply(
                            "renew",
                            &json!({"generation":self.generation,"lease":lease}),
                        )
                        .map_err(|error| self.execution_failure(error))?;
                    self.plan = plan;
                    self.lease = lease.clone();
                    Ok(())
                })();
                if result.is_err() || p.step == "appRenewed" {
                    self.renewal = None;
                    self.reply(id, lease, result.err());
                }
            }
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<()> {
        let acquiring = self
            .pending
            .values()
            .any(|p| matches!(p.effect, Effect::Acquire));
        if !self.generation.is_null() {
            let state = self
                .state
                .apply("execution", &json!({"generation":self.generation}))?;
            if !self.revoke.is_null() && state["appBound"] == true && !self.app_revoking {
                self.app_revoking = true;
                self.effect(Effect::Revoke, "revokeApp", self.revoke.clone());
            }
            let revoke_done = self.revoke.is_null()
                || (self.cancel_done
                    && !acquiring
                    && (state["appBound"] != true || self.app_revoked));
            if revoke_done && !self.revokers.is_empty() {
                let result = if self.app_revoked {
                    self.revoke_result.clone()
                } else {
                    json!({"revokedEpoch":self.revoke["revokedEpoch"]})
                };
                for id in std::mem::take(&mut self.revokers) {
                    self.reply(id, result.clone(), self.revoke_error.clone());
                }
            }
            if self.closing && self.cancel_done && !acquiring && !self.interrupt_started {
                self.interrupt_started = true;
                self.effect(Effect::Interrupt, "interrupt", Value::Null);
            }
            let turn_done = self.turn.is_some()
                && !self
                    .pending
                    .values()
                    .any(|p| matches!(p.effect, Effect::Turn));
            let settle =
                self.binding_error.is_some() || turn_done || (self.closing && self.interrupt_done);
            if settle
                && !acquiring
                && self.renewal.is_none()
                && revoke_done
                && (!self.closing || self.interrupt_done)
            {
                if !self.cleanup_started {
                    self.cleanup_started = true;
                    self.cleanup = self
                        .state
                        .apply("cleanup", &json!({"generation":self.generation}))?
                        .as_array()
                        .ok_or("invalid runtime cleanup")?
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                }
                if !self
                    .pending
                    .values()
                    .any(|p| matches!(p.effect, Effect::Cleanup))
                {
                    if !self.cleanup.is_empty() {
                        let step = self.cleanup.remove(0);
                        let mut params = self.lease.clone();
                        if matches!(step.as_str(), "budget" | "budgetRecord") {
                            params["budgetStatus"] = json!(if self.cancelled || self.closing {
                                "cancelled"
                            } else if self.binding_error.is_some()
                                || self.turn_error.is_some()
                                || self.cleanup_error.is_some()
                                || self.turn.is_none()
                            {
                                "failed"
                            } else {
                                "completed"
                            });
                        }
                        self.effect(Effect::Cleanup, &format!("end:{step}"), params);
                    } else {
                        self.state.apply("settled",&json!({"generation":self.generation,"failed":self.cleanup_error.is_some()}))?;
                        self.generation = Value::Null;
                        if let Some(id) = self.bind.take() {
                            let error = self.binding_error.take();
                            self.reply(id, Value::Null, error);
                        }
                        self.effects.push(json!({"kind":"releaseBudget"}));
                    }
                }
            }
        }
        if self.generation.is_null()
            && !self
                .pending
                .values()
                .any(|p| matches!(p.effect, Effect::Turn))
            && let Some(id) = self.turn.take()
        {
            let error = self.turn_error.take().or(self.cleanup_error.clone());
            self.reply(id, self.turn_result.clone(), error);
        }
        if self.closing && self.generation.is_null() {
            if !self.closers_started {
                self.closers_started = true;
                self.effect(Effect::Close, "closeApp", Value::Null);
                if self.owned {
                    self.effect(Effect::Close, "closeBridge", Value::Null);
                }
            } else if !self
                .pending
                .values()
                .any(|p| matches!(p.effect, Effect::Close))
            {
                for id in std::mem::take(&mut self.closes) {
                    self.reply(id, Value::Null, self.close_error.clone());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(runtime: &mut RuntimeCoordinator, id: u64, method: &str) -> Value {
        runtime
            .apply(
                "command",
                &json!({"id":id,"method":method,"params":{
                    "lease":{"runId":"run","leaseId":"lease","budgetId":"budget","epoch":0},
                    "sessionPlan":{}
                }}),
            )
            .unwrap()
    }

    fn bound_runtime() -> RuntimeCoordinator {
        let mut runtime = RuntimeCoordinator::default();
        let mut effects = command(&mut runtime, 1, "bind");
        for step in [
            "budget",
            "bridge",
            "budgetStarted",
            "leaseStarted",
            "appBound",
        ] {
            assert_eq!(effects[0]["method"], step);
            effects = runtime
                .apply("callback", &json!({"id":effects[0]["id"],"result":null}))
                .unwrap();
        }
        assert_eq!(
            effects,
            json!([{"kind":"reply","id":1,"result":null,"error":null}])
        );
        runtime
    }

    #[test]
    fn callback_failure_wire_is_strict_and_invalid_input_preserves_pending_identity() {
        let invalid = [
            json!("old string error"),
            json!([]),
            json!(true),
            json!({"message":"missing code"}),
            json!({"code":null}),
            json!({"code":null,"message":"extra field","cause":"private"}),
            json!({"code":5,"message":"wrong code type"}),
            json!({"code":null,"message":{}}),
            json!({"code":"","message":"empty"}),
            json!({"code":"1BAD","message":"first digit"}),
            json!({"code":"非法","message":"non ascii"}),
            json!({"code":"BAD CODE","message":"space"}),
            json!({"code":"A".repeat(81),"message":"oversized code"}),
            json!({"code":null,"message":"文".repeat(1001)}),
        ];
        for error in invalid {
            let mut runtime = RuntimeCoordinator::default();
            let effects = command(&mut runtime, 1, "bind");
            let id = effects[0]["id"].clone();
            assert!(
                runtime
                    .apply("callback", &json!({"id":id,"error":error}))
                    .is_err()
            );
            assert_eq!(runtime.pending.len(), 1);
            let effects = runtime
                .apply(
                    "callback",
                    &json!({"id":id,"error":{
                        "code":null,"message":"plain failure"
                    }}),
                )
                .unwrap();
            let reply = effects
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["kind"] == "reply")
                .unwrap();
            assert_eq!(
                reply["error"],
                json!({"code":null,"message":"plain failure"})
            );
            assert!(runtime.pending.is_empty());
            assert!(runtime.callers.is_empty());
        }
        assert!(
            RuntimeFailure::parse(&json!({"code":"A".repeat(80),"message":"文".repeat(1000)}))
                .is_ok()
        );
    }

    #[test]
    fn original_failure_wins_cleanup_and_cleanup_failure_still_blocks_runtime_reuse() {
        for original in [true, false] {
            let mut runtime = bound_runtime();
            let turn = command(&mut runtime, 2, "turn");
            let mut input = json!({"id":turn[0]["id"],"result":{"turn":"completed"}});
            if original {
                input["error"] = json!({"code":"tool_argument_correction_limit","message":"tool correction limit"});
            }
            let lease = runtime.apply("callback", &input).unwrap();
            assert_eq!(lease[0]["method"], "end:lease");
            let budget = runtime
                .apply(
                    "callback",
                    &json!({"id":lease[0]["id"],"error":{
                        "code":"LEASE_DRAIN_FAILED","message":"lease drain failed"
                    }}),
                )
                .unwrap();
            assert_eq!(budget[0]["method"], "end:budget");
            assert_eq!(budget[0]["params"]["budgetStatus"], "failed");
            let app = runtime
                .apply("callback", &json!({"id":budget[0]["id"],"result":true}))
                .unwrap();
            assert_eq!(app[0]["method"], "end:app");
            let effects = runtime
                .apply("callback", &json!({"id":app[0]["id"],"result":null}))
                .unwrap();
            let expected = if original {
                json!({"code":"tool_argument_correction_limit","message":"tool correction limit"})
            } else {
                json!({"code":"LEASE_DRAIN_FAILED","message":"lease drain failed"})
            };
            assert_eq!(
                effects,
                json!([
                    {"kind":"releaseBudget"},
                    {"kind":"reply","id":2,"result":{"turn":"completed"},"error":expected}
                ])
            );
            assert_eq!(
                runtime.apply("snapshot", &Value::Null).unwrap(),
                json!({"closed":false,"blocked":true})
            );
            assert!(runtime.generation.is_null());
            assert!(runtime.pending.is_empty());
            let blocked = command(&mut runtime, 3, "bind");
            assert_eq!(blocked[0]["error"]["code"], Value::Null);
            assert!(
                blocked[0]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("cleanup is unresolved")
            );
        }
    }

    #[test]
    fn renewal_callback_retains_machine_failure_without_updating_execution() {
        let mut runtime = bound_runtime();
        runtime.callers.insert(2);
        let lease = runtime.lease.clone();
        let plan = runtime.plan.clone();
        runtime.renewal = Some((2, json!({"id":"new-plan"}), lease.clone()));
        runtime.effect(
            Effect::Renew,
            "leaseRenewed",
            json!({"lease":lease,"commandId":2}),
        );
        let id = runtime.effects.pop().unwrap()["id"].clone();
        let failure = json!({"code":"RENEWAL_PROVIDER_FAILED","message":"renewal failed"});
        let effects = runtime
            .apply("callback", &json!({"id":id,"error":failure}))
            .unwrap();
        assert_eq!(
            effects,
            json!([{"kind":"reply","id":2,"result":lease,"error":failure}])
        );
        assert_eq!(runtime.plan, plan);
        assert_eq!(runtime.lease, lease);
        assert!(runtime.renewal.is_none());
        assert!(runtime.pending.is_empty());
    }
}
