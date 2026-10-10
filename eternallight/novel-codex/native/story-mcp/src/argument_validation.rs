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
// jsonschema reports local-reference errors at the call site's /$ref path,
// rather than at the referenced $defs path. Traverse only the static contract;
// the finite token budget also bounds repeated or circular reference locations.
fn schema_at_path<'a>(schema: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(schema);
    }
    if path.len() > 2048 {
        return None;
    }
    let mut current = schema;
    for (index, token) in path.strip_prefix('/')?.split('/').enumerate() {
        if index >= 64 {
            return None;
        }
        let field = token.replace("~1", "/").replace("~0", "~");
        current = if field == "$ref" {
            schema.pointer(current["$ref"].as_str()?.strip_prefix('#')?)?
        } else if current.is_array() {
            current.get(field.parse::<usize>().ok()?)?
        } else {
            current.get(&field)?
        };
    }
    Some(current)
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
    let Some(branches) =
        schema_at_path(schema, &error.schema_path.to_string()).and_then(Value::as_array)
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

// Locate only declared fields and genuine array positions. Arbitrary property
// names and rejected values must never become model-visible diagnostic paths.
fn declared_child<'a>(
    definition: &'a Value,
    root: &'a Value,
    field: &str,
    array: bool,
    depth: usize,
    found: &mut Vec<&'a Value>,
) {
    if depth >= 16 || found.len() >= 64 {
        return;
    }
    if let Some(child) = if array {
        definition.get("items")
    } else {
        definition["properties"].get(field)
    } {
        found.push(child);
    }
    if let Some(reference) = definition["$ref"]
        .as_str()
        .and_then(|r| r.strip_prefix('#'))
    {
        if let Some(target) = root.pointer(reference) {
            declared_child(target, root, field, array, depth + 1, found);
        }
    }
    for composition in ["anyOf", "oneOf", "allOf"] {
        for branch in definition[composition].as_array().into_iter().flatten() {
            declared_child(branch, root, field, array, depth + 1, found);
        }
    }
}

fn declared_instance_path(path: &str, value: &Value, schema: &Value) -> Option<String> {
    if path.len() > 160 {
        return None;
    }
    if path.is_empty() {
        return Some(String::new());
    }
    let mut definitions = vec![schema];
    let mut instance = value;
    for token in path.strip_prefix('/')?.split('/') {
        let field = token.replace("~1", "/").replace("~0", "~");
        let array = instance.is_array();
        instance = if array {
            instance.get(field.parse::<usize>().ok()?)?
        } else {
            instance.get(&field)?
        };
        let mut next = Vec::new();
        for definition in definitions {
            declared_child(definition, schema, &field, array, 0, &mut next);
        }
        if next.is_empty() {
            return None;
        }
        definitions = next;
    }
    Some(path.to_owned())
}

fn safe_property_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    !name.is_empty()
        && name.len() <= 64
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

