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
    "dependentRequired",
    "else",
    "encrypted",
    "enum",
    "exclusiveMaximum",
    "exclusiveMinimum",
    "if",
    "items",
    "maxItems",
    "maxLength",
    "maximum",
    "minItems",
    "minLength",
    "minimum",
    "not",
    "oneOf",
    "pattern",
    "properties",
    "required",
    "then",
    "title",
    "type",
    "uniqueItems",
];

const MAX_PATTERN_BYTES: usize = 1024;
const MAX_PATTERN_BOUND: u64 = 40_000;
const PATTERN_COMPILE_BYTES: usize = 1024 * 1024;

fn child_path(path: &str, segment: &str) -> String {
    format!("{path}/{}", segment.replace('~', "~0").replace('/', "~1"))
}

fn bounded_pattern(map: &serde_json::Map<String, Value>, path: &str) -> Result<()> {
    let pattern_path = child_path(path, "pattern");
    let invalid =
        |reason: &str| format!("unsupported JSON Schema pattern at {pattern_path}: {reason}");
    let pattern = map["pattern"]
        .as_str()
        .ok_or_else(|| invalid("pattern must be a string"))?;
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(invalid("pattern exceeds the 1024-byte limit"));
    }
    let hir = regex_syntax::ParserBuilder::new()
        .nest_limit(48)
        .build()
        .parse(pattern)
        .map_err(|_| {
            invalid("pattern is invalid or outside the linear regular-expression subset")
        })?;
    let bounded_input = map
        .get("maxLength")
        .and_then(Value::as_u64)
        .is_some_and(|maximum| maximum <= MAX_PATTERN_BOUND);
    let bounded_match = hir
        .properties()
        .maximum_len()
        .is_some_and(|maximum| maximum <= MAX_PATTERN_BOUND as usize);
    if !bounded_input && !bounded_match {
        return Err(invalid("unbounded regular expressions require maxLength or a finite matching bound at most 40000"));
    }
    // The library may report an empty schema path on a regex compilation limit.
    // Compile at the declaration node so that every pattern refusal retains its
    // actual field identity; use the identical engine and resource limits.
    compile_validator(&serde_json::json!({"type":"string","pattern":pattern}))
        .map_err(|_| invalid("pattern cannot compile within the linear engine resource limits"))?;
    Ok(())
}

pub(crate) fn compile_validator(schema: &Value) -> Result<jsonschema::Validator> {
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .with_pattern_options(
            jsonschema::PatternOptions::regex()
                .size_limit(PATTERN_COMPILE_BYTES)
                .dfa_size_limit(PATTERN_COMPILE_BYTES),
        )
        .build(schema)
        .map_err(|error| format!("tool parameter schema is invalid at {}", error.schema_path))
}

fn normalize(schema: &Value, depth: usize, path: &str) -> Result<Value> {
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
            bounded_pattern(&map, path)?;
        }
        if !KEYWORDS.contains(&key.as_str()) {
            return Err(format!(
                "unsupported JSON Schema keyword at {}",
                child_path(path, key)
            ));
        }
        if key == "encrypted" && !value.is_boolean() {
            return Err("JSON Schema encrypted annotation must be boolean".into());
        }
        if key == "$ref" && !value.as_str().is_some_and(|s| s.starts_with("#/$defs/")) {
            return Err("only local $defs JSON Schema references are supported".into());
        }
    }
    map.remove("encrypted");
    for key in ["items", "additionalProperties", "not", "if", "then", "else"] {
        if let Some(value) = map.get_mut(key) {
            *value = normalize(value, depth + 1, &child_path(path, key))?;
        }
    }
    for key in ["properties", "$defs"] {
        if let Some(value) = map.get_mut(key) {
            for (name, child) in value
                .as_object_mut()
                .ok_or("schema properties must be an object")?
                .iter_mut()
            {
                *child = normalize(child, depth + 1, &child_path(&child_path(path, key), name))?;
            }
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(value) = map.get_mut(key) {
            for (index, child) in value
                .as_array_mut()
                .ok_or("schema composition must be an array")?
                .iter_mut()
                .enumerate()
            {
                *child = normalize(
                    child,
                    depth + 1,
                    &child_path(&child_path(path, key), &index.to_string()),
                )?;
            }
        }
    }
    Ok(map.into())
}

pub fn convert(schema: &Value) -> Result<Value> {
    if serde_json::to_vec(schema).map_err(|e| e.to_string())?.len() > 40_000 {
        return Err("tool parameter schema exceeds the 40000-byte schema limit".into());
    }
    let normalized = normalize(schema, 0, "")?;
    compile_validator(&normalized)?;
    Ok(normalized)
}

pub fn check(schema: &Value) -> Result<Value> {
    convert(schema)?;
    Ok(schema.clone())
}

pub fn validate(value: &Value, schema: &Value, label: &str) -> Result<Value> {
    let schema = convert(schema)?;
    let validator = compile_validator(&schema)?;
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
