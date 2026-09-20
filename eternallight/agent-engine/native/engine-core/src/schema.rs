use crate::Result;
use serde_json::Value;

const KEYWORDS: &[&str] = &[
    "$defs",
    "$ref",
    "additionalProperties",
    "allOf",
    "anyOf",
    "const",
    "description",
    "encrypted",
    "enum",
    "items",
    "maxItems",
    "maxLength",
    "maximum",
    "minItems",
    "minLength",
    "minimum",
    "not",
    "oneOf",
    "properties",
    "required",
    "title",
    "type",
    "uniqueItems",
];

fn normalize(schema: &Value, depth: usize) -> Result<Value> {
    if depth > 48 {
        return Err("JSON Schema exceeds maximum depth".into());
    }
    if schema.is_boolean() {
        return Ok(schema.clone());
    }
    let mut map = schema
        .as_object()
        .cloned()
        .ok_or("tool parameter schema must be an object or boolean schema")?;
    for (key, value) in &map {
        if key == "pattern" {
            return Err("unsupported JSON Schema keyword: pattern (unbounded regular expressions are not executable)".into());
        }
        if !KEYWORDS.contains(&key.as_str()) {
            return Err(format!("unsupported JSON Schema keyword: {key}"));
        }
        if key == "encrypted" && !value.is_boolean() {
            return Err("JSON Schema encrypted annotation must be boolean".into());
        }
        if key == "$ref" && !value.as_str().is_some_and(|s| s.starts_with("#/$defs/")) {
            return Err("only local $defs JSON Schema references are supported".into());
        }
    }
    map.remove("encrypted");
    for key in ["items", "additionalProperties", "not"] {
        if let Some(value) = map.get_mut(key) {
            *value = normalize(value, depth + 1)?;
        }
    }
    for key in ["properties", "$defs"] {
        if let Some(value) = map.get_mut(key) {
            for child in value
                .as_object_mut()
                .ok_or("schema properties must be an object")?
                .values_mut()
            {
                *child = normalize(child, depth + 1)?;
            }
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(value) = map.get_mut(key) {
            for child in value
                .as_array_mut()
                .ok_or("schema composition must be an array")?
            {
                *child = normalize(child, depth + 1)?;
            }
        }
    }
    Ok(map.into())
}

pub fn convert(schema: &Value) -> Result<Value> {
    if serde_json::to_vec(schema).map_err(|e| e.to_string())?.len() > 40_000 {
        return Err("tool parameter schema exceeds the 10K-token conservative item limit".into());
    }
    let normalized = normalize(schema, 0)?;
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&normalized)
        .map_err(|_| "tool parameter schema is invalid".to_owned())?;
    Ok(normalized)
}

pub fn check(schema: &Value) -> Result<Value> {
    convert(schema)?;
    Ok(schema.clone())
}

pub fn validate(value: &Value, schema: &Value, label: &str) -> Result<Value> {
    let schema = convert(schema)?;
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .map_err(|_| "tool parameter schema is invalid")?;
    let details = validator
        .iter_errors(value)
        .take(4)
        .map(|error| {
            use jsonschema::error::ValidationErrorKind;
            match &error.kind {
                ValidationErrorKind::Required { property } => {
                    format!("required property {property}")
                }
                ValidationErrorKind::FalseSchema => "boolean schema is false".into(),
                ValidationErrorKind::MinLength { limit } => {
                    format!("fewer than {limit} characters")
                }
                ValidationErrorKind::Not { .. } => "must NOT be valid".into(),
                _ => format!("invalid value at {}", error.instance_path),
            }
        })
        .collect::<Vec<_>>();
    if !details.is_empty() {
        return Err(format!(
            "{label} violate the declared schema: {}",
            details.join("; ")
        ));
    }
    Ok(value.clone())
}
