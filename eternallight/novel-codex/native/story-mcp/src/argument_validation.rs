use jsonschema::error::ValidationErrorKind;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};
const LIMIT: u8 = 3;
#[derive(Default)]
pub(super) struct ArgumentValidation {
    failures: RefCell<BTreeMap<String, u8>>,
    exhausted: RefCell<Option<String>>,
}
fn terminal(tool: &str, origin: &str) -> String {
    json!({"code":"tool_argument_correction_limit","tool":tool,"originTool":origin,"limit":LIMIT})
        .to_string()
}
fn selected_error<'a, 'b>(
    error: &'a jsonschema::ValidationError<'b>,
    schema: &Value,
    depth: usize,
) -> &'a jsonschema::ValidationError<'b> {
    if depth >= 8 {
        return error;
    }
    let ValidationErrorKind::AnyOf { context } = &error.kind else {
        return error;
    };
    let Some(branches) = schema
        .pointer(&error.schema_path.to_string())
        .and_then(Value::as_array)
    else {
        return error;
    };
    let mut matching = branches.iter().enumerate().filter(|(_, branch)| {
        branch["properties"].as_object().is_some_and(|properties| {
            properties.iter().any(|(field, definition)| {
                definition["enum"].as_array().is_some_and(|values| {
                    error
                        .instance
                        .get(field)
                        .is_some_and(|value| values.contains(value))
                })
            }) && properties.iter().all(|(field, definition)| {
                definition["enum"].as_array().is_none_or(|values| {
                    error
                        .instance
                        .get(field)
                        .is_none_or(|value| values.contains(value))
                })
            })
        })
    });
    let Some((index, _)) = matching.next() else {
        return error;
    };
    if matching.next().is_some() {
        return error;
    }
    context
        .get(index)
        .and_then(|errors| errors.first())
        .map_or(error, |inner| selected_error(inner, schema, depth + 1))
}

impl ArgumentValidation {
    pub(super) fn ensure_active(&self, tool: &str) -> Result<(), String> {
        if let Some(origin) = self.exhausted.borrow().as_deref() {
            Err(terminal(tool, origin))
        } else {
            Ok(())
        }
    }
    pub(super) fn complete_preflight(&self, tool: &str) {
        self.failures.borrow_mut().remove(tool);
    }
    fn reject(&self, tool: &str, mut feedback: Value) -> String {
        let mut counts = self.failures.borrow_mut();
        let count = counts.entry(tool.to_owned()).or_default();
        *count += 1;
        if *count >= LIMIT {
            *self.exhausted.borrow_mut() = Some(tool.to_owned());
            return terminal(tool, tool);
        }
        feedback["code"] = json!("invalid_tool_arguments");
        feedback["tool"] = json!(tool);
        feedback["attempt"] = json!(*count);
        feedback["limit"] = json!(LIMIT);
        let text = feedback.to_string();
        if text.len() <= 1600 {
            text
        } else {
            json!({"code":"invalid_tool_arguments","tool":tool,"attempt":*count,"limit":LIMIT,
                "instruction":"Correct arguments against the declared tool contract. No business request was sent."}).to_string()
        }
    }
    // Only explicit local payload validators enter here, never identity, HTTP,
    // Runtime or transport errors. Hints are constructor-owned, not model text.
    pub(super) fn local<T>(
        &self,
        tool: &str,
        result: Result<T, String>,
        hint: &str,
    ) -> Result<T, String> {
        result.map_err(|error| {
            let detail = serde_json::from_str::<Value>(&error).ok();
            if matches!(error.as_str(), "chapter plan validator configuration is invalid"
                | "query validator configuration is invalid" | "brief validator configuration is invalid"
                | "outline validator configuration is invalid" | "query diagnostic is invalid")
                || detail.as_ref().is_some_and(|d| d["code"] == "invalid_draft_payload"
                    && d["path"] == "payload" && d["expected"] == "validator configuration is invalid") {
                return error;
            }
            let mut feedback = json!({"reason":"domain","instruction":hint,
                "recovery":"No business request was sent. Correct this tool in the same task; preserve author scope and binding."});
            // These validators construct bounded paths from declared fields and
            // constraints. Never forward arbitrary error strings or input values.
            if let Some(detail) = detail {
                if matches!(detail["code"].as_str(), Some("invalid_draft_payload" | "invalid_chapter_plan" | "invalid_query_plan")) {
                    feedback["detail"] = detail;
                }
            }
            self.reject(tool, feedback)
        })
    }
    pub(super) fn validate(&self, tool: &str, value: &Value, schema: &Value) -> Result<(), String> {
        self.ensure_active(tool)?;
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(schema)
            .map_err(|_| "tool schema is invalid".to_owned())?;
        if let Err(error) = validator.validate(value) {
            let error = selected_error(&error, schema, 0);
            let path = error.schema_path.to_string();
            let reason = match &error.kind {
                ValidationErrorKind::Required { .. } => "required",
                ValidationErrorKind::Type { .. } => "type",
                ValidationErrorKind::AdditionalProperties { .. } => "additionalProperties",
                ValidationErrorKind::Enum { .. } => "enum",
                _ => "constraint",
            };
            let mut feedback = json!({"reason":reason,"schemaPath":path,
                "instruction":"No business request was sent. Correct arguments against the full declared schema in the same task. Check all required fields together; preserve author scope and task binding. Do not add unlisted fields."});
            if reason == "required" {
                feedback["requiredFields"] = schema.pointer(&path).cloned().unwrap_or(Value::Null);
            }
            return Err(self.reject(tool, feedback));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nested_missing_fields_and_latches_before_any_later_valid_call() {
        let schema = crate::tool_specs()
            .into_iter()
            .find(|t| t["name"] == "publish_knowledge_batch")
            .unwrap()["inputSchema"]
            .clone();
        let validation = ArgumentValidation::default();
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":0,
            "batchId":"batch","entities":[],"assertions":[]});
        // Envelope shape is independently validated as well as the nested union.
        for field in schema["required"].as_array().unwrap() {
            args.as_object_mut()
                .unwrap()
                .entry(field.as_str().unwrap())
                .or_insert(json!("fixture"));
        }
        let valid = args.clone();
        args["assertions"] = json!([{"kind":"state","subjectId":"person","predicate":"location",
            "value":"private-value","evidence":{"quote":"人物所在"},"stateSchema":{"cardinality":"single","valueType":{"type":"text"}}}]);
        let first = validation
            .validate("publish_knowledge_batch", &args, &schema)
            .unwrap_err();
        assert!(!first.contains("private-value"));
        assert!(first.contains("invalid_tool_arguments"));
        assert!(first.contains("validFrom"));
        assert!(
            validation
                .validate("publish_knowledge_batch", &args, &schema)
                .is_err()
        );
        assert!(
            validation
                .validate("publish_knowledge_batch", &args, &schema)
                .unwrap_err()
                .contains("tool_argument_correction_limit")
        );
        assert!(
            validation
                .validate("publish_knowledge_batch", &valid, &schema)
                .unwrap_err()
                .contains("tool_argument_correction_limit")
        );
    }
}

