use crate::{AppServerTurn, Result, RpcPending};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "app_client_events.rs"]
mod events;

#[derive(PartialEq)]
enum Phase {
    Dormant,
    Initializing,
    Ready,
    Retiring,
    Closing,
}
enum Rpc {
    Initialize,
    ThreadStart,
    Resume(u64, String),
    TurnStart(u64),
    Name,
    Interrupt(u64),
}
struct Turn {
    id: u64,
    state: AppServerTurn,
    input: Value,
    options: Value,
    deadline: Option<u64>,
    sent: bool,
    created_thread: bool,
}

/// Owns App Server lifecycle and all protocol decisions. The embedding executes
/// emitted transport effects and delivers host callbacks without mutating state.
pub struct AppClient {
    options: Value,
    phase: Phase,
    generation: u64,
    now: u64,
    rpc: RpcPending,
    requests: BTreeMap<u64, Rpc>,
    callers: BTreeSet<u64>,
    sessions: Vec<u64>,
    resume: Option<(u64, String)>,
    resuming: bool,
    thread: Option<String>,
    creating: bool,
    turn: Option<Turn>,
    interrupting: bool,
    interrupts: Vec<(u64, Value)>,
    lease: Value,
    revoked: bool,
    closes: Vec<u64>,
    retirement_error: Option<String>,
    effects: Vec<Value>,
}

