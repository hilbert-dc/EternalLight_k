use serde_json::{Value, json};

pub(crate) fn query_plan_schema() -> Value {
    let ranking = json!({"oneOf":[
        {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"literal"}}},
        {"type":"object","additionalProperties":false,"required":["type","baseReportHash","judgments"],
         "properties":{"type":{"const":"modelSemantic"},"baseReportHash":{"type":"string","minLength":64,"maxLength":64,"description":"Exact lowercase SHA-256 contentHash of the baseline report; Runtime verifies identity."},
            "judgments":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"object","additionalProperties":false,
                "required":["resultId","score","rationale"],"properties":{"resultId":{"type":"string","minLength":64,"maxLength":64,"description":"Exact resultId from the baseline report; Runtime verifies membership."},
                    "score":{"type":"integer","minimum":0,"maximum":1000},"rationale":{"type":"string","minLength":1,"maxLength":256}}}}}}
    ]});
    let text = json!({"type":"object","additionalProperties":false,"required":["type","query","limit"],
        "properties":{"type":{"const":"text"},"query":{"type":"string","minLength":1,"maxLength":240},"limit":{"type":"integer","minimum":1,"maximum":20}}});
    let passages = json!({"type":"object","additionalProperties":false,"required":["type","cursor","maxChars","limit"],
        "properties":{"type":{"const":"passages"},"cursor":{"type":["string","null"],"minLength":1,"maxLength":768},
            "maxChars":{"type":"integer","minimum":80,"maximum":400},"limit":{"type":"integer","minimum":1,"maximum":4}}});
    let resolve = json!({"type":"object","additionalProperties":false,"required":["type","name"],
        "properties":{"type":{"const":"resolveEntity"},"name":{"type":"string","minLength":1,"maxLength":160}}});
    let state = json!({"type":"object","additionalProperties":false,"required":["type","entityId","perspective"],
        "properties":{"type":{"const":"state"},"entityId":{"type":"string","minLength":1,"maxLength":160},
            "perspective":{"enum":["author","world","character","reader"]},"predicate":{"type":"string","minLength":1,"maxLength":80},
            "ownerId":{"type":"string","minLength":1,"maxLength":160},"readingChapter":{"type":"integer","minimum":0},
            "worldTime":{"type":"integer"},"epistemicTime":{"type":"integer"}}});
    let relations = json!({"type":"object","additionalProperties":false,"required":["type","entityId","direction","maxHops","limit"],
        "properties":{"type":{"const":"relations"},"entityId":{"type":"string","minLength":1,"maxLength":160},
            "relationType":{"type":"string","minLength":1,"maxLength":160},"direction":{"enum":["outgoing","incoming","both"]},
            "worldTime":{"type":"integer"},"maxHops":{"type":"integer","minimum":1,"maximum":3},"limit":{"type":"integer","minimum":1,"maximum":20}}});
    json!({"type":"object","additionalProperties":false,"required":["question","byteBudget","steps","ranking"],
        "properties":{"ranking":ranking,"question":{"type":"string","minLength":1,"maxLength":1000},"byteBudget":{"type":"integer","minimum":4000,"maximum":9000,"description":"Query report budget: 4000 through 9000 bytes inclusive, distinct from read_context.byteBudget. If truncated, use fewer steps or smaller result limits."},
            "steps":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["id","operation"],
                "properties":{"id":{"type":"string","minLength":1,"maxLength":32},"operation":{"oneOf":[text,passages,resolve,state,relations]}}}}}})
}