#[cfg(test)]
mod generic_tests {
    use super::*;
    #[test]
    fn only_complete_preflight_for_the_same_tool_resets_and_exhaustion_closes_every_tool() {
        let v = ArgumentValidation::default();
        let schema = json!({"type":"object","required":["payload"]});
        assert!(
            v.validate("save_draft_checkpoint", &json!({}), &schema)
                .is_err()
        );
        v.validate("read_context", &json!({"payload":null}), &schema)
            .unwrap();
        v.complete_preflight("read_context");
        let second: Value = serde_json::from_str(
            &v.local::<()>(
                "save_draft_checkpoint",
                Err("secret-value".into()),
                "Use unique scene group IDs",
            )
            .unwrap_err(),
        )
        .unwrap();
        assert_eq!(second["attempt"], 2);
        assert!(!second.to_string().contains("secret-value"));
        // Schema success alone does not clear a domain-validation failure.
        v.validate("save_draft_checkpoint", &json!({"payload":null}), &schema)
            .unwrap();
        assert!(
            v.local::<()>(
                "save_draft_checkpoint",
                Err("bad group".into()),
                "Use unique scene group IDs"
            )
            .unwrap_err()
            .contains("tool_argument_correction_limit")
        );
        let terminal: Value =
            serde_json::from_str(&v.ensure_active("read_context").unwrap_err()).unwrap();
        assert_eq!(terminal["tool"], "read_context");
        assert_eq!(terminal["originTool"], "save_draft_checkpoint");
        v.complete_preflight("save_draft_checkpoint");
        assert!(v.ensure_active("save_brief").is_err());
    }
    #[test]
    fn successful_local_preflight_resets_only_that_tool_and_internal_errors_are_not_counted() {
        let v = ArgumentValidation::default();
        let schema = json!({"type":"object","required":["payload"]});
        v.validate("save_brief", &json!({}), &schema).unwrap_err();
        v.validate("save_brief", &json!({"payload":null}), &schema)
            .unwrap();
        v.complete_preflight("save_brief");
        assert_eq!(
            v.local::<()>(
                "save_brief",
                Err("brief validator configuration is invalid".into()),
                "unused"
            )
            .unwrap_err(),
            "brief validator configuration is invalid"
        );
        let error: Value =
            serde_json::from_str(&v.validate("save_brief", &json!({}), &schema).unwrap_err())
                .unwrap();
        assert_eq!(error["attempt"], 1);
    }
    #[test]
    fn bundled_names_match_every_compiled_tool_once() {
        let names: Vec<String> = serde_json::from_str(include_str!(
            "../../../electron/executor-validated-tools.json"
        ))
        .unwrap();
        let expected: std::collections::BTreeSet<_> = crate::tool_specs()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(names.len(), expected.len());
        assert!(names.len() <= 64);
        assert_eq!(
            names.into_iter().collect::<std::collections::BTreeSet<_>>(),
            expected
        );
    }
}
