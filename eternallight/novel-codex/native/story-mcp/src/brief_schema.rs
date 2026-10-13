use serde_json::{Value, json};

pub(super) fn payload() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["premise"],
    "properties":{
        "premise":{"type":"string"},
        "promise":{"type":"string"},"stakes":{"type":"string"},
        "endingDirection":{"type":"string"}
    }})
}

pub(super) fn validate(value: &Value) -> Result<(), String> {
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&payload())
        .map_err(|_| "brief validator configuration is invalid".to_owned())?;
    if validator.is_valid(value) {
        Ok(())
    } else {
        Err("payload must match the declared brief schema: string premise and optional string promise/stakes/endingDirection; no other fields".into())
    }
}
