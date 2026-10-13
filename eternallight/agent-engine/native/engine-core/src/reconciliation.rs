use crate::{Result, recovery_contract};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Candidates,
    Resolve,
    Settle,
    Runtime,
    Verify,
    Complete,
}

/// Owns reconciliation ordering. An adapter executes each returned effect and
/// acknowledges it only after success; a failed persistence effect cannot be
/// reported as a reconciled operation.
pub struct Reconciliation {
    run_id: String,
    phase: Phase,
    candidates: Vec<Value>,
    cursor: usize,
    resolution: Value,
    operations: Vec<Value>,
    runtime_status: Value,
    fence: Value,
    reconciled_at: Value,
    result: Value,
}

fn candidates(value: &Value, run_id: &str) -> Result<Vec<Value>> {
    let items = value
        .as_array()
        .ok_or("reconciliation candidates must be an array")?;
    if items.len() > 10_000 || value.to_string().len() > 8 * 1024 * 1024 {
        return Err("reconciliation candidates exceed limit".into());
    }
    let mut seen = HashSet::new();
    for item in items {
        let id = item["requestId"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 2048)
            .ok_or("invalid reconciliation request identity")?;
        if !seen.insert(id) || item["runId"].as_str().is_some_and(|id| id != run_id) {
            return Err("duplicate or foreign reconciliation candidate".into());
        }
    }
    Ok(items.clone())
}

impl Reconciliation {
    pub fn new(input: &Value) -> Result<Self> {
        let run_id = input["runId"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 2048)
            .ok_or("invalid reconciliation run identity")?
            .to_owned();
        Ok(Self {
            run_id,
            phase: Phase::Candidates,
            candidates: Vec::new(),
            cursor: 0,
            resolution: Value::Null,
            operations: Vec::new(),
            runtime_status: input["runtimeStatus"].clone(),
            fence: input["fence"].clone(),
            reconciled_at: input["reconciledAt"].clone(),
            result: Value::Null,
        })
    }

    pub fn next(&self) -> Value {
        let args = match self.phase {
            Phase::Candidates | Phase::Verify => json!([self.run_id]),
            Phase::Resolve => json!([self.candidates[self.cursor]]),
            Phase::Settle => json!([
                self.run_id,
                self.candidates[self.cursor]["requestId"],
                self.resolution,
                self.reconciled_at
            ]),
            Phase::Runtime => {
                json!([self.run_id, self.runtime_status, self.reconciled_at, {"fence":self.fence}])
            }
            Phase::Complete => return json!({"method":"complete","result":self.result}),
        };
        let method = match self.phase {
            Phase::Candidates | Phase::Verify => "reconciliationCandidates",
            Phase::Resolve => "resolve",
            Phase::Settle => "settleRequest",
            Phase::Runtime => "reconcileRun",
            Phase::Complete => unreachable!(),
        };
        json!({"method":method,"args":args})
    }

    pub fn accept(&mut self, input: &Value) -> Result<Value> {
        if input["method"] != self.next()["method"] || self.phase == Phase::Complete {
            return Err("out-of-order reconciliation acknowledgement".into());
        }
        match self.phase {
            Phase::Candidates => {
                self.candidates = candidates(&input["value"], &self.run_id)?;
                self.phase = if self.candidates.is_empty() {
                    self.after_operations()
                } else {
                    Phase::Resolve
                };
            }
            Phase::Resolve => {
                let resolution = recovery_contract::resolution(&input["value"], "reconcile")?;
                if resolution.is_null() {
                    return Err("reconciliation requires an explicit outcome".into());
                }
                self.resolution = resolution;
                self.phase = Phase::Settle;
            }
            Phase::Settle => {
                let mut operation = self.resolution.clone();
                operation["requestId"] = self.candidates[self.cursor]["requestId"].clone();
                self.operations.push(operation);
                self.cursor += 1;
                self.phase = if self.cursor == self.candidates.len() {
                    self.after_operations()
                } else {
                    Phase::Resolve
                };
            }
            Phase::Runtime => self.phase = Phase::Verify,
            Phase::Verify => {
                let unresolved: Vec<_> = candidates(&input["value"], &self.run_id)?
                    .iter()
                    .map(
                        |item| json!({"requestId":item["requestId"],"operation":item["operation"]}),
                    )
                    .collect();
                self.result = json!({"runId":self.run_id,"status":if unresolved.is_empty() {"reconciled"} else {"outcome-unknown"},"operations":self.operations,"unresolved":unresolved});
                self.phase = Phase::Complete;
            }
            Phase::Complete => unreachable!(),
        }
        Ok(self.next())
    }

    fn after_operations(&self) -> Phase {
        if self.runtime_status.is_null() {
            Phase::Verify
        } else {
            Phase::Runtime
        }
    }
}
