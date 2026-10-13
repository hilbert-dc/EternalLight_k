use serde_json::{Value, json};
pub(super) fn schema() -> Value {
    let text = json!({"type":"string","minLength":1,"maxLength":1600});
    let issues = json!({"type":"array","description":"Only real author decisions blocking the CURRENT authorized goal. Use [] when none. Future optional adoption, publication or further editing suggestions are not blockers of a candidate-only goal; explain those separately in the summary. Never invent a decision to hide incomplete work.","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":600}});
    let mut resolution = super::work_issue_schema::link();
    resolution["properties"]["newIssue"] = json!({"type":"null"});
    resolution["properties"]["resolutionOf"]["minItems"] = json!(1);
    json!({"type":"object","additionalProperties":false,"required":["expectedWorkVersion","expectedSourceDigest","goalStatus","goalAssessment","checkedScope","unresolved","issueResolutions","authorDecisions"],"properties":{
      "expectedWorkVersion":{"type":"integer","minimum":0,"description":"Current read_work_status version. This field belongs inside deliveryReview, not the complete_task root."},"expectedSourceDigest":{"type":"string","minLength":1,"maxLength":160,"description":"Current read_work_status sourceDigest, nested inside deliveryReview alongside expectedWorkVersion."},
      "goalStatus":{"type":"string","enum":["fulfilled","partial","needsAuthorDecision"],"description":"fulfilled requires all authorized goals complete and both unresolved=[] and authorDecisions=[]. partial requires actual unresolved work. needsAuthorDecision requires a real blocking author decision."},
      "goalAssessment":text,"checkedScope":text,
      "unresolved":{"type":"array","maxItems":16,"items":{"type":"object","additionalProperties":false,"required":["issue","observationId","reason"],"properties":{
        "issue":super::work_issue_schema::link(),"observationId":super::work_issue_schema::nullable_issue_id(),
        "reason":{"type":"string","minLength":1,"maxLength":600}}}},
      "issueResolutions":{"type":"array","maxItems":16,"items":{"type":"object","additionalProperties":false,"required":["issue","reason"],"properties":{
        "issue":resolution,"reason":{"type":"string","minLength":1,"maxLength":600}}}},"authorDecisions":issues,
      "evidence":{"type":"array","description":"Optional strict assertion of the complete current saved outputs. Omit for Native to collect all current outputs of this Work; do not copy IDs from status pages. Explicit lists must be exact and complete.","maxItems":1024,"items":{"type":"object","additionalProperties":false,"required":["taskId","revisionId"],"properties":{"taskId":{"type":"string","minLength":1,"maxLength":160},"revisionId":{"type":"string","minLength":1,"maxLength":160}}}}
    },"oneOf":[
        {"properties":{"goalStatus":{"const":"fulfilled"},"unresolved":{"maxItems":0},"authorDecisions":{"maxItems":0}}},
        {"properties":{"goalStatus":{"const":"partial"},"unresolved":{"minItems":1}}},
        {"properties":{"goalStatus":{"const":"needsAuthorDecision"},"authorDecisions":{"minItems":1}}}
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_review_contract_accepts_only_bounded_declared_fields() {
        let validator = jsonschema::validator_for(&schema()).unwrap();
        let valid = json!({"expectedWorkVersion":1,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"完成指定改句","checkedScope":"只读该句上下文","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":[{"taskId":"writer","revisionId":"draft"}]});
        assert!(validator.is_valid(&valid));
        for (field, value) in [
            ("goalStatus", json!("allTestsPassed")),
            ("goalStatus", json!("partial")),
            ("goalStatus", json!("needsAuthorDecision")),
            ("authorDecisions", json!(["以后可自行选择是否采用"])),
            ("evidence", json!([{"taskId":"writer"}])),
            ("extra", json!(true)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(!validator.is_valid(&invalid));
        }
        let mut implicit = valid.clone();
        implicit.as_object_mut().unwrap().remove("evidence");
        assert!(validator.is_valid(&implicit));
        implicit["evidence"] = Value::Null;
        assert!(!validator.is_valid(&implicit));
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("checkedScope");
        assert!(!validator.is_valid(&missing));
    }
    #[test]
    fn delivery_evidence_supports_large_authorized_work_but_remains_bounded() {
        let validator = jsonschema::validator_for(&schema()).unwrap();
        let mut review = json!({"expectedWorkVersion":1,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"核对授权章节","checkedScope":"当前正文与所选检查成果","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":[]});
        for count in [300, 1024, 1025] {
            review["evidence"] = json!((0..count).map(|i| json!({"taskId":format!("task-{i}"),"revisionId":format!("revision-{i}")})).collect::<Vec<_>>());
            assert_eq!(validator.is_valid(&review), count <= 1024);
        }
    }
    #[test]
    fn non_prose_resolution_is_explicit_and_cannot_omit_its_problem_link() {
        let validator = jsonschema::validator_for(&schema()).unwrap();
        let mut review = json!({"expectedWorkVersion":1,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"当前服务已恢复且结果已核对","checkedScope":"当前保存结果和原作者要求","unresolved":[],"issueResolutions":[{"issue":{"issueId":"account-access","newIssue":null,"resolutionOf":["observation-access"]},"reason":"已读取并核对当前保存结果"}],"authorDecisions":[],"evidence":[]});
        assert!(validator.is_valid(&review));
        let valid = review.clone();
        review["issueResolutions"][0]["issue"]["resolutionOf"] = json!([]);
        assert!(!validator.is_valid(&review));
        review = valid.clone();
        review["issueResolutions"][0]["issue"]["newIssue"] =
            json!({"summary":"新问题不能自评为解决"});
        assert!(!validator.is_valid(&review));
        review["issueResolutions"] = json!(["服务已经恢复"]);
        assert!(!validator.is_valid(&review));
        review["issueResolutions"] = json!([{"reason":"已经恢复"}]);
        assert!(!validator.is_valid(&review));
        review.as_object_mut().unwrap().remove("issueResolutions");
        assert!(!validator.is_valid(&review));
        // Semantic chapter scope and actual observation identity remain Native checks.
    }
    #[test]
    fn unresolved_delivery_uses_real_problem_links_without_string_fallback() {
        let validator = jsonschema::validator_for(&schema()).unwrap();
        let mut review = json!({"expectedWorkVersion":1,"expectedSourceDigest":"digest","goalStatus":"partial","goalAssessment":"保留已保存稿，问题尚未解决","checkedScope":"当前稿和独立报告","unresolved":[{"issue":{"issueId":"letter-action","newIssue":null,"resolutionOf":[]},"observationId":"observation-current","reason":"独立报告仍不支持藏信"}],"issueResolutions":[],"authorDecisions":[],"evidence":[]});
        assert!(validator.is_valid(&review));
        review["unresolved"] = json!(["独立报告仍不支持藏信"]);
        assert!(!validator.is_valid(&review));
        review["unresolved"] = json!([{"issue":{"issueId":"account-access","newIssue":{"summary":"当前服务不可用"},"resolutionOf":[]},"observationId":null,"reason":"无法在原授权内完成余项"}]);
        assert!(validator.is_valid(&review));
        review["unresolved"][0]
            .as_object_mut()
            .unwrap()
            .remove("observationId");
        assert!(!validator.is_valid(&review));
    }
}