// Field lists come from this exact compiled schema, never from argument values.
// Bound their serialized size so useful correction guidance survives the
// existing 1600-byte diagnostic envelope.
fn expected_fields(definition: &Value) -> Option<(Vec<&str>, usize)> {
    let properties = definition["properties"].as_object()?;
    let mut names = Vec::new();
    for name in properties.keys() {
        if names.len() >= 24 || name.len() > 80 {
            continue;
        }
        names.push(name.as_str());
        if json!(names).to_string().len() > 512 {
            names.pop();
        }
    }
    let omitted = properties.len() - names.len();
    Some((names, omitted))
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
        if feedback.to_string().len() > 1600 {
            feedback
                .as_object_mut()
                .expect("diagnostic")
                .remove("fieldInstruction");
        }
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
                "instruction":"No business request was sent. instancePath locates the affected object; an empty path means root arguments. Use missingField for an omitted field, unknownFields for safe extra field names, and expectedFields from the exact schema. Correct in this task; preserve author scope, task binding and all other valid fields."});
            if let Some(location) =
                declared_instance_path(&error.instance_path.to_string(), value, schema)
            {
                feedback["instancePath"] = json!(location);
            }
            // Return constructor-owned field guidance, not a guess at model intent.
            if let Some((definition, _)) = path.rsplit_once('/') {
                if let Some(definition) = schema_at_path(schema, definition) {
                    if let Some(hint) = definition["description"]
                        .as_str()
                        .filter(|s| s.len() <= 600)
                    {
                        feedback["fieldInstruction"] = json!(hint);
                    }
                    if matches!(reason, "required" | "additionalProperties") {
                        if let Some((fields, omitted)) = expected_fields(definition) {
                            feedback["expectedFields"] = json!(fields);
                            if omitted > 0 {
                                feedback["expectedFieldsOmitted"] = json!(omitted);
                            }
                        }
                    }
                }
            }
            if let ValidationErrorKind::AdditionalProperties { unexpected } = &error.kind {
                let mut fields = Vec::new();
                for name in unexpected.iter().filter(|name| safe_property_name(name)) {
                    if fields.len() >= 4 {
                        break;
                    }
                    fields.push(name);
                    if json!(fields).to_string().len() > 256 {
                        fields.pop();
                    }
                }
                feedback["unknownFields"] = json!(fields);
                let omitted = unexpected.len() - fields.len();
                if omitted > 0 {
                    feedback["unknownFieldsOmitted"] = json!(omitted);
                }
            }
            if reason == "required" {
                feedback["requiredFields"] = schema_at_path(schema, &path)
                    .cloned()
                    .unwrap_or(Value::Null);
                if let ValidationErrorKind::Required { property } = &error.kind {
                    if feedback["requiredFields"]
                        .as_array()
                        .is_some_and(|fields| fields.contains(property))
                    {
                        feedback["missingField"] = property.clone();
                    }
                }
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
    fn identifies_the_omitted_field_in_a_later_array_record_and_accepts_correction() {
        let schema = crate::tool_specs()
            .into_iter()
            .find(|t| t["name"] == "publish_knowledge_batch")
            .unwrap()["inputSchema"]
            .clone();
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":0,
        "batchId":"batch","entities":[],"assertions":[
            {"kind":"assertion","subjectId":"person","predicate":"name","value":"private-value","evidence":{"quote":"source"}},
            {"kind":"relation","subjectId":"person","predicate":"knows","targetId":"other","evidence":{"quote":"source"}}
        ]});
        let validation = ArgumentValidation::default();
        let diagnostic: Value = serde_json::from_str(
            &validation
                .validate("publish_knowledge_batch", &args, &schema)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(diagnostic["missingField"], "value");
        assert_eq!(diagnostic["instancePath"], "/assertions/1");
        assert!(!diagnostic.to_string().contains("private-value"));
        args["assertions"][1]["value"] = json!("a sourced relation");
        validation
            .validate("publish_knowledge_batch", &args, &schema)
            .unwrap();
    }

    #[test]
    fn diagnostic_locations_follow_references_and_never_echo_unknown_properties() {
        let schema = json!({"type":"object","properties":{"rows":{"type":"array","items":{"$ref":"#/$defs/row"}}},
            "$defs":{"row":{"type":"object","additionalProperties":false,"properties":{"value":{"type":"integer"}}}}});
        let value = json!({"rows":[{"value":"private-value","private-key":false}]});
        assert_eq!(
            declared_instance_path("/rows/0/value", &value, &schema).as_deref(),
            Some("/rows/0/value")
        );
        assert_eq!(
            declared_instance_path("/rows/0/private-key", &value, &schema),
            None
        );
        let diagnostic = ArgumentValidation::default()
            .validate("save", &value, &schema)
            .unwrap_err();
        assert!(!diagnostic.contains("private-key"));
        assert!(!diagnostic.contains("private-value"));
    }

    #[test]
    fn extra_root_field_is_identified_without_removing_legal_chapter_acceptance() {
        let schema = crate::tool_specs()
            .into_iter()
            .find(|tool| tool["name"] == "request_work_action")
            .unwrap()["inputSchema"]
            .clone();
        let acceptance = json!({"requirements":[{"id":"saved-chapter","text":"保存本次授权的章节"}],"length":{
            "requirement":"作者没有指定篇幅限制","minCharacters":null,"maxCharacters":null}});
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":0,
            "expectedWorkVersion":1,"expectedSourceDigest":"digest","actionKey":"available-action",
            "instruction":"按授权保存正文","idempotencyKey":"one-action","chapterAcceptance":acceptance,
            "operation":"PRIVATE_REJECTED_VALUE"});
        let validation = ArgumentValidation::default();
        let diagnostic: Value = serde_json::from_str(
            &validation
                .validate("request_work_action", &args, &schema)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(diagnostic["reason"], "additionalProperties");
        assert_eq!(diagnostic["instancePath"], "");
        assert_eq!(diagnostic["unknownFields"], json!(["operation"]));
        assert!(
            diagnostic["expectedFields"]
                .as_array()
                .unwrap()
                .contains(&json!("chapterAcceptance"))
        );
        assert!(
            diagnostic["expectedFields"]
                .as_array()
                .unwrap()
                .contains(&json!("chapterReview"))
        );
        assert!(!diagnostic.to_string().contains("PRIVATE_REJECTED_VALUE"));
        args.as_object_mut().unwrap().remove("operation");
        validation
            .validate("request_work_action", &args, &schema)
            .unwrap();
        let (_, _, body) = crate::task_source_tools::request("request_work_action", &args).unwrap();
        assert_eq!(
            body.unwrap()["chapterAcceptance"],
            acceptance,
            "The current schema and semantic request preserve this legal field unchanged"
        );
    }

    #[test]
    fn extra_nested_fields_report_the_declared_object_path_and_hide_prose_keys_and_values() {
        let schema = json!({"type":"object","properties":{"rows":{"type":"array","items":{"$ref":"#/$defs/row"}}},
            "$defs":{"row":{"type":"object","additionalProperties":false,"properties":{"value":{"type":"integer"}},"required":["value"]}}});
        // validate() returns the first unknown property, not every unknown key.
        // Exercise each error independently so redaction cannot hide the safe
        // typo case or depend on the map's property iteration order.
        for (field, private_value, expected) in [
            (
                "typodField".to_owned(),
                "PRIVATE_REJECTED_VALUE",
                json!(["typodField"]),
            ),
            (
                "私密正文作为键".to_owned(),
                "PRIVATE_SECRET_VALUE",
                json!([]),
            ),
            ("A".repeat(65), "PRIVATE_LONG_KEY_VALUE", json!([])),
        ] {
            let mut row = json!({"value":1});
            row.as_object_mut()
                .unwrap()
                .insert(field.clone(), json!(private_value));
            let args = json!({"rows":[{"value":0},row]});
            let diagnostic: Value = serde_json::from_str(
                &ArgumentValidation::default()
                    .validate("save", &args, &schema)
                    .unwrap_err(),
            )
            .unwrap();
            assert_eq!(diagnostic["instancePath"], "/rows/1");
            assert_eq!(
                diagnostic["schemaPath"],
                "/properties/rows/items/$ref/additionalProperties"
            );
            assert_eq!(diagnostic["expectedFields"], json!(["value"]));
            assert_eq!(diagnostic["unknownFields"], expected);
            let serialized = diagnostic.to_string();
            assert!(!serialized.contains(private_value));
            if expected.as_array().unwrap().is_empty() {
                assert_eq!(diagnostic["unknownFieldsOmitted"], 1);
                assert!(!serialized.contains(&field));
            } else {
                assert!(diagnostic.get("unknownFieldsOmitted").is_none());
            }
        }

        let missing: Value = serde_json::from_str(
            &ArgumentValidation::default()
                .validate("save", &json!({"rows":[{"value":0},{}]}), &schema)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(missing["instancePath"], "/rows/1");
        assert_eq!(
            missing["schemaPath"],
            "/properties/rows/items/$ref/required"
        );
        assert_eq!(missing["missingField"], "value");
        assert_eq!(missing["requiredFields"], json!(["value"]));
        assert_eq!(missing["expectedFields"], json!(["value"]));
    }

    #[test]
    fn bounded_field_metadata_survives_large_contracts_and_preserves_required_feedback() {
        let mut properties = serde_json::Map::new();
        for index in 0..40 {
            properties.insert(
                format!("field_{index:02}_{}", "x".repeat(35)),
                json!({"type":"integer"}),
            );
        }
        let schema = json!({"type":"object","additionalProperties":false,"properties":properties});
        let diagnostic = ArgumentValidation::default()
            .validate("save", &json!({"typo":"PRIVATE_VALUE"}), &schema)
            .unwrap_err();
        assert!(diagnostic.len() <= 1600);
        let diagnostic: Value = serde_json::from_str(&diagnostic).unwrap();
        assert_eq!(diagnostic["unknownFields"], json!(["typo"]));
        assert!(diagnostic["expectedFields"].as_array().unwrap().len() < 40);
        assert!(diagnostic["expectedFieldsOmitted"].as_u64().unwrap() > 0);
        assert!(!diagnostic.to_string().contains("PRIVATE_VALUE"));

        let required =
            json!({"type":"object","properties":{"value":{"type":"integer"}},"required":["value"]});
        let missing: Value = serde_json::from_str(
            &ArgumentValidation::default()
                .validate("save", &json!({}), &required)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(missing["missingField"], "value");
        assert_eq!(missing["requiredFields"], json!(["value"]));
        assert_eq!(missing["expectedFields"], json!(["value"]));
        for (schema, input, reason) in [
            (
                json!({"type":"object","properties":{"value":{"type":"integer"}}}),
                json!({"value":"PRIVATE_TYPE_VALUE"}),
                "type",
            ),
            (
                json!({"type":"object","properties":{"value":{"enum":["allowed"]}}}),
                json!({"value":"PRIVATE_ENUM_VALUE"}),
                "enum",
            ),
        ] {
            let diagnostic: Value = serde_json::from_str(
                &ArgumentValidation::default()
                    .validate("save", &input, &schema)
                    .unwrap_err(),
            )
            .unwrap();
            assert_eq!(diagnostic["reason"], reason);
            assert_eq!(diagnostic["instancePath"], "/value");
            assert!(!diagnostic.to_string().contains("PRIVATE_"));
        }
    }

    #[test]
    fn extra_fields_in_a_selected_union_branch_keep_precise_scope() {
        let schema = json!({"type":"object","properties":{"rows":{"type":"array","items":{"$ref":"#/$defs/row"}}},"$defs":{"row":{"anyOf":[
            {"type":"object","additionalProperties":false,"properties":{"kind":{"enum":["state"]},"value":{"type":"integer"}},"required":["kind","value"]},
            {"type":"object","additionalProperties":false,"properties":{"kind":{"enum":["event"]},"time":{"type":"integer"}},"required":["kind","time"]}
        ]}}});
        let input = json!({"rows":[{"kind":"state","value":1,"storyTime":"PRIVATE_VALUE"}]});
        let diagnostic: Value = serde_json::from_str(
            &ArgumentValidation::default()
                .validate("save", &input, &schema)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(diagnostic["reason"], "additionalProperties");
        assert_eq!(diagnostic["instancePath"], "/rows/0");
        assert_eq!(diagnostic["unknownFields"], json!(["storyTime"]));
        assert_eq!(diagnostic["expectedFields"], json!(["kind", "value"]));
        assert!(!diagnostic.to_string().contains("PRIVATE_VALUE"));
    }

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
