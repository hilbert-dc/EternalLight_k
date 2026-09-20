use crate::{Result, behavior};
use serde_json::{Value, json};

/// Owns evaluation order and accounting. External callbacks return observations,
/// not authority to skip samples, restart a run, or refund consumed tokens.
pub struct BehaviorRun {
    tasks: Value,
    total: u64,
    budget: Value,
    started: u64,
    clock: u64,
    active: Option<u64>,
    samples: Vec<Value>,
    tokens: u64,
    stopped: Option<&'static str>,
    result_bytes: usize,
}

fn positive(value: &Value, label: &str) -> Result<u64> {
    value
        .as_u64()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
        .ok_or_else(|| format!("{label} must be a positive integer"))
}

fn usage_tokens(usage: &Value) -> u64 {
    let first = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| usage.get(name).filter(|v| !v.is_null()))
    };
    if let Some(total) = first(&["totalTokens", "total_tokens"]) {
        return total.as_u64().unwrap_or(0);
    }
    let input = first(&[
        "inputTokens",
        "promptTokens",
        "input_tokens",
        "prompt_tokens",
    ]);
    let output = first(&[
        "outputTokens",
        "completionTokens",
        "output_tokens",
        "completion_tokens",
    ]);
    match (
        input.unwrap_or(&Value::from(0)).as_u64(),
        output.unwrap_or(&Value::from(0)).as_u64(),
    ) {
        (Some(input), Some(output)) => input.saturating_add(output),
        _ => 0,
    }
}

impl BehaviorRun {
    pub fn new(input: &Value) -> Result<Self> {
        let tasks = behavior::task_set(&input["taskSet"])?;
        let repetitions = positive(&input["repetitions"], "behavior repetitions")?;
        let total = repetitions
            .checked_mul(tasks["tasks"].as_array().ok_or("invalid tasks")?.len() as u64)
            .filter(|n| *n <= 10_000)
            .ok_or("behavior schedule exceeds the sample limit")?;
        let source = &input["budget"];
        if !source.is_null() && !source.is_object() {
            return Err("behavior budget is invalid".into());
        }
        let mut budget = json!({});
        for (name, default) in [
            ("maxExecutions", total),
            ("maxTotalTokens", 1_000_000),
            ("maxDurationMs", 1_800_000),
        ] {
            let value = source
                .get(name)
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(default.into());
            budget[name] = positive(&value, &format!("behavior budget {name}"))?.into();
        }
        if total > budget["maxExecutions"].as_u64().unwrap_or(0) {
            return Err("behavior schedule exceeds the total execution budget".into());
        }
        let started = input["now"].as_u64().ok_or("invalid behavior clock")?;
        Ok(Self {
            tasks,
            total,
            budget,
            started,
            clock: started,
            active: None,
            samples: Vec::new(),
            tokens: 0,
            stopped: None,
            result_bytes: 0,
        })
    }

    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        let now = input["now"]
            .as_u64()
            .filter(|now| *now >= self.clock)
            .ok_or("behavior clock must be monotonic")?;
        self.clock = now;
        match method {
            "next" => {
                if self.active.is_some() {
                    return Err("behavior sample is already running".into());
                }
                if self.stopped.is_some() || self.samples.len() as u64 == self.total {
                    return Ok(Value::Null);
                }
                if now - self.started >= self.budget["maxDurationMs"].as_u64().unwrap_or(0) {
                    self.stopped = Some("duration-budget-exhausted");
                } else if self.tokens >= self.budget["maxTotalTokens"].as_u64().unwrap_or(0) {
                    self.stopped = Some("token-budget-exhausted");
                }
                if self.stopped.is_some() {
                    return Ok(Value::Null);
                }
                let tasks = self.tasks["tasks"].as_array().ok_or("invalid tasks")?;
                let index = self.samples.len();
                self.active = Some(now);
                Ok(
                    json!({"sampleId":index,"task":tasks[index % tasks.len()],"repetition":index / tasks.len()}),
                )
            }
            "complete" => {
                let started = self.active.ok_or("no behavior sample is running")?;
                if input["sampleId"].as_u64() != Some(self.samples.len() as u64) {
                    return Err("behavior sample identity mismatch".into());
                }
                let score = &input["score"];
                let tasks = self.tasks["tasks"].as_array().ok_or("invalid tasks")?;
                let task = &tasks[self.samples.len() % tasks.len()];
                if !matches!(score["outcome"].as_str(), Some("passed" | "failed")) {
                    return Err(format!(
                        "behavior scorer returned an invalid outcome for {}",
                        task["taskId"].as_str().unwrap_or("")
                    ));
                }
                let mut failures = score
                    .get("blockingFailures")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                let failures = failures
                    .as_array_mut()
                    .filter(|values| values.iter().all(Value::is_string))
                    .ok_or("invalid behavior blocking failures")?;
                let execution = &input["execution"];
                let tokens = self
                    .tokens
                    .saturating_add(usage_tokens(&execution["usage"]));
                let mut budget_failures = Vec::new();
                if tokens > self.budget["maxTotalTokens"].as_u64().unwrap_or(0) {
                    budget_failures.push("token-budget-exceeded");
                }
                if now - self.started > self.budget["maxDurationMs"].as_u64().unwrap_or(0) {
                    budget_failures.push("duration-budget-exceeded");
                }
                failures.extend(budget_failures.iter().copied().map(Value::from));
                let sample = json!({"taskId":task["taskId"],"category":score.get("category").filter(|v| !v.is_null()).unwrap_or(&task["category"]),"repetition":self.samples.len()/tasks.len(),"outcome":if score["outcome"]=="passed" && budget_failures.is_empty() {"passed"} else {"failed"},"blockingFailures":failures,"durationMs":now-started,"usage":execution.get("usage").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| json!({})),"runRef":execution["runRef"].as_str(),"executionFacts":behavior::facts(&execution["executionFacts"])});
                let size = serde_json::to_vec(&sample)
                    .map_err(|_| "invalid behavior sample")?
                    .len();
                if self.result_bytes.saturating_add(size) > 16 * 1024 * 1024 {
                    return Err("behavior results exceed the byte limit".into());
                }
                self.result_bytes += size;
                self.tokens = tokens;
                self.stopped = budget_failures.first().copied();
                self.samples.push(sample);
                self.active = None;
                Ok(Value::Null)
            }
            "result" => {
                if self.active.is_some()
                    || (self.stopped.is_none() && self.samples.len() as u64 != self.total)
                {
                    return Err("behavior run is not complete".into());
                }
                let mut budget = self.budget.clone();
                budget["executions"] = self.samples.len().into();
                budget["consumedTokens"] = self.tokens.into();
                budget["durationMs"] = (now - self.started).into();
                budget["stoppedReason"] = json!(self.stopped);
                Ok(
                    json!({"taskSetId":self.tasks["taskSetId"],"taskSetVersion":self.tasks["taskSetVersion"],"authorizationEffect":"none","samples":self.samples,"budget":budget}),
                )
            }
            _ => Err("unknown behavior run operation".into()),
        }
    }
}
