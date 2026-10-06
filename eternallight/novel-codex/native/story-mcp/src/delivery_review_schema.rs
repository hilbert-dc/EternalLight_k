use serde_json::{Value, json};
pub(super) fn schema() -> Value {
    let text = json!({"type":"string","minLength":1,"maxLength":1600});
    let issues = json!({"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":600}});
    json!({"type":"object","additionalProperties":false,"required":["expectedWorkVersion","expectedSourceDigest","goalStatus","goalAssessment","checkedScope","unresolved","authorDecisions","evidence"],"properties":{
      "expectedWorkVersion":{"type":"integer","minimum":0},"expectedSourceDigest":{"type":"string","minLength":1,"maxLength":160},
      "goalStatus":{"type":"string","enum":["fulfilled","partial","needsAuthorDecision"]},
      "goalAssessment":text,"checkedScope":text,"unresolved":issues,"authorDecisions":issues,
      "evidence":{"type":"array","maxItems":1024,"items":{"type":"object","additionalProperties":false,"required":["taskId","revisionId"],"properties":{"taskId":{"type":"string","minLength":1,"maxLength":160},"revisionId":{"type":"string","minLength":1,"maxLength":160}}}}
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_review_contract_accepts_only_bounded_declared_fields() {
        let validator = jsonschema::validator_for(&schema()).unwrap();
        let valid = json!({"expectedWorkVersion":1,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"完成指定改句","checkedScope":"只读该句上下文","unresolved":[],"authorDecisions":[],"evidence":[{"taskId":"writer","revisionId":"draft"}]});
        assert!(validator.is_valid(&valid));
        for (field, value) in [
            ("goalStatus", json!("allTestsPassed")),
            ("evidence", json!([{"taskId":"writer"}])),
            ("extra", json!(true)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(!validator.is_valid(&invalid));
        }
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("checkedScope");
        assert!(!validator.is_valid(&missing));
    }
    #[test]
    fn delivery_evidence_supports_large_authorized_work_but_remains_bounded() {
        let validator = jsonschema::validator_for(&schema()).unwrap();
        let mut review = json!({"expectedWorkVersion":1,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"核对授权章节","checkedScope":"当前正文与所选检查成果","unresolved":[],"authorDecisions":[],"evidence":[]});
        for count in [300, 1024, 1025] {
            review["evidence"] = json!((0..count).map(|i| json!({"taskId":format!("task-{i}"),"revisionId":format!("revision-{i}")})).collect::<Vec<_>>());
            assert_eq!(validator.is_valid(&review), count <= 1024);
        }
    }
}
