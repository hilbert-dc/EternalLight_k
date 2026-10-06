use serde_json::{Value, json};

pub(super) fn payload() -> Value {
    let id = json!({"type":["string","null"],"minLength":1,"maxLength":160,
        "description":"Use JSON null when no exact association is known; never an empty string."});
    json!({"type":"object","additionalProperties":false,
        "required":["chapterId","sourceRevisionId","examinedRanges","candidates"],"properties":{
        "chapterId":{"type":"string","minLength":1,"maxLength":160},
        "sourceRevisionId":{"type":"string","minLength":1,"maxLength":160},
        "examinedRanges":{"type":"array","maxItems":16,"items":{
            "type":"object","additionalProperties":false,"required":["start","end"],"properties":{
            "start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}}}},
        "candidates":{"type":"array","maxItems":16,"items":{
            "type":"object","additionalProperties":false,
            "required":["id","kind","summary","relatedEntityIds","setupEvidenceRefs",
                "fulfillmentCriteria","targetWindow","status","fulfillmentEvidenceRefs"],"properties":{
            "id":{"type":"string","minLength":1,"maxLength":120},
            "kind":{"type":"string","enum":["foreshadowing","suspense","characterTransformation","goal","setPiece"]},
            "summary":{"type":"string","minLength":1,"maxLength":500},
            "relatedEntityIds":{"type":"array","maxItems":16,"uniqueItems":true,
                "items":{"type":"string","minLength":1,"maxLength":160}},
            "plotThreadId":id,"characterArcId":id,
            "setupEvidenceRefs":{"type":"array","minItems":1,"maxItems":8,"items":{
                "type":"object","additionalProperties":false,"required":["objectId","revisionId","quote","start","end"],
                "description":"Exact whole-body Unicode offsets, end exclusive. No sceneId or field-relative offsets.","properties":{
                "objectId":{"type":"string","minLength":1,"maxLength":160},
                "revisionId":{"type":"string","minLength":1,"maxLength":160},
                "quote":{"type":"string","minLength":1,"maxLength":2000},
                "start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}}}},
            "fulfillmentCriteria":{"type":"string","minLength":1,"maxLength":1000},
            "targetWindow":{"type":"object","additionalProperties":false,"required":["kind"],
                "properties":{"kind":{"type":"string","enum":["open"]}}},
            "status":{"type":"string","enum":["open"]},
            "fulfillmentEvidenceRefs":{"type":"array","maxItems":0},"decisionReason":{"type":"null"}
        }}}
    }})
}

// Validate the failed shapes before HTTP; source positions remain Runtime-owned.
pub(super) fn validate(payload: &Value) -> Result<(), String> {
    let candidates = payload["candidates"]
        .as_array()
        .ok_or("payload.candidates must be an array")?;
    if candidates.len() > 16 {
        return Err("payload.candidates exceeds 16 proposals".into());
    }
    for (index, candidate) in candidates.iter().enumerate() {
        for field in ["plotThreadId", "characterArcId"] {
            if candidate.get(field).is_some_and(|id| {
                !id.is_null()
                    && !id.as_str().is_some_and(|value| {
                        !value.trim().is_empty() && value.chars().count() <= 160
                    })
            }) {
                return Err(format!(
                    "payload.candidates[{index}].{field}: use null or a non-empty ID"
                ));
            }
        }
        for (anchor_index, anchor) in candidate["setupEvidenceRefs"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if anchor.as_object().is_some_and(|fields| {
                fields.keys().any(|field| {
                    !["objectId", "revisionId", "quote", "start", "end"].contains(&field.as_str())
                })
            }) {
                return Err(format!(
                    "payload.candidates[{index}].setupEvidenceRefs[{anchor_index}]: unexpected field; only objectId, revisionId, quote, start, end are allowed"
                ));
            }
        }
    }
    Ok(())
}
