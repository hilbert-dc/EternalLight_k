use crate::{Result, RpcPending};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "bridge_supervisor_events.rs"]
mod events;
#[derive(PartialEq)]
enum Phase {
    Stopped,
    Credentials,
    Starting,
    Running,
    Retiring,
    Closing,
}
enum Request {
    Start,
    Command(u64, String),
    Renew(u64, Value),
    Heartbeat,
    Close,
}
enum Callback {
    Credentials,
    Host(Value),
    Recovery(Value),
    RetryRecovery(u64, Value),
    CompleteBudget(u64, Value, Value),
    CloseTransport,
}

/// Supervises one isolated bridge generation, including heartbeat fencing and
/// fail-closed budget recovery. Host objects are referenced only by budget id.
pub struct BridgeSupervisor {
    options: Value,
    phase: Phase,
    generation: u64,
    now: u64,
    rpc: RpcPending,
    requests: BTreeMap<u64, Request>,
    callbacks: BTreeMap<u64, Callback>,
    next_callback: u64,
    callers: BTreeSet<u64>,
    starts: Vec<u64>,
    closes: Vec<u64>,
    effects: Vec<Value>,
    address: Value,
    pid: Value,
    budget: Value,
    lease: Value,
    revoked: bool,
    invalidated: bool,
    recovering: bool,
    recovery_error: Value,
    recovery_waiters: Vec<u64>,
    next_heartbeat: u64,
    heartbeat_pending: bool,
    retirement_error: Option<String>,
    retired_completions: Vec<(u64, Value)>,
}
impl BridgeSupervisor {
    pub fn new(options: Value) -> Result<Self> {
        for key in [
            "heartbeatIntervalMs",
            "heartbeatTimeoutMs",
            "lifecycleTimeoutMs",
        ] {
            if options[key]
                .as_u64()
                .is_none_or(|n| n == 0 && key != "heartbeatIntervalMs" || n > 3_600_000)
            {
                return Err(format!("invalid bridge {key}"));
            }
        }
        Ok(Self {
            options,
            phase: Phase::Stopped,
            generation: 0,
            now: 0,
            rpc: RpcPending::default(),
            requests: BTreeMap::new(),
            callbacks: BTreeMap::new(),
            next_callback: 0,
            callers: BTreeSet::new(),
            starts: Vec::new(),
            closes: Vec::new(),
            effects: Vec::new(),
            address: Value::Null,
            pid: Value::Null,
            budget: Value::Null,
            lease: Value::Null,
            revoked: false,
            invalidated: false,
            recovering: false,
            recovery_error: Value::Null,
            recovery_waiters: Vec::new(),
            next_heartbeat: 0,
            heartbeat_pending: false,
            retirement_error: None,
            retired_completions: Vec::new(),
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        self.now = self.now.max(input["now"].as_u64().unwrap_or(self.now));
        match method {
            "inspect" => {
                return Ok(
                    json!({"generation":self.generation,"processId":self.pid,"baseUrl":if self.pid.is_null(){Value::Null}else{self.address["baseUrl"].clone()},"token":self.address["token"],"budgetRecoveryError":self.recovery_error,"acceptingStart":self.closes.is_empty()&&!matches!(self.phase,Phase::Closing|Phase::Retiring)}),
                );
            }
            "command" => {
                let id = input["id"].as_u64().ok_or("invalid bridge command id")?;
                if self.callers.len() >= 1024 || !self.callers.insert(id) {
                    return Err("bridge command capacity or identity invalid".into());
                }
                if let Err(error) =
                    self.command(id, input["method"].as_str().unwrap_or(""), &input["params"])
                {
                    self.reject(id, &error);
                }
            }
            "callback" => self.callback(input)?,
            "spawned" if input["generation"] == self.generation => self.pid = input["pid"].clone(),
            "spawned" => {}
            "transport" if input["generation"] == self.generation => {
                self.transport(&input["event"])
            }
            "transport" => {}
            "tick" => self.tick()?,
            _ => return Err("unknown bridge supervisor operation".into()),
        }
        Ok(std::mem::take(&mut self.effects).into())
    }
    fn command(&mut self, id: u64, method: &str, params: &Value) -> Result<()> {
        match method {
            "start" => {
                if self.invalidated {
                    return Err("bridge task budget outcome is unknown; terminate the task before restarting".into());
                }
                if !self.closes.is_empty() || matches!(self.phase, Phase::Closing | Phase::Retiring)
                {
                    return Err("bridge process is closing".into());
                }
                if self.phase == Phase::Running {
                    self.resolve(id, self.address.clone());
                    return Ok(());
                }
                self.starts.push(id);
                if self.phase == Phase::Stopped {
                    self.phase = Phase::Credentials;
                    let references: BTreeSet<_> = self.options["connections"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| c["credentialRef"].as_str())
                        .collect();
                    self.effect("credentials", json!(references), Callback::Credentials)?;
                }
            }
            "close" => {
                self.closes.push(id);
                match self.phase {
                    Phase::Stopped => self.closed(),
                    Phase::Running => {
                        self.phase = Phase::Closing;
                        self.request("close", json!({}), Request::Close)?;
                    }
                    Phase::Credentials | Phase::Starting | Phase::Retiring | Phase::Closing => {}
                }
            }
            "cancel" => {
                self.revoked = true;
                if self.phase == Phase::Stopped {
                    self.resolve(id, 0.into());
                } else {
                    self.running()?;
                    self.request(method, json!({}), Request::Command(id, method.into()))?;
                }
            }
            "budget:start" => {
                self.running()?;
                if !self.budget.is_null() {
                    return Err("a task budget is already active".into());
                }
                self.budget = params.clone();
                self.invalidated = false;
                self.request(method, params.clone(), Request::Command(id, method.into()))?;
            }
            "lease:start" => {
                self.running()?;
                if !self.lease.is_null() {
                    return Err("an execution lease is already active".into());
                }
                self.lease = params.clone();
                self.revoked = false;
                self.request(method, params.clone(), Request::Command(id, method.into()))?;
            }
            "lease:renew" => {
                self.running()?;
                if self.revoked || self.lease.is_null() {
                    return Err("execution lease has been revoked".into());
                }
                crate::renewal::execution(
                    &json!({"currentLease":self.lease,"previousPlan":self.options["sessionPlan"],"sessionPlan":params["sessionPlan"],"lease":params["lease"],"epoch":self.lease["epoch"],"now":crate::renewal::now_ms()?}),
                )?;
                self.request(method, params.clone(), Request::Renew(id, params.clone()))?;
            }
            "lease:end" => {
                if self.lease.is_null() || self.lease["leaseId"] != params["leaseId"] {
                    self.resolve(id, false.into());
                } else {
                    self.running()?;
                    self.request(method, json!({}), Request::Command(id, method.into()))?;
                }
            }
            "budget:end" => {
                if self.budget.is_null() || self.budget["id"] != params["id"] {
                    self.resolve(id, false.into());
                } else if self.invalidated {
                    if self.recovering {
                        self.recovery_waiters.push(id);
                    } else {
                        self.effect(
                            "recoverBudget",
                            json!({"budgetId":self.budget["id"]}),
                            Callback::RetryRecovery(id, self.budget["id"].clone()),
                        )?;
                    }
                } else {
                    self.running()?;
                    self.request(method, json!({}), Request::Command(id, method.into()))?;
                }
            }
            _ => return Err("unknown bridge supervisor command".into()),
        }
        Ok(())
    }
    fn running(&self) -> Result<()> {
        if self.phase != Phase::Running {
            Err("bridge process is not running".into())
        } else {
            Ok(())
        }
    }
    fn request(&mut self, method: &str, params: Value, purpose: Request) -> Result<()> {
        let timeout = if matches!(purpose, Request::Heartbeat) {
            self.options["heartbeatTimeoutMs"].clone()
        } else {
            self.options["lifecycleTimeoutMs"].clone()
        };
        let id=self.rpc.apply("register",&json!({"method":method,"generation":self.generation,"now":self.now,"timeoutMs":timeout}))?.as_u64().ok_or("invalid bridge RPC id")?;
        self.requests.insert(id, purpose);
        self.write(json!({"id":id,"method":method,"params":params}));
        Ok(())
    }
    fn effect(&mut self, method: &str, params: Value, purpose: Callback) -> Result<()> {
        if self.callbacks.len() >= 1024 {
            return Err("bridge callback limit exceeded".into());
        }
        self.next_callback = self
            .next_callback
            .checked_add(1)
            .ok_or("bridge callback identity exhausted")?;
        self.callbacks.insert(self.next_callback, purpose);
        self.effects.push(json!({"kind":"callback","id":self.next_callback,"method":method,"params":params,"generation":self.generation}));
        Ok(())
    }
    fn write(&mut self, message: Value) {
        self.effects
            .push(json!({"kind":"write","generation":self.generation,"message":message}));
    }
    fn resolve(&mut self, id: u64, result: Value) {
        if self.callers.remove(&id) {
            self.effects
                .push(json!({"kind":"resolve","id":id,"result":result}));
        }
    }
    fn reject(&mut self, id: u64, error: &str) {
        if self.callers.remove(&id) {
            self.effects
                .push(json!({"kind":"reject","id":id,"error":error}));
        }
    }
}
