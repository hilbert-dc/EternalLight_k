use jsonschema::{error::ValidationErrorKind, paths::LocationSegment};
use serde_json::{Value, json};

// Keep the writer's payload contract separate from the shared tool envelope.
// Additional metadata is preserved; Runtime owns target, CAS and protection checks.
pub(super) fn payload() -> Value {
    json!({
        "type":"object", "additionalProperties":true,
        "required":["chapterId","chapterNumber","title","completeness"],
        "oneOf":[
            {"required":["body"],"not":{"required":["sceneGroups"]}},
            {"required":["sceneGroups"],"not":{"required":["body"]}}
        ],
        "properties":{
            "chapterId":{"type":"string","minLength":1,"maxLength":160,
                "description":"Copy the frozen task target chapterId exactly."},
            "chapterNumber":{"type":"integer","minimum":1},
            "title":{"type":"string","maxLength":160},
            "body":{"type":"string","maxLength":120000},
            "completeness":{"type":"string",
                "description":"Use chapterComplete when the author-requested writing within the authorized chapter scope is finished, including an explicitly requested short opening. This does not authorize extra length or scope. Intermediate checkpoints retain their actual completeness and cannot complete the writer task."},
            "sceneGroups":{"type":"array","maxItems":100,
                "description":"For a whole chapter bound to a chapter plan, chapterComplete requires explicit groups: at least one nonempty group copies a bound sceneId, and every non-null sceneId belongs to that plan. Other checkpoints and unbound chapters may be body-only. Preserve stable group IDs and protected metadata. Submit sceneGroups only, without a duplicate body; MCP derives the canonical full body from trimmed group bodies joined with \\n\\n---\\n\\n. This mapping does not certify effects or require every planned scene to appear.",
                "items":{"type":"object","additionalProperties":true,
                    "required":["id","body","locked"],
                    "properties":{
                        "id":{"type":"string","minLength":1},
                        "body":{"type":"string"}, "locked":{"type":"boolean"},
                        "sceneId":{"type":["string","null"],"description":"Copy the exact bound plan scene id for the prose actually assigned to that scene. Null denotes prose not assigned to a planned scene; never infer a scene id from group order or title."}
                    }}
            }
        }
    })
}

pub(super) fn transport_payload() -> Value {
    json!({"type":"object","description":format!(
        "MCP validates before dispatch; invalid_draft_payload gives path/expected for correction within this task budget. Send the complete chapter: body OR sceneGroups. MCP derives body from groups and preserves metadata. {}", payload())})
}

fn feedback(path: &str, expected: &str) -> String {
    let value = json!({"code":"invalid_draft_payload","path":path,"expected":expected}).to_string();
    if value.len() <= 768 {
        value
    } else {
        json!({"code":"invalid_draft_payload","path":"payload","expected":"match the DraftPayload contract"}).to_string()
    }
}

pub(super) fn runtime_payload(value: &Value) -> Result<Value, String> {
    if let Some(payload) = value.as_object()
        && payload.contains_key("body") == payload.contains_key("sceneGroups")
    {
        return Err(feedback(
            "payload",
            "provide exactly one of body or sceneGroups",
        ));
    }
    let schema = payload();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .map_err(|_| feedback("payload", "validator configuration is invalid"))?;
    if let Err(error) = validator.validate(value) {
        const FIELDS: &[&str] = &[
            "chapterId",
            "chapterNumber",
            "title",
            "body",
            "completeness",
            "sceneGroups",
            "id",
            "locked",
            "sceneId",
        ];
        let mut path = "payload".to_owned();
        for segment in &error.instance_path {
            match segment {
                LocationSegment::Property(field) if FIELDS.contains(&field.as_ref()) => {
                    path.push('.');
                    path.push_str(&field);
                }
                LocationSegment::Index(index) if index < 100 => {
                    path.push_str(&format!("[{index}]"))
                }
                LocationSegment::Property(_) | LocationSegment::Index(_) => break,
            }
        }
        if let ValidationErrorKind::Required { property } = &error.kind
            && let Some(field) = property.as_str().filter(|field| FIELDS.contains(field))
        {
            return Err(feedback(&format!("{path}.{field}"), "required field"));
        }
        let keyword = error
            .schema_path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let constraint = schema
            .pointer(error.schema_path.as_str())
            .unwrap_or(&Value::Null);
        let expected = match keyword {
            "type" => format!("type {constraint}"),
            "minimum" | "minLength" | "maxLength" | "maxItems" => format!("{keyword} {constraint}"),
            _ => "match the DraftPayload contract".to_owned(),
        };
        return Err(feedback(&path, &expected));
    }
    if let Some(groups) = value["sceneGroups"].as_array() {
        let mut ids = std::collections::BTreeSet::new();
        for (index, group) in groups.iter().enumerate() {
            if !ids.insert(group["id"].as_str().expect("schema-validated group ID")) {
                return Err(feedback(
                    &format!("payload.sceneGroups[{index}].id"),
                    "unique group ID",
                ));
            }
        }
    }
    let mut canonical = value.clone();
    if let Some(groups) = value["sceneGroups"].as_array() {
        let body = crate::scene_body::canonical_scene_body(groups.iter().map(|group| {
            group["body"].as_str().expect("schema-validated group body")
        }));
        if body.chars().count() > 120_000 {
            return Err(feedback("payload.body", "maxLength 120000"));
        }
        canonical["body"] = json!(body);
    }
    Ok(canonical)
}

/// Exact edits refer to immutable task input; Native preserves all other fields.
pub(super) fn patch() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["chapterId","sourceRevisionId","edits"],"properties":{
        "chapterId":{"type":"string","minLength":1,"maxLength":160,"description":"Copy the frozen target chapterId."},
        "sourceRevisionId":{"type":"string","minLength":1,"maxLength":160,"description":"Exact frozen draft baseline or this task's saved checkpoint; copy matching candidate CAS. Read its original groups before editing."},
        "edits":{"type":"array","minItems":1,"maxItems":100,"items":{"type":"object","additionalProperties":false,"required":["groupId","start","end","oldText","newText"],"properties":{
            "groupId":{"type":"string","minLength":1,"maxLength":160},
            "start":{"type":"integer","minimum":0,"description":"Unicode scalar offset in original group body, inclusive; not UTF-8 bytes or UTF-16 units."},
            "end":{"type":"integer","minimum":0,"description":"Exclusive Unicode scalar offset. Equal start/end inserts text."},
            "oldText":{"type":"string","maxLength":120000,"description":"Exact original text in [start,end); empty for insertion."},
            "newText":{"type":"string","maxLength":120000}
        }},"description":"Non-overlapping ranges in original coordinates. Metadata and unselected/locked groups remain unchanged; Native enforces the task's write scope."}
    }})
}
