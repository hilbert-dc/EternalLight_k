use crate::Result;
use serde_json::{Value, json};

const BIND: [&str; 5] = [
    "budget",
    "bridge",
    "budgetStarted",
    "leaseStarted",
    "appBound",
];

struct Execution {
    generation: u64,
    lease: Value,
    stage: usize,
    revoked: bool,
    dispatching: bool,
}

/// Owns reusable-runtime admission and acquired-resource state. IO adapters
/// acknowledge acquisition even after cancellation so cleanup includes late
/// successful effects; cancellation never permits a subsequent acquisition.
#[derive(Default)]
pub struct SessionRuntime {
    closed: bool,
    blocked: bool,
    generation: u64,
    execution: Option<Execution>,
}

impl SessionRuntime {
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "snapshot" => Ok(json!({"closed":self.closed,"blocked":self.blocked})),
            "open" => {
                self.assert_open()?;
                Ok(Value::Null)
            }
            "begin" => {
                self.assert_open()?;
                if self.blocked {
                    return Err("Codex runtime cleanup is unresolved; close this runtime".into());
                }
                if self.execution.is_some() {
                    return Err("Codex runtime already has an active execution".into());
                }
                let lease = &input["lease"];
                if ["runId", "leaseId"]
                    .iter()
                    .any(|name| lease[name].as_str().is_none_or(str::is_empty))
                    || lease["epoch"].as_u64().is_none()
                {
                    return Err("Codex runtime execution lease is invalid".into());
                }
                self.generation = self
                    .generation
                    .checked_add(1)
                    .filter(|n| *n <= 9_007_199_254_740_991)
                    .ok_or("Codex runtime generation exhausted")?;
                self.execution = Some(Execution {
                    generation: self.generation,
                    lease: lease.clone(),
                    stage: 0,
                    revoked: false,
                    dispatching: false,
                });
                Ok(self.generation.into())
            }
            "current" | "dispatch" => {
                self.assert_open()?;
                let execution = self.execution(input)?;
                if execution.revoked {
                    return Err("TASK_CANCELLED:Codex runtime execution was revoked".into());
                }
                if method == "dispatch" {
                    if execution.stage != BIND.len() {
                        return Err("Codex runtime execution is still binding".into());
                    }
                    if execution.dispatching {
                        return Err("Codex runtime turn is already active".into());
                    }
                    execution.dispatching = true;
                }
                Ok(Value::Null)
            }
            "next" => {
                let execution = self.execution(input)?;
                Ok(BIND.get(execution.stage).map_or(Value::Null, |s| json!(s)))
            }
            "ack" => {
                let execution = self.execution(input)?;
                if BIND
                    .get(execution.stage)
                    .is_none_or(|step| input["step"] != *step)
                {
                    return Err("out-of-order runtime acquisition acknowledgement".into());
                }
                execution.stage += 1;
                Ok(Value::Null)
            }
            "execution" => {
                let execution = self.execution(input)?;
                Ok(
                    json!({"revoked":execution.revoked,"budgetStarted":execution.stage>=3,"leaseStarted":execution.stage>=4,"appBound":execution.stage>=5}),
                )
            }
            "revoke" => {
                self.assert_open()?;
                let execution = self
                    .execution
                    .as_mut()
                    .ok_or("Codex runtime has no active execution")?;
                let request = &input["request"];
                if execution.lease["runId"] != request["runId"]
                    || execution.lease["leaseId"] != request["leaseId"]
                    || execution.lease["epoch"] != request["expectedEpoch"]
                {
                    return Err("Codex runtime execution lease mismatch".into());
                }
                execution.revoked = true;
                Ok(Value::Null)
            }
            "close" => {
                self.closed = true;
                if let Some(execution) = &mut self.execution {
                    execution.revoked = true;
                }
                Ok(Value::Null)
            }
            "cleanup" => {
                let execution = self.execution(input)?;
                let mut effects = Vec::new();
                if execution.stage >= 4 {
                    effects.push("lease");
                }
                if execution.stage >= 3 {
                    effects.push("budget");
                } else if execution.stage >= 1 {
                    effects.push("budgetRecord");
                }
                if execution.stage >= 5 {
                    effects.push("app");
                }
                Ok(json!(effects))
            }
            "settled" => {
                self.execution(input)?;
                self.blocked |= input["failed"] == true;
                self.execution = None;
                Ok(Value::Null)
            }
            _ => Err("unknown session runtime operation".into()),
        }
    }

    fn assert_open(&self) -> Result<()> {
        if self.closed {
            Err("Codex runtime is closed".into())
        } else {
            Ok(())
        }
    }

    fn execution(&mut self, input: &Value) -> Result<&mut Execution> {
        self.execution
            .as_mut()
            .filter(|e| Some(e.generation) == input["generation"].as_u64())
            .ok_or_else(|| "TASK_CANCELLED:Codex runtime execution was revoked".into())
    }
}
