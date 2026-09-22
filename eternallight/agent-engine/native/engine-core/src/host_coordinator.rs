use crate::{ExecutionAuthority, HostRun, HostSession, Result, lease};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[path = "host_coordinator_callbacks.rs"]
mod callbacks;
#[path = "host_coordinator_runs.rs"]
mod runs;

struct Session {
    plan: Value,
    state: HostSession,
    start: u64,
    resume: Value,
    recovery: Value,
    runtime: bool,
    visible: bool,
    closes: Vec<u64>,
    closing_runtime: bool,
    failure: Option<String>,
}
struct Run {
    session: String,
    thread: Value,
    state: HostRun,
    lease: Value,
    input: Value,
    options: Value,
    start: Option<u64>,
    cancels: Vec<u64>,
    interrupted: bool,
    completion: Option<Value>,
    completion_error: Option<Value>,
    completion_delivered: bool,
}
#[derive(Clone, Copy)]
enum Purpose {
    Recovery,
    Factory,
    Subscribe,
    Ensure,
    Resume,
    Bind,
    Turn,
    Revoke,
    Interrupt,
    Reconcile,
    Close,
}
#[derive(Clone)]
struct Pending {
    session: String,
    run: Option<String>,
    purpose: Purpose,
}

/// Owns reusable sessions, run admission and cancellation/reconciliation order.
/// Effects reference embedding-owned runtime handles, never mutable snapshots.
pub struct HostCoordinator {
    sessions: BTreeMap<String, Session>,
    runs: BTreeMap<String, Run>,
    authority: ExecutionAuthority,
    pending: BTreeMap<u64, Pending>,
    next: u64,
    callers: BTreeSet<u64>,
    effects: Vec<Value>,
    now: String,
    maximum: u64,
}

