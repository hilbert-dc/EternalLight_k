use serde_json::{Value, json};

pub(crate) const DESCRIPTION: &str = "Read evidence-bearing story impact within this live root advisory task's captured inputs. Resolve an exact entity first. No caller snapshot or revision override is allowed. maxHops 1-3, limit 1-20. The report covers explicit dependencies and known-name mentions, not all semantic consequences. Check coverage; empty or truncated findings do not prove absence. If model-item budget is exceeded, reduce limit/maxHops in the same task; do not rewrite history or confirm facts.";

pub(crate) fn validate(arguments: &Value) -> Result<(), String> {
    for (field, maximum) in [("maxHops", 3), ("limit", 20)] {
        if let Some(value) = arguments.get(field)
            && value
                .as_u64()
                .is_none_or(|number| !(1..=maximum).contains(&number))
        {
            return Err(format!("{field} is outside the impact read budget"));
        }
    }
    arguments["entityId"]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 160)
        .ok_or("entityId must be a non-empty string of at most 160 bytes")?;
    Ok(())
}

pub(crate) fn properties() -> Value {
    json!({"maxHops":{"type":"integer","minimum":1,"maximum":3}})
}
