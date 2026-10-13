//! The model sees the same report shape that Native validates. Negative
//! judgments are successful reports; missing provenance is not a new schema.
use serde_json::{Value, json};

pub(super) fn payload() -> Value {
    let id = json!({"type":"string","minLength":1,"maxLength":160});
    let nullable_id = json!({"anyOf":[id.clone(),{"type":"null"}]});
    json!({"type":"object","additionalProperties":false,
    "required":["writerTaskId","chapterId","originalRevisionId","draftRevisionId","knowledgeBatchId","uncoveredRequirements","outcomes"],
    "properties":{
        "writerTaskId":id,"chapterId":id,"draftRevisionId":id,
        "originalRevisionId":nullable_id,"knowledgeBatchId":nullable_id,
        "uncoveredRequirements":{"type":"array","maxItems":8,"description":"After actually reading authorRequirementsSource, register omitted or misinterpreted original requirements with stable issue links and actual author quotes. Empty means no omission was found, not certainty of completeness. Any entry prevents passing; correct the interpretation and obtain a fresh independent assessment.","items":{"type":"object","additionalProperties":false,"required":["issue","requirement","reason","authorSourceChecks"],"properties":{
            "issue":super::work_issue_schema::link(),"requirement":{"type":"string","minLength":1,"maxLength":600},
            "reason":{"type":"string","minLength":1,"maxLength":1600},
            "authorSourceChecks":{"type":"array","minItems":1,"maxItems":4,"items":super::work_issue_schema::author_quote()}}}},
        "outcomes":{"type":"array","minItems":1,"maxItems":16,"items":{
            "type":"object","additionalProperties":false,
            "required":["id","status","reason","evidence","issue"],
            "properties":{
                "id":{"type":"string","minLength":1,"maxLength":160,"description":"Copy one ID from revisionOutcomeInput.criterionContract.expectedCriterionIds (the frozen desiredOutcomes); report each exactly once. Never invent an ID for length: Native checks actualLength separately. Omitted original requirements belong in uncoveredRequirements with actual author-source evidence."},
                "status":{"type":"string","enum":["supported","unmet","insufficientEvidence"]},
                "reason":{"type":"string","minLength":1,"maxLength":1000},
                "issue":{"anyOf":[super::work_issue_schema::link(),{"type":"null"}]},
                "evidence":{"type":"array","maxItems":4,"items":{
                    "type":"object","additionalProperties":false,"required":["groupId","quote"],
                    "properties":{
                        "groupId":{"type":"string","minLength":1,"maxLength":160,"description":"A group ID from this assessment’s current candidate draft only. Other chapters inform your reasoning but cannot be used as candidate evidence anchors."},
                        "quote":{"type":"string","minLength":1,"maxLength":1000},
                        "start":{"type":"integer","minimum":0},
                        "end":{"type":"integer","minimum":1}
                    },
                    "dependentRequired":{"start":["end"],"end":["start"]}
                }}
            },
            "allOf":[{"if":{"properties":{"status":{"enum":["supported","unmet"]}}},
                "then":{"properties":{"evidence":{"minItems":1}}}},
                {"if":{"properties":{"status":{"enum":["unmet","insufficientEvidence"]}}},
                "then":{"properties":{"issue":super::work_issue_schema::link()}}}]
        }}
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separate_criteria_can_explicitly_share_one_new_issue_in_a_report() {
        let validator = jsonschema::validator_for(&payload()).unwrap();
        let issue = json!({"issueId":"early-knowledge","newIssue":{"summary":"人物提前知情"},"resolutionOf":[]});
        let mut report = json!({"writerTaskId":"writer","chapterId":"chapter","originalRevisionId":null,
        "draftRevisionId":"saved","knowledgeBatchId":null,"uncoveredRequirements":[],
        "outcomes":[
            {"id":"knowledge-rule","status":"unmet","reason":"人物提前知情","issue":issue,
                "evidence":[{"groupId":"scene","quote":"他已经知道了。"}]},
            {"id":"reveal-order","status":"unmet","reason":"同一提前知情也打破揭示顺序","issue":issue,
                "evidence":[{"groupId":"scene","quote":"他已经知道了。"}]}
        ]});
        assert!(validator.is_valid(&report));
        report["outcomes"][0]["issue"]["newIssue"] = Value::Null;
        assert!(validator.is_valid(&report));
        report["outcomes"][0]["issue"]["newIssue"] = json!({"summary":"人物提前知情"});
        report["outcomes"][1]["issue"]["newIssue"] = Value::Null;
        assert!(validator.is_valid(&report));
        // Native validates the complete report atomically, including the
        // identical definition/chapter, empty first resolutions and ownership.
        // The strict schema describes each explicit link; it does not decide
        // whether separate judgments concern the same literary problem.
    }

    #[test]
    fn report_contract_accepts_negative_reports_and_rejects_guessed_shapes() {
        let validator = jsonschema::validator_for(&payload()).unwrap();
        let mut report = json!({"writerTaskId":"writer","chapterId":"chapter","originalRevisionId":null,
            "draftRevisionId":"saved","knowledgeBatchId":null,"uncoveredRequirements":[],
            "outcomes":[{"id":"knowledge-rule","status":"unmet","reason":"人物提前知情",
                "issue":{"issueId":"early-knowledge","newIssue":{"summary":"人物提前知情"},"resolutionOf":[]},
                "evidence":[{"groupId":"scene","quote":"他已经知道了。"}]}]});
        assert!(validator.is_valid(&report));
        report["outcomes"][0]["evidence"] = json!([]);
        assert!(!validator.is_valid(&report));
        report["outcomes"][0]["status"] = json!("insufficientEvidence");
        assert!(validator.is_valid(&report));
        report["outcomes"][0]["issue"] = Value::Null;
        assert!(!validator.is_valid(&report));
        report["outcomes"][0]["status"] = json!("supported");
        report["outcomes"][0]["evidence"] = json!([{"groupId":"scene","quote":"他已经知道了。"}]);
        assert!(validator.is_valid(&report));
        report["criteria"] = json!([]);
        assert!(!validator.is_valid(&report));
        report.as_object_mut().unwrap().remove("criteria");
        report["outcomes"][0]["evidence"] = json!([{"groupId":"scene","quote":"x"}]);
        report["uncoveredRequirements"] = json!(["遗漏原要求"]);
        assert!(!validator.is_valid(&report));
        report["uncoveredRequirements"] = json!([{"issue":{"issueId":"author-omission","newIssue":{"summary":"遗漏原要求"},"resolutionOf":[]},"requirement":"保留告别信","reason":"清单没有覆盖作者原话","authorSourceChecks":[{"sourceId":"source","contentHash":"a".repeat(64),"quote":"保留告别信"}]}]);
        assert!(validator.is_valid(&report));
        report["outcomes"][0]["evidence"] = json!([{"groupId":"scene","quote":"x","start":0}]);
        assert!(!validator.is_valid(&report));
    }
}