impl AppClient {
    pub fn new(options: Value) -> Result<Self> {
        if options["model"].as_str().is_none_or(str::is_empty) {
            return Err("App Server model is required".into());
        }
        for field in ["requestTimeoutMs", "turnTimeoutMs"] {
            if field == "turnTimeoutMs" && options[field].is_null() {
                continue;
            }
            if options[field]
                .as_u64()
                .is_none_or(|v| v == 0 || v > 86_400_000)
            {
                return Err(format!("invalid App Server {field}"));
            }
        }
        Ok(Self {
            options,
            phase: Phase::Dormant,
            generation: 0,
            now: 0,
            rpc: RpcPending::default(),
            requests: BTreeMap::new(),
            callers: BTreeSet::new(),
            sessions: Vec::new(),
            resume: None,
            resuming: false,
            thread: None,
            creating: false,
            turn: None,
            interrupting: false,
            interrupts: Vec::new(),
            lease: Value::Null,
            revoked: false,
            closes: Vec::new(),
            retirement_error: None,
            effects: Vec::new(),
        })
    }

    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        self.now = input["now"].as_u64().unwrap_or(self.now).max(self.now);
        match method {
            "inspect" => {
                return Ok(
                    json!({"generation":self.generation,"threadId":self.thread,"turnId":self.turn.as_ref().map(|t|t.state.snapshot()["turnId"].clone()),"activeLease":self.lease,"executionRevoked":self.revoked}),
                );
            }
            "command" => {
                let id = input["id"]
                    .as_u64()
                    .ok_or("invalid App Server command id")?;
                if self.callers.len() >= 1024 || !self.callers.insert(id) {
                    return Err("App Server command capacity or identity invalid".into());
                }
                if let Err(error) =
                    self.command(id, input["method"].as_str().unwrap_or(""), &input["params"])
                {
                    self.reject(id, &error, Value::Null);
                }
            }
            "tick" => self.tick()?,
            "transport" if input["generation"] == self.generation => {
                self.transport(&input["event"])
            }
            "retired" if input["generation"] == self.generation => self.retired(),
            "transport" | "retired" => {}
            "hostResponse"
                if input["generation"] == self.generation && self.phase == Phase::Ready =>
            {
                self.write(input["message"].clone())
            }
            "hostResponse" => {}
            _ => return Err("unknown App Server client operation".into()),
        }
        Ok(std::mem::take(&mut self.effects).into())
    }

    fn command(&mut self, id: u64, method: &str, params: &Value) -> Result<()> {
        if method == "close" {
            self.closes.push(id);
            if self.phase == Phase::Closing {
                return Ok(());
            }
            self.revoked = true;
            self.lease = Value::Null;
            self.fail_callers("Codex App Server client closed");
            if self.phase == Phase::Dormant {
                self.retired();
            } else {
                self.phase = Phase::Closing;
                self.effects.push(json!({"kind":"retire","generation":self.generation,"timeoutMs":params["timeoutMs"].as_u64().unwrap_or(2000).min(60_000)}));
            }
            return Ok(());
        }
        if matches!(self.phase, Phase::Closing | Phase::Retiring) {
            return Err("Codex App Server client is closing".into());
        }
        match method {
            "ensureSession" => {
                self.sessions.push(id);
                self.ensure_initialized()?;
                self.advance()?;
            }
            "resumeSession" => {
                if self.turn.is_some() || self.resuming || self.creating {
                    return Err("cannot resume a thread during an active operation".into());
                }
                let thread = params["threadId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("thread id is required")?;
                self.resume = Some((id, thread.into()));
                self.resuming = true;
                self.ensure_initialized()?;
                self.advance()?;
            }
            "sendTurn" => {
                if self.turn.is_some() || self.resuming {
                    return Err("Codex App Server already has an active operation".into());
                }
                if params["input"].as_array().is_none_or(Vec::is_empty) {
                    return Err("Codex turn input is required".into());
                }
                if self.revoked {
                    self.reject(
                        id,
                        "Codex execution has been revoked",
                        "TASK_CANCELLED".into(),
                    );
                    return Ok(());
                }
                self.turn = Some(Turn {
                    id,
                    state: AppServerTurn::default(),
                    input: params["input"].clone(),
                    options: params["options"].clone(),
                    deadline: self.options["turnTimeoutMs"]
                        .as_u64()
                        .map(|duration| self.now.saturating_add(duration)),
                    sent: false,
                    created_thread: self.thread.is_none(),
                });
                self.ensure_initialized()?;
                self.advance()?;
            }
            "bindExecution" => {
                let lease = &params["lease"];
                if lease["runId"].as_str().is_none_or(str::is_empty)
                    || lease["leaseId"].as_str().is_none_or(str::is_empty)
                    || lease["epoch"].as_u64().is_none()
                {
                    return Err("App Server execution lease is invalid".into());
                }
                if self.turn.is_some() {
                    return Err("cannot replace execution during an active turn".into());
                }
                self.lease = json!({"runId":lease["runId"],"leaseId":lease["leaseId"],"epoch":lease["epoch"]});
                self.revoked = false;
                self.resolve(id, Value::Null);
            }
            "interruptTurn" => {
                if self.turn.is_none() {
                    self.resolve(id, false.into());
                } else {
                    self.turn.as_mut().unwrap().state.cancel();
                    self.interrupts.push((id, true.into()));
                    self.interrupt()?;
                }
            }
            "revokeExecution" => {
                for (field, option) in [
                    ("runId", "runId"),
                    ("leaseId", "leaseId"),
                    ("epoch", "expectedEpoch"),
                ] {
                    if self.lease.is_null()
                        || params
                            .get(option)
                            .is_some_and(|value| value != &self.lease[field])
                    {
                        return Err("App Server execution lease mismatch".into());
                    }
                }
                let epoch = params["revokedEpoch"]
                    .as_u64()
                    .unwrap_or(self.lease["epoch"].as_u64().unwrap_or(0).saturating_add(1));
                if epoch <= self.lease["epoch"].as_u64().unwrap_or(0) {
                    return Err("App Server revoked epoch must advance".into());
                }
                self.revoked = true;
                self.lease["epoch"] = epoch.into();
                if let Some(turn) = &mut self.turn {
                    turn.state.cancel();
                    self.interrupts.push((id, json!({"revokedEpoch":epoch})));
                    self.interrupt()?;
                } else {
                    self.resolve(id, json!({"revokedEpoch":epoch}));
                }
            }
            _ => return Err("unknown App Server command".into()),
        }
        Ok(())
    }

    fn ensure_initialized(&mut self) -> Result<()> {
        if self.phase != Phase::Dormant {
            return Ok(());
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("App Server generation exhausted")?;
        self.phase = Phase::Initializing;
        self.effects
            .push(json!({"kind":"spawn","generation":self.generation}));
        self.request("initialize",json!({"clientInfo":self.options["clientInfo"],"capabilities":{"experimentalApi":true}}),Rpc::Initialize)
    }
    fn advance(&mut self) -> Result<()> {
        if self.phase != Phase::Ready {
            return Ok(());
        }
        if let Some((id, thread)) = self.resume.take() {
            return self.request(
                "thread/resume",
                json!({"threadId":thread}),
                Rpc::Resume(id, thread),
            );
        }
        if self.resuming {
            return Ok(());
        }
        if self.thread.is_none() {
            if !self.creating && (!self.sessions.is_empty() || self.turn.is_some()) {
                self.creating = true;
                self.request("thread/start",json!({"model":self.options["model"],"cwd":self.options["cwd"],"serviceName":"eternallight_agent_engine"}),Rpc::ThreadStart)?;
            }
            return Ok(());
        }
        for id in std::mem::take(&mut self.sessions) {
            self.resolve(id, json!(self.thread));
        }
        let Some(turn) = &mut self.turn else {
            return Ok(());
        };
        if turn.sent {
            return Ok(());
        }
        turn.state.bind_thread(
            self.generation,
            self.thread.clone().ok_or("missing App Server thread")?,
        )?;
        if turn.state.snapshot()["cancellationRequested"] == true || self.revoked {
            let id = turn.id;
            self.turn = None;
            self.reject(
                id,
                "Codex execution has been revoked",
                "TASK_CANCELLED".into(),
            );
            return Ok(());
        }
        turn.sent = true;
        let mut params = json!({"threadId":self.thread,"input":turn.input});
        if turn.options["disableEnvironment"] == true {
            params["environments"] = json!([]);
            params["approvalPolicy"] = "never".into();
        }
        let id = turn.id;
        self.request("turn/start", params, Rpc::TurnStart(id))
    }
    fn request(&mut self, method: &str, params: Value, purpose: Rpc) -> Result<()> {
        let id=self.rpc.apply("register", &json!({"method":method,"generation":self.generation,"now":self.now,"timeoutMs":self.options["requestTimeoutMs"]}))?.as_u64().ok_or("invalid RPC id")?;
        self.requests.insert(id, purpose);
        self.write(json!({"id":id,"method":method,"params":params}));
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
    fn reject(&mut self, id: u64, error: &str, code: Value) {
        if self.callers.remove(&id) {
            self.effects
                .push(json!({"kind":"reject","id":id,"error":error,"code":code}));
        }
    }
    fn interrupt(&mut self) -> Result<()> {
        if self.interrupting {
            return Ok(());
        }
        let snapshot = self
            .turn
            .as_ref()
            .map(|t| t.state.snapshot())
            .unwrap_or(Value::Null);
        if snapshot["turnId"].is_null() {
            for (id, result) in std::mem::take(&mut self.interrupts) {
                self.resolve(id, result);
            }
        } else {
            self.interrupting = true;
            let id = self.turn.as_ref().ok_or("missing interrupted turn")?.id;
            self.request(
                "turn/interrupt",
                json!({"threadId":snapshot["threadId"],"turnId":snapshot["turnId"]}),
                Rpc::Interrupt(id),
            )?;
        }
        Ok(())
    }
}
