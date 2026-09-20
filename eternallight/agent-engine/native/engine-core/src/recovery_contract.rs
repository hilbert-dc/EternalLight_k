use crate::Result;
use serde_json::{Value, json};

fn text(value: &Value, name: &str, limit: usize) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    if value
        .as_str()
        .is_none_or(|s| s.is_empty() || s.encode_utf16().count() > limit)
    {
        return Err(format!("invalid recovery {name}"));
    }
    Ok(value.clone())
}

pub(crate) fn classification(value: &Value, tool: &Value) -> Result<Value> {
    if !matches!(value["effect"].as_str(), Some("read" | "write")) {
        return Err("recovery classify() must return a read or write effect".into());
    }
    Ok(
        json!({"effect":value["effect"],"operation":text(value.get("operation").filter(|v|!v.is_null()).unwrap_or(tool),"operation",500)?,"idempotencyRef":text(&value["idempotencyRef"],"idempotency reference",500)?}),
    )
}

pub(crate) fn resolution(value: &Value, source: &str) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    if !matches!(
        value["outcome"].as_str(),
        Some("committed" | "rejected" | "cancelled-before-execution" | "unknown")
    ) {
        return Err(format!("recovery {source}() returned an invalid outcome"));
    }
    let receipt = text(&value["receiptRef"], "receipt reference", 500)?;
    if value["outcome"] == "committed" && receipt.is_null() {
        return Err(format!(
            "recovery {source}() must bind committed outcomes to a receipt reference"
        ));
    }
    Ok(json!({"outcome":value["outcome"],"receiptRef":receipt}))
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "recoveryContract.classify" => classification(&input["value"], &input["tool"]),
        "recoveryContract.hash" => text(input, "operation hash", 256),
        "recoveryContract.resolve" => resolution(
            &input["value"],
            input["source"].as_str().unwrap_or("receipt"),
        ),
        _ => Err("unknown recovery contract operation".into()),
    }
}
