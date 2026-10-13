use serde_json::Value;

// A Brief is planning content, never an alternative container for chapter prose.
pub(crate) fn validate(payload: &Value) -> Result<(), &'static str> {
    let fields = payload
        .as_object()
        .ok_or("payload must be a brief object")?;
    if fields.keys().any(|field| {
        !matches!(
            field.as_str(),
            "premise" | "promise" | "stakes" | "endingDirection"
        )
    }) {
        return Err("payload must contain only declared brief fields");
    }
    if fields.get("premise").is_none_or(|value| !value.is_string()) {
        return Err("payload/premise must be a string");
    }
    for (field, message) in [
        ("promise", "payload/promise must be a string"),
        ("stakes", "payload/stakes must be a string"),
        (
            "endingDirection",
            "payload/endingDirection must be a string",
        ),
    ] {
        if fields.get(field).is_some_and(|value| !value.is_string()) {
            return Err(message);
        }
    }
    Ok(())
}
