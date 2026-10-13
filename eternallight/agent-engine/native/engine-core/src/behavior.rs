use crate::{
    Result,
    canonical::{array, string},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn positive(value: &Value, label: &str) -> Result<u64> {
    value
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{label} must be a positive integer"))
}

fn text<'a>(value: &'a Value, label: &str, maximum: usize) -> Result<&'a str> {
    value
        .as_str()
        .filter(|v| !v.trim().is_empty() && v.encode_utf16().count() <= maximum)
        .ok_or_else(|| format!("{label} is invalid"))
}

pub(crate) fn task_set(input: &Value) -> Result<Value> {
    text(&input["taskSetId"], "behavior taskSetId", 160)?;
    positive(&input["taskSetVersion"], "behavior taskSetVersion")?;
    let tasks = input["tasks"]
        .as_array()
        .filter(|t| !t.is_empty())
        .ok_or("behavior tasks are required")?;
    if tasks.len() > 256 {
        return Err("behavior tasks exceed the task limit".into());
    }
    let mut ids = BTreeSet::new();
    let mut result = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        let id = text(&task["taskId"], &format!("behavior task {index} id"), 160)?;
        if !ids.insert(id) {
            return Err(format!("duplicate behavior task id: {id}"));
        }
        text(&task["prompt"], &format!("behavior task {id} prompt"), 8000)?;
        let mut normalized = json!({"taskId":id,"prompt":task["prompt"],"category":task.get("category").unwrap_or(&json!("general")),"metadata":task.get("metadata").unwrap_or(&json!({}))});
        text(
            &normalized["category"],
            &format!("behavior task {id} category"),
            160,
        )?;
        for field in ["requiredFacts", "forbiddenClaims"] {
            let values = task.get(field).cloned().unwrap_or_else(|| json!([]));
            if values
                .as_array()
                .is_none_or(|values| values.iter().any(|v| v.as_str().is_none_or(str::is_empty)))
            {
                return Err(format!("behavior task {id} assertions are invalid"));
            }
            normalized[field] = values;
        }
        if array(&normalized["requiredFacts"]).len() + array(&normalized["forbiddenClaims"]).len()
            > 128
        {
            return Err(format!("behavior task {id} exceeds the assertion limit"));
        }
        let requirements = task
            .get("executionRequirements")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !requirements.is_object() {
            return Err(format!(
                "behavior task {id} execution requirements are invalid"
            ));
        }
        let delegation = requirements["delegation"].as_str().unwrap_or("allowed");
        if !["required", "allowed", "forbidden"].contains(&delegation) {
            return Err(format!(
                "behavior task {id} delegation expectation is invalid"
            ));
        }
        let mut requirements_out = json!({"delegation":delegation});
        for field in ["requiredTools", "forbiddenTools"] {
            let values = requirements
                .get(field)
                .cloned()
                .unwrap_or_else(|| json!([]));
            if values.as_array().is_none_or(|values| {
                values.len() > 128
                    || values.iter().any(|v| {
                        v.as_str().is_none_or(|v| {
                            v.is_empty()
                                || v.len() > 160
                                || !v
                                    .bytes()
                                    .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
                        })
                    })
            }) {
                return Err(format!("behavior task {id} tool requirements are invalid"));
            }
            requirements_out[field] =
                json!(array(&values).iter().map(string).collect::<BTreeSet<_>>());
        }
        normalized["executionRequirements"] = requirements_out;
        result.push(normalized);
    }
    Ok(
        json!({"taskSetId":input["taskSetId"],"taskSetVersion":input["taskSetVersion"],"tasks":result}),
    )
}

pub(crate) fn facts(input: &Value) -> Value {
    if !input.is_object() {
        return Value::Null;
    }
    let tools = input.get("toolCalls").cloned().unwrap_or_else(|| json!([]));
    let lineage = input.get("lineage").cloned().unwrap_or_else(|| json!([]));
    if !tools.is_array() || !lineage.is_array() {
        return Value::Null;
    }
    if array(&tools).iter().any(|c| {
        !c["name"].is_string()
            || !matches!(
                c["status"].as_str(),
                Some("started" | "completed" | "failed" | "cancelled")
            )
    }) {
        return Value::Null;
    }
    if array(&lineage).iter().any(|e| {
        !e["threadId"].is_string()
            || e.get("parentThreadId")
                .is_none_or(|p| !p.is_null() && !p.is_string())
    }) {
        return Value::Null;
    }
    json!({"toolCalls":array(&tools).iter().map(|c|json!({"name":c["name"],"status":c["status"],"callId":c["callId"].as_str()})).collect::<Vec<_>>(),"lineage":array(&lineage).iter().map(|e|json!({"threadId":e["threadId"],"parentThreadId":e["parentThreadId"],"model":e["model"].as_str(),"status":e["status"].as_str()})).collect::<Vec<_>>()})
}

