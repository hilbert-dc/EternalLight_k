use jsonschema::error::ValidationErrorKind;
use jsonschema::paths::LocationSegment;
use serde_json::{Value, json};

pub(super) fn payload() -> Value {
    let id = json!({"type":"string","minLength":1,"maxLength":160});
    let reference = json!({"oneOf":[
        {"type":"object","additionalProperties":false,"required":["type","commitmentId","revisionId"],"properties":{"type":{"const":"commitment"},"commitmentId":id,"revisionId":id}},
        {"type":"object","additionalProperties":false,"required":["type","reportRevisionId","proposalId","sourceRevisionId"],"properties":{"type":{"const":"proposal"},"reportRevisionId":id,"proposalId":id,"sourceRevisionId":id}}
    ]});
    let effect = json!({
        "type":"object","additionalProperties":true,"required":["id","dimension","description","commitmentRefs"],
        "properties":{"id":id,"dimension":{"type":"string","enum":["world","knowledge","relationship","narrative"]},
            "description":{"type":"string","minLength":1,"maxLength":500},
            "commitmentRefs":{"type":"array","maxItems":4,"description":"At most 64 commitment references across this entire plan. Copy exact source identities; use [] when unrelated.","items":reference}}
    });
    let scene = json!({
        "type":"object","additionalProperties":true,"required":["id","goal","beats","expectedEffects"],
        "properties":{"id":id,"title":{"type":"string"},"goal":{"type":"string"},
            "conflict":{"type":"string"},"locked":{"type":"boolean"},
            "beats":{"type":"array","items":{"type":"object","additionalProperties":true,
                "required":["id","description"],"properties":{"id":id,"description":{"type":"string"},"locked":{"type":"boolean"}}}},
            "expectedEffects":{"type":"array","minItems":1,"maxItems":12,"items":effect}}
    });
    let chapter = json!({
        "type":"object","additionalProperties":true,
        "required":["chapterPlanId","chapterNumber","goal","scenes"],"properties":{
            "chapterPlanId":id,"chapterNumber":{"type":"integer","minimum":1},
            "primaryNodeId":{"type":"string"},"title":{"type":"string"},"goal":{"type":"string"},
            "conflict":{"type":"string"},"dependencyKeys":{"type":"array","items":{"type":"string"}},
            "publicTitle":{"type":"string"},"writerGuidance":{"type":"string"},
            "publicCommitments":{"type":"array","items":{"type":"string"}},
            "allowances":{"type":"array","items":{"type":"string"}},"locked":{"type":"boolean"},
            "povCharacterId":{"type":["string","null"],"minLength":1,"maxLength":160},
            "storyTime":{"type":["integer","null"]},
            "scenes":{"type":"array","minItems":1,"maxItems":24,"items":scene}}
    });
    json!({"type":"object","additionalProperties":true,"required":["chapters"],"properties":{
        "sourceOutlineRevisionId":{"type":"string","description":"Optional exact source outline revision; no formal adoption is required."},
        "window":{"type":"integer","minimum":1},
        "chapters":{"type":"array","minItems":1,"maxItems":12,"items":chapter}
    }})
}

pub(super) fn validate(value: &Value) -> Result<(), String> {
    let schema = payload();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .map_err(|_| "chapter plan validator configuration is invalid")?;
    if let Err(error) = validator.validate(value) {
        let fields = [
            "chapters",
            "chapterPlanId",
            "chapterNumber",
            "goal",
            "scenes",
            "id",
            "beats",
            "description",
            "expectedEffects",
            "dimension",
            "commitmentRefs",
            "povCharacterId",
            "storyTime",
            "type",
            "commitmentId",
            "revisionId",
            "reportRevisionId",
            "proposalId",
            "sourceRevisionId",
            "sourceOutlineRevisionId",
            "window",
            "primaryNodeId",
            "title",
            "conflict",
            "dependencyKeys",
            "publicTitle",
            "writerGuidance",
            "publicCommitments",
            "allowances",
            "locked",
        ];
        let mut path = "payload".to_owned();
        for segment in &error.instance_path {
            match segment {
                LocationSegment::Property(field) if fields.contains(&field.as_ref()) => {
                    path.push('.');
                    path.push_str(&field);
                }
                LocationSegment::Index(index) if index <= 64 => {
                    path.push_str(&format!("[{index}]"))
                }
                LocationSegment::Property(_) | LocationSegment::Index(_) => break,
            }
        }
        if let ValidationErrorKind::Required { property } = error.kind
            && let Some(field) = property.as_str().filter(|field| fields.contains(field))
        {
            path.push('.');
            path.push_str(field);
        }
        let result = json!({"code":"invalid_chapter_plan","path":path,"expected":"match the declared chapter plan schema"}).to_string();
        return Err(if result.len() <= 400 {
            result
        } else {
            "{\"code\":\"invalid_chapter_plan\",\"path\":\"payload\",\"expected\":\"match the declared chapter plan schema\"}".into()
        });
    }
    Ok(())
}
