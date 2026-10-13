use jsonschema::error::ValidationErrorKind;
use serde_json::{Value, json};

// The model must produce the same editable OutlinePayload used by the author UI.
// Runtime remains authoritative for source, CAS, protection and persistence.
pub(super) fn outline_payload() -> Value {
    json!({"type":"object","additionalProperties":true,"required":["nodes"],
    "properties":{"nodes":{"type":"array","items":{
        "type":"object","additionalProperties":true,
        "required":["kind","position","title","purpose","summary","dependencyKeys","certainty","protection"],
        "properties":{
            "id":{"type":"string"},"clientKey":{"type":"string"},
            "parentId":{"type":["string","null"]},"parentClientKey":{"type":"string"},
            "kind":{"type":"string","enum":["book","volume","unit"]},
            "position":{"type":"number"},"title":{"type":"string"},
            "publicTitle":{"type":"string"},"purpose":{"type":"string"},
            "summary":{"type":"string"},"dependencyKeys":{"type":"array","items":{"type":"string"}},
            "certainty":{"type":"string","enum":["open","tentative","settled"]},
            "protection":{"type":"string","enum":["free","preserve-intent","preserve-arrangement"]},
            "writerGuidance":{"type":"string"}
        }
    }}}})
}

pub(super) fn validate_outline(payload: &Value) -> Result<(), String> {
    let schema = outline_payload();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .map_err(|_| "outline validator configuration is invalid".to_owned())?;
    if let Err(error) = validator.validate(payload) {
        let mut path = format!("payload{}", error.instance_path);
        if let ValidationErrorKind::Required { property } = &error.kind
            && let Some(field) = property.as_str()
        {
            path.push('/');
            path.push_str(field);
        }
        return Err(format!("{path} must match the declared outline schema"));
    }
    Ok(())
}