impl HostCoordinator {
    pub fn new(maximum: u64) -> Result<Self> {
        if !(1..=100_000).contains(&maximum) {
            return Err("maxRetainedEvents must be positive and bounded".into());
        }
        Ok(Self {
            sessions: BTreeMap::new(),
            runs: BTreeMap::new(),
            authority: ExecutionAuthority::default(),
            pending: BTreeMap::new(),
            next: 0,
            callers: BTreeSet::new(),
            effects: Vec::new(),
            now: String::new(),
            maximum,
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        if let Some(now) = input["now"].as_str() {
            chrono::DateTime::parse_from_rfc3339(now).map_err(|_| "invalid host clock")?;
            self.now = now.into();
        }
        match method {
            "command" => {
                let id = input["id"].as_u64().ok_or("invalid host command id")?;
                if self.callers.len() >= 10_000 || !self.callers.insert(id) {
                    return Err("host command capacity or identity invalid".into());
                }
                if let Err(error) =
                    self.command(id, input["method"].as_str().unwrap_or(""), &input["params"])
                {
                    self.reject(id, &error);
                }
            }
            "callback" => self.callback(input)?,
            "event" => {
                let session = string(&input["sessionId"]);
                if let Some(session) = self.sessions.get_mut(session) {
                    let active =
                        session.state.apply("snapshot", &Value::Null)?["activeRunId"].clone();
                    if let Some(run) = active.as_str() {
                        let run = run.to_owned();
                        self.record(&run, input["event"].clone())?;
                    }
                }
            }
            "runStatus" => {
                let id = string(&input["runId"]);
                let run = self
                    .runs
                    .get_mut(id)
                    .ok_or_else(|| format!("unknown run: {id}"))?;
                let state = run.state.apply("snapshot", &Value::Null)?;
                return Ok(
                    json!({"runId":id,"sessionId":run.session,"threadId":run.thread,"state":state["state"],"sequence":state["sequence"],"leaseId":run.lease["leaseId"],"epoch":state["epoch"]}),
                );
            }
            "sessionStatus" => {
                let id = string(&input["sessionId"]);
                let session = self
                    .sessions
                    .get_mut(id)
                    .filter(|s| s.visible)
                    .ok_or_else(|| format!("unknown session: {id}"))?;
                let state = session.state.apply("snapshot", &Value::Null)?;
                return Ok(
                    json!({"sessionId":id,"threadId":state["threadId"],"state":state["state"],"activeRunId":state["activeRunId"],"planId":session.plan["id"]}),
                );
            }
            "replay" => {
                let run = self
                    .runs
                    .get_mut(string(&input["runId"]))
                    .ok_or("unknown run")?;
                return run.state.apply("replay", &input["afterSequence"]);
            }
            "closeRun" => {
                let id = string(&input["runId"]);
                let run = self.runs.get_mut(id).ok_or("unknown run")?;
                if run.state.apply("snapshot", &Value::Null)?["terminal"] != true {
                    return Err("cannot release a non-terminal run".into());
                }
                self.authority.apply("release", json!({"runId":id}))?;
                self.runs.remove(id);
                self.pending.retain(|_, p| p.run.as_deref() != Some(id));
            }
            _ => return Err("unknown host coordinator operation".into()),
        }
        let released: Vec<_> = self
            .sessions
            .iter()
            .filter(|(id, s)| {
                !s.visible && !s.runtime && !self.pending.values().any(|p| p.session == **id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in released {
            self.sessions.remove(&id);
            self.effects
                .push(json!({"kind":"releaseSession","sessionId":id}));
        }
        Ok(std::mem::take(&mut self.effects).into())
    }
    fn command(&mut self, id: u64, method: &str, input: &Value) -> Result<()> {
        match method {
            "startSession" | "resumeSession" => {
                let session_id = string(&input["sessionId"]);
                if session_id.is_empty() {
                    return Err("session id is required".into());
                }
                if self.sessions.contains_key(session_id) {
                    return Err(format!("session already exists: {session_id}"));
                }
                if self.sessions.len() >= 10_000 {
                    return Err("host session capacity exceeded".into());
                }
                let resume = method == "resumeSession";
                self.sessions.insert(
                    session_id.into(),
                    Session {
                        plan: input["sessionPlan"].clone(),
                        state: HostSession::new(if resume { "recovering" } else { "preparing" })?,
                        start: id,
                        resume: if resume {
                            input["threadId"].clone()
                        } else {
                            Value::Null
                        },
                        recovery: Value::Null,
                        runtime: false,
                        visible: true,
                        closes: Vec::new(),
                        closing_runtime: false,
                        failure: None,
                    },
                );
                if resume {
                    self.effect(session_id,None,Purpose::Recovery,"recoverSession",json!({"sessionId":session_id,"threadId":input["threadId"],"sessionPlan":input["sessionPlan"]}))?;
                } else {
                    self.effect(
                        session_id,
                        None,
                        Purpose::Factory,
                        "runtimeFactory",
                        json!({"sessionPlan":input["sessionPlan"]}),
                    )?;
                }
            }
            "startRun" => self.start_run(id, input)?,
            "cancelRun" => self.cancel(
                id,
                string(&input["runId"]),
                input["reason"].as_str().unwrap_or("cancelled-by-host"),
            )?,
            "closeSession" => {
                let sid = string(&input["sessionId"]);
                let session = self
                    .sessions
                    .get_mut(sid)
                    .ok_or_else(|| format!("unknown session: {sid}"))?;
                session.closes.push(id);
                session.state.apply("closing", &Value::Null)?;
                let active = session.state.apply("snapshot", &Value::Null)?["activeRunId"].clone();
                if let Some(run) = active.as_str() {
                    self.cancel(0, run, "session-closing")?;
                } else {
                    self.close_ready(sid)?;
                }
            }
            _ => return Err("unknown engine host command".into()),
        }
        Ok(())
    }
    fn effect(
        &mut self,
        session: &str,
        run: Option<&str>,
        purpose: Purpose,
        method: &str,
        params: Value,
    ) -> Result<()> {
        if self.pending.len() >= 10_000 {
            return Err("host pending effect capacity exceeded".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("host effect identity exhausted")?;
        self.pending.insert(
            self.next,
            Pending {
                session: session.into(),
                run: run.map(str::to_owned),
                purpose,
            },
        );
        self.effects.push(json!({"kind":"callback","id":self.next,"sessionId":session,"runId":run,"method":method,"params":params}));
        Ok(())
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
    fn record(&mut self, id: &str, event: Value) -> Result<()> {
        let Some(run) = self.runs.get_mut(id) else {
            return Ok(());
        };
        let event = run
            .state
            .apply("record", &json!({"event":event,"emittedAt":self.now}))?;
        self.effects
            .push(json!({"kind":"event","runId":id,"event":event}));
        Ok(())
    }
    fn close_ready(&mut self, sid: &str) -> Result<()> {
        let session = self.sessions.get_mut(sid).ok_or("unknown session")?;
        if session.closing_runtime {
            return Ok(());
        }
        let state = session.state.apply("snapshot", &Value::Null)?;
        if !state["activeRunId"].is_null() {
            return Ok(());
        }
        if session.runtime {
            session.closing_runtime = true;
            self.effect(sid, None, Purpose::Close, "close", json!({}))?;
        } else {
            session.visible = false;
            session.state.apply("closed", &Value::Null)?;
            let closes = std::mem::take(&mut session.closes);
            for id in closes {
                self.resolve(id, Value::Null);
            }
        }
        Ok(())
    }
}
fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
