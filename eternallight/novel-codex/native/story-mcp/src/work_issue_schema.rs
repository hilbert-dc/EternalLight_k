use serde_json::{Value, json};

pub(crate) fn id() -> Value {
    json!({"type":"string","minLength":1,"maxLength":160})
}

pub(crate) fn issue_id() -> Value {
    json!({"type":"string","minLength":1,"maxLength":160,"pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$"})
}

pub(crate) fn nullable_issue_id() -> Value {
    json!({"anyOf":[issue_id(),{"type":"null"}]})
}

pub(crate) fn link() -> Value {
    json!({"type":"object","additionalProperties":false,
        "required":["issueId","newIssue","resolutionOf"],"properties":{
            "issueId":issue_id(),
            "newIssue":{"anyOf":[{"type":"object","additionalProperties":false,"required":["summary"],"properties":{
                "summary":{"type":"string","minLength":1,"maxLength":600}}},{"type":"null"}]},
            "resolutionOf":{"type":"array","maxItems":16,"uniqueItems":true,"items":issue_id()}
        },"description":"Choose a stable problem identity, never a per-report replacement. A new problem supplies newIssue.summary. Within one actual revisionOutcome report, multiple criteria may explicitly share that new issueId using the identical summary and chapter, or newIssue:null referencing that report's definition; array order does not grant authority. All links for a first definition have empty resolutionOf. Native atomically registers the problem once and retains each criterion's observation; it never merges by text. Subsequent reports use the existing issueId with newIssue:null. To resolve earlier findings, copy their actual Native observationIds into resolutionOf after reading this Work's issue records. Native rejects conflicting definitions, chapters, Work ownership or forged references, and validates current sources and verdict; a link does not certify semantic correctness."})
}

pub(crate) fn author_quote() -> Value {
    json!({"type":"object","additionalProperties":false,
        "description":"Evidence from the author's actual original input, never a model interpretation or suggestion. Read the current task's source whose sectionPath is /authorRequirementsSource; copy that source's sourceId/contentHash and quote its authorText exactly. revisionOutcomeInput contains model-selected frozen criteria and engineering instructions; it, chapterAcceptance and plans are not authorRequirementsSource. Native checks same Work ownership, current source/hash, literal quote and actual main-task reading. A source reference cannot turn model advice into an original author requirement.",
        "required":["sourceId","contentHash","quote"],"properties":{
        "sourceId":id(),"contentHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},
        "quote":{"type":"string","minLength":1,"maxLength":2000}}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn issue_links_are_explicit_bounded_current_contracts() {
        let validator = jsonschema::validator_for(&link()).unwrap();
        let valid = json!({"issueId":"letter-action","newIssue":{"summary":"密函仍被烧毁"},"resolutionOf":[]});
        assert!(validator.is_valid(&valid));
        assert!(validator.is_valid(
            &json!({"issueId":"letter-action","newIssue":null,"resolutionOf":["observation-one"]})
        ));
        for field in ["issueId", "newIssue", "resolutionOf"] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(!validator.is_valid(&missing));
        }
        for invalid in [
            json!("密函仍被烧毁"),
            json!({"issueId":"letter-action","newIssue":null,"resolutionOf":["same","same"]}),
            json!({"issueId":"正文问题","newIssue":null,"resolutionOf":[]}),
            json!({"issueId":"letter-action","newIssue":{"summary":"问题","status":"resolved"},"resolutionOf":[]}),
        ] {
            assert!(!validator.is_valid(&invalid));
        }
    }
}
