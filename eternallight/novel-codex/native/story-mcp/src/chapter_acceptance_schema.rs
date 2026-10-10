use serde_json::{Value, json};

pub(crate) fn length() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["requirement","minCharacters","maxCharacters"],"properties":{
        "requirement":{"type":"string","minLength":1,"maxLength":600},
        "minCharacters":{"type":["integer","null"],"minimum":0,"maximum":1000000},
        "maxCharacters":{"type":["integer","null"],"minimum":0,"maximum":1000000}}})
}
pub(crate) fn acceptance() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["requirements","length"],"properties":{
    "requirements":{"type":"array","minItems":1,"maxItems":12,"uniqueItems":true,"items":{"type":"object","additionalProperties":false,"required":["id","text"],"properties":{
        "id":super::work_issue_schema::issue_id(),"text":{"type":"string","minLength":1,"maxLength":600}}}},
    "length":length(),
    "verification":{"type":"object","additionalProperties":false,"required":["method","reason"],"description":"Choose the evidence method from the author request. direct lets the root inspect saved prose and submit evidence without a separate critic. independent requires a separate assessment task; explicit author requests for independent review must use it. Omission conservatively requires independent review.","properties":{"method":{"type":"string","enum":["direct","independent"]},"reason":{"type":"string","minLength":1,"maxLength":600}}},
    "interpretationCorrection":{"type":"object","additionalProperties":false,"required":["reason","impact","sourceChecks"],"description":"Correct a model's interpretation with the applicable original author-source evidence, not a new author grant. Explain how the correction follows the applicable author requirements, including any explicit limits. Model-inferred bounds can be corrected even when the current draft then fits. A correction does not certify the draft: independently reassess against original author sources and the corrected interpretation.","properties":{
        "reason":{"type":"string","minLength":1,"maxLength":600},"impact":{"type":"string","minLength":1,"maxLength":600},
        "sourceChecks":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["sourceId","contentHash","quote"],"properties":{
            "sourceId":{"type":"string","minLength":1,"maxLength":160},"contentHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"quote":{"type":"string","minLength":1,"maxLength":2000}}}}
    }}}})
}
pub(crate) fn review() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["sourceRevisionId","assessmentRevisionId","checks","length"],"properties":{
        "sourceRevisionId":{"type":"string","minLength":1,"maxLength":160},
        "assessmentRevisionId":{"type":"string","minLength":1,"maxLength":160,"pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$","description":"Current independent revisionOutcome report revision from read_work_status; a historical supported report cannot certify a newer draft."},
        "checks":{"type":"array","minItems":1,"maxItems":12,"items":{"type":"object","additionalProperties":false,"required":["criterionId","issueId"],"properties":{
            "criterionId":{"type":"string","minLength":1,"maxLength":160,"pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$","description":"Copy chapterAcceptance.requirements[].id. Every frozen criterion exactly once; Native verifies its supported judgment in the current assessment report."},
            "issueId":super::work_issue_schema::nullable_issue_id()}}},
        "length":length()}})
}

pub(crate) fn source_check() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["sourceId","revisionId","contentHash","evidence"],"properties":{
        "sourceId":{"type":"string","minLength":1,"maxLength":160},
        "revisionId":{"type":"string","minLength":1,"maxLength":160},
        "contentHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},
        "evidence":{"type":"object","additionalProperties":false,"required":["quote"],"properties":{
            "quote":{"type":"string","minLength":1,"maxLength":2000},
            "start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0}}}}})
}
