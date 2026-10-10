use crate::{schema, Result};
use jsonschema::error::ValidationErrorKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PREFIX: &str = "invalid_tool_arguments; toolValidation=";
const MAX_MESSAGE_BYTES: usize = 400;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum Reason {
    Required,
    Type,
    Enum,
    AdditionalProperties,
    Constraint,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Diagnostic {
    tool: String,
    reason: Reason,
    schema_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    missing_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_fields: Option<Vec<String>>,
}

fn identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
}

fn schema_path(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-/$~".contains(&byte))
}

// Report the concrete failure in a uniquely discriminated union branch. Keep
// ambiguous unions opaque; never relax validation or echo rejected values.
fn specific_error<'a, 'b>(
    error: &'a jsonschema::ValidationError<'b>,
    declaration: &Value,
    depth: usize,
) -> &'a jsonschema::ValidationError<'b> {
    if depth >= 8 {
        return error;
    }
    let ValidationErrorKind::AnyOf { context } = &error.kind else {
        return error;
    };
    let Some(branches) = declaration
        .pointer(&error.schema_path.to_string())
        .and_then(Value::as_array)
    else {
        return error;
    };
    let mut matches = branches.iter().enumerate().filter(|(_, branch)| {
        let Some(properties) = branch["properties"].as_object() else {
            return false;
        };
        let mut tagged = false;
        for (field, schema) in properties {
            let Some(value) = error.instance.get(field) else {
                continue;
            };
            if let Some(values) = schema["enum"].as_array() {
                tagged = true;
                if !values.contains(value) {
                    return false;
                }
            }
            if let Some(expected) = schema.get("const") {
                tagged = true;
                if expected != value {
                    return false;
                }
            }
        }
        tagged
    });
    let Some((index, _)) = matches.next() else {
        return error;
    };
    if matches.next().is_some() {
        return error;
    }
    context
        .get(index)
        .and_then(|errors| errors.first())
        .map_or(error, |inner| specific_error(inner, declaration, depth + 1))
}

// This is called only after the executable tool was found in the request's registry.
// Schema locations come from the declaration, never the caller's unknown keys or values.
pub(crate) fn validate(value: &Value, declaration: &Value, declared_name: &str) -> Result<Value> {
    let declaration = schema::convert(declaration)?;
    let validator = schema::compile_validator(&declaration)?;
    if let Err(error) = validator.validate(value) {
        let error = specific_error(&error, &declaration, 0);
        let (reason, missing_field) = match &error.kind {
            ValidationErrorKind::Required { property } => (
                Reason::Required,
                property
                    .as_str()
                    .filter(|name| identifier(name, /*maximum*/ 48))
                    .map(str::to_owned),
            ),
            ValidationErrorKind::Type { .. } => (Reason::Type, None),
            ValidationErrorKind::Enum { .. } => (Reason::Enum, None),
            ValidationErrorKind::AdditionalProperties { .. } => {
                (Reason::AdditionalProperties, None)
            }
            _ => (Reason::Constraint, None),
        };
        let path = error.schema_path.to_string();
        let required_fields = if matches!(reason, Reason::Required) {
            declaration
                .pointer(&path)
                .and_then(Value::as_array)
                .filter(|fields| !fields.is_empty() && fields.len() <= 8)
                .and_then(|fields| {
                    fields
                        .iter()
                        .map(|field| {
                            field
                                .as_str()
                                .filter(|name| identifier(name, 32))
                                .map(str::to_owned)
                        })
                        .collect::<Option<Vec<_>>>()
                })
        } else {
            None
        };
        let mut diagnostic = Diagnostic {
            tool: if identifier(declared_name, /*maximum*/ 80) {
                declared_name
            } else {
                "unknown"
            }
            .into(),
            reason,
            schema_path: if schema_path(&path) {
                path
            } else {
                "unknown".into()
            },
            missing_field,
            required_fields,
        };
        let mut message = format!(
            "{PREFIX}{}",
            serde_json::to_string(&diagnostic).map_err(|_| "invalid tool diagnostic")?
        );
        if message.len() > MAX_MESSAGE_BYTES {
            diagnostic.required_fields = None;
            message = format!(
                "{PREFIX}{}",
                serde_json::to_string(&diagnostic).map_err(|_| "invalid tool diagnostic")?
            );
        }
        // The entire message fits the existing observer's 500-character envelope.
        if message.len() > MAX_MESSAGE_BYTES {
            return Err("invalid_tool_arguments".into());
        }
        return Err(message);
    }
    Ok(value.clone())
}

// Accept only our current canonical, bounded diagnostic at local codec error sites.
// Provider HTTP/error frames never enter this path.
pub(crate) fn diagnostic_message(message: &str) -> Option<String> {
    if message.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    let diagnostic: Diagnostic = serde_json::from_str(message.strip_prefix(PREFIX)?).ok()?;
    if !identifier(&diagnostic.tool, /*maximum*/ 80)
        || !schema_path(&diagnostic.schema_path)
        || diagnostic
            .missing_field
            .as_deref()
            .is_some_and(|field| !identifier(field, /*maximum*/ 48))
        || diagnostic.required_fields.as_ref().is_some_and(|fields| {
            fields.is_empty()
                || fields.len() > 8
                || fields.iter().any(|field| !identifier(field, 32))
        })
    {
        return None;
    }
    let canonical = format!("{PREFIX}{}", serde_json::to_string(&diagnostic).ok()?);
    (canonical.len() <= MAX_MESSAGE_BYTES).then_some(canonical)
}

#[cfg(test)]
#[path = "tool_validation_tests.rs"]
mod tests;
