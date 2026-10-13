use crate::{Result, lease};
use serde_json::{Value, json};

pub(crate) fn now_ms() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "invalid renewal clock".to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "renewal clock overflow".into())
}

fn timestamp(value: &Value) -> Result<i64> {
    chrono::DateTime::parse_from_rfc3339(value.as_str().unwrap_or(""))
        .map(|time| time.timestamp_millis())
        .map_err(|_| "invalid renewal timestamp".into())
}

pub(crate) fn plan(previous: &Value, next: &Value, now: i64) -> Result<Value> {
    let old_expiry = timestamp(&previous["expiresAt"])?;
    let new_expiry = timestamp(&next["expiresAt"])?;
    if old_expiry <= now || new_expiry <= old_expiry {
        return Err(
            "session plan renewal requires an unexpired plan and later fresh evidence".into(),
        );
    }
    if previous["id"] == next["id"]
        || previous["evidenceDigest"] == next["evidenceDigest"]
        || next["evidenceRefs"].as_array().is_none_or(Vec::is_empty)
    {
        return Err("session plan renewal requires new evidence".into());
    }
    let scope = |value: &Value| -> Result<Value> {
        let mut scope = value.clone();
        let object = scope.as_object_mut().ok_or("invalid session plan")?;
        for key in ["id", "expiresAt", "evidenceRefs", "evidenceDigest"] {
            object.remove(key);
        }
        if let Some(dependencies) = object
            .get_mut("dependencyDigests")
            .and_then(Value::as_object_mut)
        {
            dependencies.remove("report");
        }
        Ok(scope)
    };
    if scope(previous)? != scope(next)? {
        return Err("session plan renewal cannot change frozen execution scope".into());
    }
    Ok(next.clone())
}

pub(crate) fn execution(input: &Value) -> Result<Value> {
    let now = input["now"].as_i64().ok_or("invalid renewal clock")?;
    let previous = &input["previousPlan"];
    let next = plan(previous, &input["sessionPlan"], now)?;
    let current = &input["currentLease"];
    lease::validate(
        &json!({"lease":current,"sessionPlan":previous,"epoch":input["epoch"],"now":now,"deadline":timestamp(&current["deadline"])?,"planExpiry":timestamp(&previous["expiresAt"])?}),
    )?;
    let mut renewed = current.clone();
    let expiry = timestamp(&next["expiresAt"])?;
    let cap = current["deadlineCap"].as_i64().unwrap_or(expiry);
    let deadline = expiry.min(cap);
    if deadline <= timestamp(&current["deadline"])? {
        return Err("execution lease renewal cannot extend an explicit execution deadline".into());
    }
    renewed["planId"] = next["id"].clone();
    renewed["deadline"] = chrono::DateTime::from_timestamp_millis(deadline)
        .ok_or("invalid renewed deadline")?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        .into();
    if let Some(supplied) = input.get("lease")
        && supplied != &renewed
    {
        return Err("renewed lease does not retain execution identity and deadline cap".into());
    }
    Ok(renewed)
}

pub(crate) fn dispatch(input: &Value) -> Result<Value> {
    plan(
        &input["previousPlan"],
        &input["sessionPlan"],
        input["now"].as_i64().ok_or("invalid renewal clock")?,
    )
}
