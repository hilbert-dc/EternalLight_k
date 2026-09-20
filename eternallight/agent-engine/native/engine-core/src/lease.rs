use crate::{
    Result,
    canonical::{array, string},
};
use serde_json::{Value, json};
use std::collections::HashMap;

pub fn create(input: &Value) -> Result<Value> {
    let plan = &input["sessionPlan"];
    if string(&plan["id"]).is_empty() || string(&plan["rootRoute"]).is_empty() {
        return Err("execution lease requires a session plan".into());
    }
    if string(&input["runId"]).is_empty() {
        return Err("execution lease requires a run id".into());
    }
    if string(&input["budgetId"]).is_empty() {
        return Err("execution lease requires a budget id".into());
    }
    let epoch = input["epoch"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("execution lease epoch is invalid")?;
    let duration = input["maxDurationMs"]
        .as_f64()
        .filter(|v| *v >= 1.0)
        .ok_or("execution lease duration is invalid")?;
    let now = input["now"]
        .as_f64()
        .ok_or("execution lease issue time is invalid")?;
    let expiry = input["planExpiry"]
        .as_f64()
        .ok_or("session plan expiry is invalid")?;
    let deadline = (now + duration).min(expiry);
    if deadline <= now {
        return Err("session plan has expired".into());
    }
    Ok(
        json!({"leaseId":uuid::Uuid::new_v4().to_string(),"runId":input["runId"],"planId":plan["id"],"mode":input["mode"],"rootRoute":plan["rootRoute"],"routeGrants":array(&plan["allowedChildEdges"]),"epoch":epoch,"budgetId":input["budgetId"],"issuedAt":now,"deadline":deadline}),
    )
}

pub fn validate(input: &Value) -> Result<Value> {
    let lease = &input["lease"];
    if lease.is_null() || lease["planId"] != input["sessionPlan"]["id"] {
        return Err("execution lease is not bound to the active session plan".into());
    }
    if lease["epoch"] != input["epoch"] {
        return Err("execution lease epoch has been revoked".into());
    }
    let now = input["now"]
        .as_f64()
        .ok_or("execution lease clock is invalid")?;
    let deadline = input["deadline"]
        .as_f64()
        .ok_or("execution lease deadline is invalid")?;
    let expiry = input["planExpiry"]
        .as_f64()
        .ok_or("session plan expiry is invalid")?;
    if deadline <= now || expiry <= now {
        return Err("execution lease has expired".into());
    }
    Ok(lease.clone())
}

/// Remembers authorized thread routes for a single lease without accepting caller snapshots.
pub struct RouteLineage {
    lease: Value,
    threads: HashMap<String, String>,
}

impl RouteLineage {
    pub fn new(lease: Value) -> Self {
        Self {
            lease,
            threads: HashMap::new(),
        }
    }

    pub fn authorize(&mut self, input: &Value) -> Result<Value> {
        let thread = input["threadId"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("trusted Codex thread identity is required")?;
        let route = input["routeId"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("model route identity is required")?;
        if let Some(existing) = self.threads.get(thread) {
            if existing != route {
                return Err("Codex thread attempted to change model route".into());
            }
            return Ok(route.into());
        }
        if let Some(parent) = input["parentThreadId"].as_str().filter(|v| !v.is_empty()) {
            let parent_route = self
                .threads
                .get(parent)
                .ok_or("Codex parent thread is outside the active execution lease")?;
            if !array(&self.lease["routeGrants"]).iter().any(|edge| {
                edge["parentConnectionId"] == *parent_route && edge["childConnectionId"] == route
            }) {
                return Err(format!(
                    "model route edge is not authorized: {parent_route} -> {route}"
                ));
            }
        } else if self.lease["rootRoute"] != route {
            return Err("only the root route may start a root Codex thread".into());
        }
        if self.threads.len() >= 100_000 {
            return Err("route lineage capacity exceeded".into());
        }
        self.threads.insert(thread.into(), route.into());
        Ok(route.into())
    }
}
