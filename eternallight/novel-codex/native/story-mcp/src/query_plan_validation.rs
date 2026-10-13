use jsonschema::error::ValidationErrorKind;
use jsonschema::paths::LocationSegment;
use serde_json::{Value, json};

const FIELDS: &[&str] = &[
    "question",
    "byteBudget",
    "steps",
    "id",
    "operation",
    "type",
    "query",
    "limit",
    "cursor",
    "maxChars",
    "name",
    "entityId",
    "perspective",
    "predicate",
    "ownerId",
    "readingChapter",
    "worldTime",
    "epistemicTime",
    "relationType",
    "direction",
    "maxHops",
    "ranking",
    "baseReportHash",
    "judgments",
    "resultId",
    "score",
    "rationale",
];

fn feedback(path: &str, expected: &str) -> String {
    let result = json!({"code":"invalid_query_plan","path":path,"expected":expected}).to_string();
    if result.len() <= 768 {
        result
    } else {
        json!({"code":"invalid_query_plan","path":"plan","expected":"match the declared query contract"}).to_string()
    }
}

fn describe(error: &jsonschema::ValidationError<'_>, schema: &Value, prefix: &str) -> String {
    let mut path = prefix.to_owned();
    for segment in &error.instance_path {
        match segment {
            LocationSegment::Property(field) if FIELDS.contains(&field.as_ref()) => {
                path.push('.');
                path.push_str(&field);
            }
            LocationSegment::Index(index) if index <= 64 => path.push_str(&format!("[{index}]")),
            LocationSegment::Property(_) | LocationSegment::Index(_) => break,
        }
    }
    let keyword_path = error.schema_path.as_str();
    let keyword = keyword_path.rsplit('/').next().unwrap_or_default();
    let constraint = schema.pointer(keyword_path).unwrap_or(&Value::Null);
    let expected = if let ValidationErrorKind::Required { property } = &error.kind
        && let Some(field) = property.as_str().filter(|field| FIELDS.contains(field))
    {
        path.push('.');
        path.push_str(field);
        "required field".to_owned()
    } else {
        match keyword {
            "type" => format!("type {constraint}"),
            "enum" | "const" => format!("allowed value {constraint}"),
            "minimum" | "maximum" | "minLength" | "maxLength" | "minItems" | "maxItems" => {
                format!("{keyword} {constraint}")
            }
            "additionalProperties" => "only the declared fields; remove unknown fields".to_owned(),
            "oneOf" => "one supported operation or ranking shape".to_owned(),
            _ => "match the declared query contract".to_owned(),
        }
    };
    feedback(&path, &expected)
}

pub(super) fn validate(plan: &Value, operation: &str, field: &str) -> Result<(), String> {
    let schema = super::query_plan_schema::query_plan_schema(operation);
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .map_err(|_| "query validator configuration is invalid".to_owned())?;
    if let Err(error) = validator.validate(plan) {
        // Select the declared discriminator branch only for a more useful diagnostic.
        // The full schema above is always the execution gate, including unknown fields.
        if matches!(error.kind, ValidationErrorKind::OneOfNotValid { .. })
            && let Some(value) = plan.pointer(error.instance_path.as_str())
            && let Some(branch) = schema
                .pointer(error.schema_path.as_str())
                .and_then(Value::as_array)
                .and_then(|branches| {
                    branches.iter().find(|branch| {
                        branch["properties"]["type"]["const"].as_str().is_some()
                            && branch["properties"]["type"]["const"] == value["type"]
                    })
                })
        {
            let branch_validator = jsonschema::options()
                .with_draft(jsonschema::Draft::Draft202012)
                .build(branch)
                .map_err(|_| "query validator configuration is invalid".to_owned())?;
            if let Err(branch_error) = branch_validator.validate(value) {
                let parent = describe(&error, &schema, field);
                let parent: Value =
                    serde_json::from_str(&parent).map_err(|_| "query diagnostic is invalid")?;
                return Err(describe(
                    &branch_error,
                    branch,
                    parent["path"].as_str().unwrap_or(field),
                ));
            }
        }
        return Err(describe(&error, &schema, field));
    }
    super::query_plan_schema::validate_utf8_bounds(plan).map_err(|error| {
        let (path, expected) = error
            .split_once(" exceeds ")
            .unwrap_or(("plan", "UTF-8 byte limit"));
        feedback(
            &path.replacen("plan", field, /*count*/ 1),
            &format!("at most {expected}"),
        )
    })
}