pub(crate) fn score(task: &Value, execution: &Value) -> Value {
    let output = string(&execution["output"]);
    let mut failures = Vec::new();
    for fact in array(&task["requiredFacts"])
        .iter()
        .map(string)
        .filter(|fact| !output.contains(fact))
    {
        failures.push(format!("missing-fact:{fact}"));
    }
    for claim in array(&task["forbiddenClaims"])
        .iter()
        .map(string)
        .filter(|claim| output.contains(claim))
    {
        failures.push(format!("forbidden-claim:{claim}"));
    }
    if execution["status"] == "completed" {
        let facts = facts(&execution["executionFacts"]);
        if facts.is_null() {
            failures.push("execution-facts-missing".into());
        } else {
            let requirements = &task["executionRequirements"];
            let delegated = array(&facts["lineage"])
                .iter()
                .any(|e| !e["parentThreadId"].is_null());
            if requirements["delegation"] == "required" && !delegated {
                failures.push("delegation-required".into());
            }
            if requirements["delegation"] == "forbidden" && delegated {
                failures.push("delegation-forbidden".into());
            }
            for tool in array(&requirements["requiredTools"]) {
                if !array(&facts["toolCalls"])
                    .iter()
                    .any(|c| c["name"] == *tool && c["status"] == "completed")
                {
                    failures.push(format!("required-tool-missing:{}", string(tool)));
                }
            }
            for tool in array(&requirements["forbiddenTools"]) {
                if array(&facts["toolCalls"])
                    .iter()
                    .any(|c| c["name"] == *tool)
                {
                    failures.push(format!("forbidden-tool-observed:{}", string(tool)));
                }
            }
        }
    }
    let completed = execution["status"] == "completed";
    let outcome = if completed && failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let category = if completed {
        &task["category"]
    } else {
        execution.get("failureCategory").unwrap_or(&Value::Null)
    };
    let category = category.as_str().unwrap_or("execution-failed");
    if failures.is_empty() && !completed {
        failures.push(category.into());
    }
    json!({"outcome":outcome,"category":category,"blockingFailures":failures})
}

pub(crate) fn suite(input: &Value) -> Result<Value> {
    let kind = string(&input["kind"]);
    if !["protocol", "core-integration", "execution-conformance"].contains(&kind) {
        return Err(format!("unknown qualification suite kind: {kind}"));
    }
    if string(&input["suiteId"]).is_empty() || input["suiteVersion"].as_u64().is_none_or(|n| n < 1)
    {
        return Err("qualification suite identity is invalid".into());
    }
    if !matches!(
        input["status"].as_str(),
        Some("passed" | "failed" | "not-run")
    ) {
        return Err("qualification suite status is invalid".into());
    }
    let count = positive(
        input.get("sampleCount").unwrap_or(&json!(1)),
        "qualification sampleCount",
    )?;
    let successes = input.get("successes").cloned().unwrap_or_else(|| {
        json!(if input["status"] == "passed" {
            count
        } else {
            0
        })
    });
    if successes.as_u64().is_none_or(|n| n > count) {
        return Err("qualification successes are invalid".into());
    }
    let failures = input
        .get("blockingFailures")
        .cloned()
        .unwrap_or_else(|| json!([]));
    if failures
        .as_array()
        .is_none_or(|v| v.iter().any(|v| v.as_str().is_none_or(str::is_empty)))
    {
        return Err("qualification blockingFailures are invalid".into());
    }
    Ok(
        json!({"kind":kind,"suiteId":input["suiteId"],"suiteVersion":input["suiteVersion"],"status":input["status"],"sampleCount":count,"successes":successes,"blockingFailures":failures,"metrics":input.get("metrics").unwrap_or(&json!({}))}),
    )
}

fn wilson(successes: u64, total: u64, z: f64) -> Result<f64> {
    if total == 0 || successes > total || !z.is_finite() || z <= 0.0 {
        return Err("invalid behavior sample counts".into());
    }
    let rate = successes as f64 / total as f64;
    let total = total as f64;
    let z2 = z * z;
    Ok((rate + z2 / (2.0 * total)
        - z * ((rate * (1.0 - rate) + z2 / (4.0 * total)) / total).sqrt())
        / (1.0 + z2 / total))
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "behavior.tasks" => task_set(input),
        "behavior.score" => Ok(score(&input["task"], &input["execution"])),
        "behavior.facts" => Ok(facts(input)),
        "behavior.suite" => suite(input),
        "behavior.wilson" => Ok(json!(wilson(
            input["successes"]
                .as_u64()
                .ok_or("invalid behavior sample counts")?,
            input["total"]
                .as_u64()
                .ok_or("invalid behavior sample counts")?,
            input["z"].as_f64().unwrap_or(1.959963984540054)
        )?)),
        "behavior.evaluate" => {
            let options = &input["options"];
            let minimum = positive(
                options.get("minimumSamples").unwrap_or(&json!(35)),
                "minimumSamples",
            )?;
            let bound = options
                .get("minimumLowerBound")
                .unwrap_or(&json!(0.9))
                .as_f64()
                .filter(|b| *b > 0.0 && *b <= 1.0)
                .ok_or("minimumLowerBound is invalid")?;
            let samples = input["samples"]
                .as_array()
                .filter(|s| !s.is_empty())
                .ok_or("autonomous behavior samples are required")?;
            let successes = samples.iter().filter(|s| s["outcome"] == "passed").count() as u64;
            let failures: BTreeSet<_> = samples
                .iter()
                .flat_map(|s| array(&s["blockingFailures"]).iter().map(string))
                .collect();
            let lower = wilson(successes, samples.len() as u64, 1.959963984540054)?;
            let mut categories = serde_json::Map::new();
            for sample in samples {
                let key = sample["category"].as_str().unwrap_or("passed");
                let count = categories.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
                categories.insert(key.to_owned(), count.into());
            }
            Ok(
                json!({"evaluationType":"autonomous-delegation","suiteId":"codex-autonomous-delegation-v1","suiteVersion":1,"status":if samples.len() as u64 >= minimum && lower >= bound && failures.is_empty() { "passed" } else { "failed" },"sampleCount":samples.len(),"successes":successes,"blockingFailures":failures,"metrics":{"successRate":successes as f64/samples.len() as f64,"wilsonLowerBound95":lower,"minimumSamples":minimum,"minimumLowerBound":bound,"categories":categories},"authorizationEffect":"none"}),
            )
        }
        _ => Err(format!("unknown behavior operation: {method}")),
    }
}
