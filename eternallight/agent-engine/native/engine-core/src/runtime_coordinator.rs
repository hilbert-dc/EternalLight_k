use crate::{Result, SessionRuntime};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

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
    turn_error: Option<String>,
    binding_error: Option<String>,
    revoke: Value,
    revokers: Vec<u64>,
    cancelled: bool,
    cancel_done: bool,
    app_revoking: bool,
    app_revoked: bool,
    revoke_result: Value,
    revoke_error: Option<String>,
    closing: bool,
    owned: bool,
    interrupt_started: bool,
    interrupt_done: bool,
    closers_started: bool,
    closes: Vec<u64>,
    close_error: Option<String>,
    cleanup_started: bool,
    cleanup: Vec<String>,
    cleanup_error: Option<String>,
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

    fn command(&mut self, id: u64, method: &str, input: &Value) -> Result<()> {
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
                    if error.starts_with("TASK_CANCELLED:")
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
                    .apply("current", &json!({"generation":self.generation}))?;
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
    fn reply(&mut self, id: u64, result: Value, error: Option<String>) {
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
        let Some(p) = input["id"].as_u64().and_then(|id| self.pending.remove(&id)) else {
            return Ok(());
        };
        let error = input["error"].as_str().map(str::to_owned);
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
                        Err(_) => {
                            self.binding_error =
                                Some("TASK_CANCELLED:Codex runtime execution was revoked".into())
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
                let result = (|| -> Result<()> {
                    if let Some(error) = error {
                        return Err(error);
                    }
                    self.state
                        .apply("current", &json!({"generation":self.generation}))?;
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
                    self.state.apply(
                        "renew",
                        &json!({"generation":self.generation,"lease":lease}),
                    )?;
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
            let error = self.cleanup_error.clone().or(self.turn_error.take());
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
