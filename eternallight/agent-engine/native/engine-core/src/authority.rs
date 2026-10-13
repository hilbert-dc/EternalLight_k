use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum State {
    Active,
    Revoked,
    Terminal,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Completed,
    Failed,
    Cancelled,
    #[serde(rename = "outcome-unknown")]
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    run_id: String,
    lease_id: String,
    plan_id: Value,
    epoch: u64,
    state: State,
    drained: bool,
    reconciled: bool,
    outcome: Option<Outcome>,
    reason: Option<String>,
}

/// Owns revocation and terminal state; serialized snapshots never grant authority.
#[derive(Default)]
pub struct ExecutionAuthority {
    records: HashMap<String, Record>,
}

impl ExecutionAuthority {
    pub fn apply(&mut self, method: &str, input: Value) -> Result<Value> {
        let run = if method == "issue" {
            &input["lease"]["runId"]
        } else {
            &input["runId"]
        };
        let run = run
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("execution authority requires a valid run id")?;
        if method == "issue" {
            let lease = &input["lease"];
            let id = lease["leaseId"]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or("execution authority requires a valid lease")?;
            let epoch = lease["epoch"]
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or("execution authority requires a valid lease")?;
            if self.records.contains_key(run) {
                return Err(format!("execution authority already contains run: {run}"));
            }
            if self.records.len() >= 100_000 {
                return Err("execution authority capacity exceeded".into());
            }
            self.records.insert(
                run.into(),
                Record {
                    run_id: run.into(),
                    lease_id: id.into(),
                    plan_id: lease["planId"].clone(),
                    epoch,
                    state: State::Active,
                    drained: false,
                    reconciled: false,
                    outcome: None,
                    reason: None,
                },
            );
        } else {
            let record = self
                .records
                .get_mut(run)
                .ok_or_else(|| format!("unknown execution authority run: {run}"))?;
            match method {
                "validate" | "renew" => {
                    if input["leaseId"] != record.lease_id
                        || input["epoch"].as_u64() != Some(record.epoch)
                        || record.state != State::Active
                    {
                        return Err(
                            "EXECUTION_REVOKED:execution authority rejected the lease".into()
                        );
                    }
                    if method == "renew" {
                        record.plan_id = input["planId"].clone();
                    }
                }
                "revoke" => {
                    if input["leaseId"] != record.lease_id {
                        return Err("execution authority lease id mismatch".into());
                    }
                    if record.state == State::Active {
                        if input["expectedEpoch"].as_u64() != Some(record.epoch) {
                            return Err("execution authority epoch mismatch".into());
                        }
                        record.epoch = record
                            .epoch
                            .checked_add(1)
                            .ok_or("execution authority epoch overflow")?;
                        record.state = State::Revoked;
                        record.reason = Some(
                            input["reason"]
                                .as_str()
                                .unwrap_or("cancelled")
                                .chars()
                                .take(240)
                                .collect(),
                        );
                    }
                }
                "drained" => {
                    if record.state == State::Active {
                        return Err("active execution cannot be marked drained".into());
                    }
                    record.drained = true;
                }
                "reconciled" => {
                    if !record.drained {
                        return Err("execution must drain before reconciliation".into());
                    }
                    record.reconciled = true;
                }
                "terminalize" => {
                    let outcome: Outcome = serde_json::from_value(input["outcome"].clone())
                        .map_err(|_| "execution authority outcome is invalid")?;
                    if record.outcome.is_some_and(|previous| previous != outcome) {
                        return Err("execution authority terminal outcome is immutable".into());
                    }
                    if matches!(outcome, Outcome::Cancelled | Outcome::Unknown)
                        && (!record.drained || !record.reconciled)
                    {
                        return Err(
                            "cancelled execution must drain and reconcile before terminalizing"
                                .into(),
                        );
                    }
                    record.state = State::Terminal;
                    record.outcome = Some(outcome);
                }
                "snapshot" => {}
                "release" => {
                    if record.state != State::Terminal {
                        return Err("cannot release non-terminal execution authority".into());
                    }
                    self.records.remove(run);
                    return Ok(Value::Null);
                }
                _ => return Err("unknown execution authority operation".into()),
            }
        }
        serde_json::to_value(self.records.get(run).unwrap())
            .map_err(|_| "invalid authority snapshot".into())
    }
}
