use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RunState {
    Preparing,
    Running,
    WaitingUser,
    WaitingChild,
    WaitingTool,
    Cancelling,
    CancellationFailed,
    Completed,
    Failed,
    Cancelled,
    OutcomeUnknown,
}
impl RunState {
    fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::OutcomeUnknown
        )
    }
}

/// Owns a run's observable lifecycle and bounded replay history. Child events are
/// retained for diagnostics but never advance the root's state.
pub struct HostRun {
    id: Value,
    thread: Value,
    state: RunState,
    epoch: u64,
    turn: Value,
    sequence: u64,
    events: VecDeque<Value>,
    bytes: usize,
    maximum: usize,
    pending: Vec<(String, RunState)>,
}
impl HostRun {
    pub fn new(input: &Value) -> Result<Self> {
        let maximum = input["maximum"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 100_000)
            .ok_or("invalid retained event limit")? as usize;
        Ok(Self {
            id: input["runId"].clone(),
            thread: input["threadId"].clone(),
            state: RunState::Preparing,
            epoch: input["epoch"].as_u64().ok_or("invalid run epoch")?,
            turn: Value::Null,
            sequence: 0,
            events: VecDeque::new(),
            bytes: 0,
            maximum,
            pending: Vec::new(),
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "snapshot" => Ok(
                json!({"state":self.state,"epoch":self.epoch,"turn":self.turn,"sequence":self.sequence,"terminal":self.state.terminal()}),
            ),
            "start" => {
                if self.state != RunState::Preparing {
                    return Err("TASK_CANCELLED:engine run was cancelled".into());
                }
                self.state = RunState::Running;
                Ok(Value::Null)
            }
            "cancel" => {
                if !self.state.terminal() {
                    self.state = RunState::Cancelling;
                }
                Ok(Value::Null)
            }
            "cancelFailed" => {
                if !self.state.terminal() {
                    self.state = RunState::CancellationFailed;
                }
                Ok(Value::Null)
            }
            "epoch" => {
                let epoch = input
                    .as_u64()
                    .filter(|n| *n >= self.epoch)
                    .ok_or("run epoch cannot move backwards")?;
                self.epoch = epoch;
                Ok(Value::Null)
            }
            "turn" => {
                self.turn = input.clone();
                Ok(Value::Null)
            }
            "checkTerminal" | "terminal" => {
                let state: RunState = serde_json::from_value(input.clone())
                    .map_err(|_| "invalid terminal run state")?;
                if !state.terminal() {
                    return Err("invalid terminal run state".into());
                }
                if self.state.terminal() {
                    return Ok(false.into());
                }
                if matches!(
                    self.state,
                    RunState::Cancelling | RunState::CancellationFailed
                ) && !matches!(state, RunState::Cancelled | RunState::OutcomeUnknown)
                {
                    return Err("cancelling run requires reconciliation".into());
                }
                if method == "terminal" {
                    self.state = state;
                    self.pending.clear();
                }
                Ok(true.into())
            }
            "record" => self.record(input),
            "replay" => {
                let after = input.as_u64().ok_or("event subscription is invalid")?;
                let first = self
                    .events
                    .front()
                    .and_then(|e| e["sequence"].as_u64())
                    .unwrap_or(self.sequence + 1);
                if after > 0 && after < first - 1 {
                    return Err(format!(
                        "event cursor expired; first available sequence is {first}"
                    ));
                }
                Ok(self
                    .events
                    .iter()
                    .filter(|e| e["sequence"].as_u64().is_some_and(|n| n > after))
                    .cloned()
                    .collect::<Vec<_>>()
                    .into())
            }
            _ => Err("unknown host run operation".into()),
        }
    }
    fn record(&mut self, input: &Value) -> Result<Value> {
        let event = &input["event"];
        let params = event
            .get("params")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or_else(|| json!({}));
        if serde_json::to_vec(&params)
            .map_err(|_| "invalid event")?
            .len()
            > 256 * 1024
        {
            return Err("engine event exceeds byte limit".into());
        }
        let method = event["method"].as_str().unwrap_or("engine/unknown");
        let thread = params
            .get("threadId")
            .filter(|v| !v.is_null())
            .or_else(|| params["thread"].get("id"))
            .unwrap_or(&self.thread);
        let item = &params["item"];
        let item_id = params["itemId"].as_str().or_else(|| item["id"].as_str());
        if *thread == self.thread
            && !self.state.terminal()
            && !matches!(
                self.state,
                RunState::Cancelling | RunState::CancellationFailed
            )
        {
            if method == "turn/started" {
                self.state = RunState::Running;
            } else if method.to_lowercase().contains("approval")
                || method.to_lowercase().contains("userinput")
            {
                self.state = RunState::WaitingUser;
            } else if let Some(id) = item_id {
                if method == "item/started" {
                    let kind = item["type"].as_str().unwrap_or("");
                    let waiting = if kind == "collabAgentToolCall" {
                        Some(RunState::WaitingChild)
                    } else if kind.to_lowercase().contains("tool") {
                        Some(RunState::WaitingTool)
                    } else {
                        None
                    };
                    if let Some(waiting) = waiting {
                        if let Some(entry) = self.pending.iter_mut().find(|(key, _)| key == id) {
                            entry.1 = waiting;
                        } else {
                            if self.pending.len() >= 10_000 {
                                return Err("pending host item limit exceeded".into());
                            }
                            self.pending.push((id.into(), waiting));
                        }
                        self.state = waiting;
                    }
                } else if method == "item/completed" {
                    self.pending.retain(|(key, _)| key != id);
                    self.state = self.pending.last().map_or(RunState::Running, |(_, s)| *s);
                }
            }
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .filter(|n| *n <= 9_007_199_254_740_991)
            .ok_or("event sequence exhausted")?;
        let record = json!({"sequence":self.sequence,"runId":self.id,"threadId":thread,"parentThreadId":params["parentThreadId"],"turnId":params.get("turnId").filter(|v|!v.is_null()).or_else(||params["turn"].get("id")).unwrap_or(&self.turn["id"]),"itemId":item_id,"requestId":params["requestId"],"callId":params.get("callId").filter(|v|!v.is_null()).unwrap_or(&item["callId"]),"connectionId":params["connectionId"],"epoch":self.epoch,"emittedAt":input["emittedAt"],"method":method,"params":params});
        self.bytes += serde_json::to_vec(&record)
            .map_err(|_| "invalid event")?
            .len();
        self.events.push_back(record.clone());
        while self.events.len() > self.maximum || self.bytes > 16 * 1024 * 1024 {
            if let Some(old) = self.events.pop_front() {
                self.bytes = self
                    .bytes
                    .saturating_sub(serde_json::to_vec(&old).map_err(|_| "invalid event")?.len());
            }
        }
        Ok(record)
    }
}

/// Session admission state independent of asynchronous runtime callbacks.
pub struct HostSession {
    state: String,
    thread: Value,
    active: Value,
}
impl HostSession {
    pub fn new(state: &str) -> Result<Self> {
        if !matches!(state, "preparing" | "recovering") {
            return Err("invalid initial session state".into());
        }
        Ok(Self {
            state: state.into(),
            thread: Value::Null,
            active: Value::Null,
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "snapshot" => {
                return Ok(
                    json!({"state":self.state,"threadId":self.thread,"activeRunId":self.active}),
                );
            }
            "expect" => {
                if input.as_str() != Some(self.state.as_str()) {
                    return Err("TASK_CANCELLED:session closed during preparation".into());
                }
            }
            "ready" => {
                if !matches!(self.state.as_str(), "preparing" | "recovering") {
                    return Err("TASK_CANCELLED:session closed during preparation".into());
                }
                self.thread = input.clone();
                self.state = "idle".into();
            }
            "block" => {
                if !matches!(self.state.as_str(), "closing" | "closed") {
                    self.state = "recovery-blocked".into();
                }
            }
            "fail" => {
                if !matches!(self.state.as_str(), "closing" | "closed") {
                    self.state = "failed".into();
                }
            }
            "attach" => {
                if self.state != "idle" || !self.active.is_null() {
                    return Err("session is not idle".into());
                }
                if input.as_str().is_none_or(str::is_empty) {
                    return Err("invalid run id".into());
                }
                self.active = input.clone();
            }
            "finish" => {
                if self.active != input["runId"] {
                    return Err("session completion is not bound to its active run".into());
                }
                self.active = Value::Null;
                if !matches!(self.state.as_str(), "closing" | "closed") {
                    self.state = if input["state"] == "outcome-unknown" {
                        "recovery-blocked"
                    } else {
                        "idle"
                    }
                    .into();
                }
            }
            "closing" => self.state = "closing".into(),
            "closed" => self.state = "closed".into(),
            _ => return Err("unknown host session operation".into()),
        }
        Ok(Value::Null)
    }
}
