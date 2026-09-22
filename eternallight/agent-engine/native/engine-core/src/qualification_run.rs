use crate::{
    Result,
    canonical::{array, string},
    delegation,
    digest::digest,
    lease, message_evidence,
    trace::{self, ExecutionTrace},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "qualification_observation.rs"]
mod observation;
#[path = "qualification_workspace.rs"]
mod workspace;
use workspace::{Workspace, read_bounded, read_json, write_private};

/// Owns a bounded, isolated delegation qualification run. The host only executes
/// the next IO callback; proof checks and cleanup order cannot be skipped.
pub struct QualificationRun {
    input: Value,
    workspace: Option<Workspace>,
    trace: ExecutionTrace,
    requests: Vec<Value>,
    responses: Vec<Value>,
    events: Vec<Value>,
    observed_bytes: usize,
    pending: Option<(u64, String)>,
    next: u64,
    effects: Vec<Value>,
    started: bool,
    budget: Value,
    lease: Value,
    address: Value,
    result: Value,
    error: Option<String>,
    collecting: Option<Instant>,
    cleaning: bool,
    cleanup: Vec<String>,
    finished: bool,
    gate_release: Option<Instant>,
    failure: Option<Value>,
}
impl QualificationRun {
    pub fn new(mut input: Value) -> Result<Self> {
        if !matches!(
            input["scenario"].as_str(),
            Some("spawn-route" | "message-running" | "wait" | "core")
        ) {
            return Err("unknown conformance scenario".into());
        }
        if array(&input["command"]).is_empty() {
            return Err("conformance runner requires the current Codex App Server command".into());
        }
        if string(&input["baseInstructions"]).trim().is_empty() {
            return Err("conformance runner requires upstream Codex instructions".into());
        }
        let timeout = input["timeoutMs"].as_u64().unwrap_or(90_000);
        if timeout == 0 || timeout > 600_000 {
            return Err("invalid conformance timeout".into());
        }
        input["timeoutMs"] = timeout.into();
        let workspace = Workspace::new()?;
        let trace = ExecutionTrace::new(
            &json!({"runId":format!("conformance-{}",workspace.nonce),"maxEvents":320}),
        )?;
        Ok(Self {
            input,
            workspace: Some(workspace),
            trace,
            requests: Vec::new(),
            responses: Vec::new(),
            events: Vec::new(),
            observed_bytes: 0,
            pending: None,
            next: 0,
            effects: Vec::new(),
            started: false,
            budget: Value::Null,
            lease: Value::Null,
            address: Value::Null,
            result: Value::Null,
            error: None,
            collecting: None,
            cleaning: false,
            cleanup: Vec::new(),
            finished: false,
            gate_release: None,
            failure: None,
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        let operation = match method {
            "start" => {
                if self.started {
                    return Err("qualification run already started".into());
                }
                self.started = true;
                self.effect(
                    "bridgeStart",
                    self.workspace.as_ref().unwrap().bridge_options(&self.input),
                );
                Ok(())
            }
            "callback" => self.callback(input),
            "request" | "response" | "event" => self.observe(method, input),
            "tick" => Ok(()),
            _ => return Err("unknown qualification run operation".into()),
        };
        if let Err(error) = operation {
            self.fail(error);
        }
        if let Err(error) = self.advance() {
            self.fail(error);
            self.advance()?;
        }
        Ok(std::mem::take(&mut self.effects).into())
    }
    fn effect(&mut self, method: &str, params: Value) {
        self.next += 1;
        self.pending = Some((self.next, method.into()));
        self.effects
            .push(json!({"kind":"callback","id":self.next,"method":method,"params":params}));
    }
    fn callback(&mut self, input: &Value) -> Result<()> {
        if self
            .pending
            .as_ref()
            .is_none_or(|(id, _)| Some(*id) != input["id"].as_u64())
        {
            return Ok(());
        }
        let (_, method) = self.pending.take().unwrap();
        if let Some(error) = input["error"].as_str() {
            self.fail(error.chars().take(1000).collect());
            if method == "budgetSnapshot" {
                self.finalize()?;
            }
            return Ok(());
        }
        let value = &input["result"];
        match method.as_str() {
            "bridgeStart" => {
                self.address = value.clone();
                self.effect("budgetCreate",json!({"maxRequests":16,"maxTokens":200_000,"maxDurationMs":self.input["timeoutMs"]}));
            }
            "budgetCreate" => {
                self.budget = value.clone();
                let workspace = self.workspace.as_ref().unwrap();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_millis() as u64;
                let expiry = chrono::DateTime::parse_from_rfc3339(string(
                    &self.input["sessionPlan"]["expiresAt"],
                ))
                .map_err(|_| "invalid conformance plan expiry")?
                .timestamp_millis();
                self.lease = lease::create(
                    &json!({"sessionPlan":self.input["sessionPlan"],"runId":format!("conformance-{}",workspace.nonce),"budgetId":value["id"],"epoch":1,"mode":"conformance","now":now,"planExpiry":expiry,"maxDurationMs":self.input["timeoutMs"]}),
                )?;
                for field in ["issuedAt", "deadline"] {
                    let millis = self.lease[field]
                        .as_f64()
                        .ok_or("invalid conformance lease time")?
                        as i64;
                    self.lease[field] = chrono::DateTime::from_timestamp_millis(millis)
                        .ok_or("invalid lease time")?
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                        .into();
                }
                self.effect("budgetStart", Value::Null);
            }
            "budgetStart" => self.effect("leaseStart", self.lease.clone()),
            "leaseStart" => {
                let params = self
                    .workspace
                    .as_ref()
                    .unwrap()
                    .prepare(&self.input, &self.address)?;
                self.effect("appCreate", params);
            }
            "appCreate" => self.effect("ensureSession", Value::Null),
            "ensureSession" => {
                let prompt = self.workspace.as_ref().unwrap().prompt(&self.input);
                self.effect("sendTurn",json!({"input":[{"type":"text","text":prompt}],"options":{"disableEnvironment":true,"threadName":"Codex Engine conformance"}}));
            }
            "sendTurn" => {
                self.result = value.clone();
                self.collecting = Some(
                    Instant::now()
                        + Duration::from_millis(
                            self.input["timeoutMs"].as_u64().unwrap().min(2000),
                        ),
                );
            }
            "budgetSnapshot" => {
                self.budget = value.clone();
                self.finalize()?;
            }
            "closeApp" | "endLease" | "endBudget" | "closeBridge" => {}
            _ => return Err("unknown qualification callback".into()),
        }
        Ok(())
    }
    fn fail(&mut self, error: String) {
        if self.error.is_none() {
            self.error = Some(error);
        }
        self.collecting = None;
    }
    fn advance(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        if let Some(deadline) = self.gate_release {
            let workspace = self.workspace.as_ref().unwrap();
            if workspace.gate.exists() && !workspace.release.exists() {
                write_private(&workspace.release, "{\"released\":true}\n")?;
                self.gate_release = None;
            } else if Instant::now() >= deadline {
                self.gate_release = None;
            }
        }
        if self.pending.is_some() {
            return Ok(());
        }
        if self.cleaning {
            if !self.cleanup.is_empty() {
                let method = self.cleanup.remove(0);
                self.effect(&method, self.lease.clone());
            } else {
                if let Some(workspace) = self.workspace.take()
                    && let Err(error) = workspace.close()
                {
                    self.fail(error);
                }
                self.finished = true;
                if self.input["scenario"] == "core" {
                    if self.error.is_some() {
                        self.result = self.core_result()?;
                    }
                    self.effects
                        .push(json!({"kind":"result","result":self.result}));
                } else if let Some(error) = &self.error {
                    let mut result = self.failure.clone().unwrap_or_else(|| json!({}));
                    result["message"] = error.clone().into();
                    self.effects.push(json!({"kind":"error","error":result}));
                } else {
                    self.effects
                        .push(json!({"kind":"result","result":self.result}));
                }
            }
            return Ok(());
        }
        if self.error.is_some() {
            self.effect("budgetSnapshot", Value::Null);
            return Ok(());
        }
        if let Some(deadline) = self.collecting {
            let ready = match string(&self.input["scenario"]) {
                "message-running" => self.proof_ready(),
                "spawn-route" => self.observations()?["childIdentityObserved"] == true,
                "wait" | "core" => true,
                _ => false,
            };
            if ready || Instant::now() >= deadline {
                self.collecting = None;
                self.effect("budgetSnapshot", Value::Null);
            }
        }
        Ok(())
    }
    fn finalize(&mut self) -> Result<()> {
        if let Some(text) = read_bounded(&self.workspace.as_ref().unwrap().trace) {
            for line in text.lines() {
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    self.trace.record(
                        &json!({"source":"mcp","type":value["type"],"details":value["details"]}),
                    )?;
                }
            }
        }
        if self.input["scenario"] == "core" {
            self.result = self.core_result()?;
        } else {
            if self.error.is_none()
                && let Err(error) = self.success()
            {
                self.fail(error);
            }
            if self.error.is_some() {
                self.failure = Some(self.diagnostics()?);
            }
        }
        self.cleaning = true;
        self.cleanup = vec!["closeApp".into()];
        if !self.lease.is_null() {
            self.cleanup.push("endLease".into());
        }
        if !self.budget.is_null() {
            self.cleanup.push("endBudget".into());
        }
        self.cleanup.push("closeBridge".into());
        Ok(())
    }
    fn core_result(&self) -> Result<Value> {
        let mut input = self.input.clone();
        if let Some(workspace) = &self.workspace {
            let proof = read_json(&workspace.proof);
            input["passed"] = (self.error.is_none()
                && proof["receipt"] == workspace.receipt
                && proof["callerThreadId"].is_string()
                && proof["callerThreadId"] == proof["rootThreadId"]
                && string(&self.result["message"])
                    .contains(&format!("CORE_CONFORMANCE_RECEIPT:{}", workspace.receipt)))
            .into();
            input["proofPath"] = json!(workspace.proof);
        } else {
            input["passed"] = false.into();
            input["proofPath"] = self.result["record"]["assertions"][0]["traceRef"].clone();
        }
        input["error"] = json!(self.error);
        crate::qualification_records::dispatch("qualification.coreResult", &input)
    }
}
