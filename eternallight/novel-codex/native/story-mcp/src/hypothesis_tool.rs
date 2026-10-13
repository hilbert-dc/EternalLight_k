use serde_json::{Value, json};

pub(crate) fn validate(args: &Value) -> Result<(), String> {
    if args.to_string().len() > 8_192 {
        return Err("hypothesis tool input exceeds 8KiB".into());
    }
    let fields: &[&str] = match args["action"].as_str() {
        Some("readBaseline") => &["entityId", "cursor", "limit"],
        Some("simulate") => {
            for field in ["baselineToken", "intent", "logicalKey"] {
                if args[field].as_str().is_none_or(|s| s.is_empty()) {
                    return Err(format!("{field} is required"));
                }
            }
            if !args["assumptions"]
                .as_array()
                .is_some_and(|a| (1..=25).contains(&a.len()))
            {
                return Err("assumptions must contain 1-25 exact baseline references".into());
            }
            &[
                "baselineToken",
                "intent",
                "logicalKey",
                "assumptions",
                "limit",
            ]
        }
        _ => return Err("action must be readBaseline or simulate".into()),
    };
    for key in args.as_object().ok_or("Expected an action object")?.keys() {
        if ![
            "action",
            "taskId",
            "projectId",
            "threadId",
            "expectedRevision",
        ]
        .contains(&key.as_str())
            && !fields.contains(&key.as_str())
        {
            return Err(format!("{key} is not permitted for this action"));
        }
    }
    if args
        .get("limit")
        .is_some_and(|v| !v.as_u64().is_some_and(|n| (1..=16).contains(&n)))
    {
        return Err("limit must be 1-16".into());
    }
    Ok(())
}

pub(crate) fn spec() -> Value {
    json!({"name":"simulate_story_hypothesis",
    "description":"Only for an explicit counterfactual/hypothesis request in a live root advisory task. First action=readBaseline: read the Runtime-derived fixed saved snapshot, exact subject/property and claim references. Optional entityId must be resolved, never guessed from a name; cursor is opaque, repeat the same entityId. Copy the returned baselineToken. action=simulate takes only references from that delivered page; assumeClaim adds a hypothetical claim using an existing exact subject/property; original claims remain unless explicitly suppressed. suppressClaim copies exact modelingBatchId/sourceRevisionId/assertionIndex. A local immutable analysis record is saved, not正文/formal facts/identity/publication. logicalKey identifies this exact analysis; retry the identical key+intent+assumptions after unknown receipt, never generate a new key to bypass failure. Read assumptions/suppressedEvidence/retained counts/findings/coverage separately. limit 1-16 controls only findings previews with explicit counts/truncation; after report budget failure retry the same logicalKey/intent/assumptions with smaller limit to read the same saved record. Only正文 and knowledge captured by this snapshot are covered; missing chapters, later edits and later knowledge are outside it. Do not publish automatically to obtain a baseline or treat unknown/truncated consequences as no impact. At most16 records/task; no caller snapshot, scope or hypothesis ID.",
    "inputSchema":{"type":"object","additionalProperties":false,"required":["taskId","projectId","threadId","expectedRevision","action"],
        "properties":{"taskId":{"type":"string","minLength":1},"projectId":{"type":"string","minLength":1},"threadId":{"type":"string","minLength":1},
            "expectedRevision":{"type":"integer","minimum":0},"action":{"type":"string","enum":["readBaseline","simulate"]},
            "entityId":{"type":["string","null"],"maxLength":160},"cursor":{"type":["string","null"],"maxLength":1024},"limit":{"type":"integer","minimum":1,"maximum":16},
            "baselineToken":{"type":"string","maxLength":1024},"intent":{"type":"string","minLength":1,"maxLength":1000},"logicalKey":{"type":"string","minLength":1,"maxLength":80},
            "assumptions":{"type":"array","minItems":1,"maxItems":25,"items":{"oneOf":[
                {"type":"object","additionalProperties":false,"required":["kind","modelingBatchId","sourceRevisionId","assertionIndex"],"properties":{
                    "kind":{"const":"suppressClaim"},"modelingBatchId":{"type":"string","minLength":1},"sourceRevisionId":{"type":"string","minLength":1},"assertionIndex":{"type":"integer","minimum":0}}},
                {"type":"object","additionalProperties":false,"required":["kind","subjectId","predicate","value"],"properties":{
                    "kind":{"const":"assumeClaim"},"subjectId":{"type":"string","minLength":1},"predicate":{"type":"string","minLength":1,"maxLength":160},"value":{"type":["string","number","boolean","null"]},"storyTime":{"type":["integer","null"]}}}
            ]}}}}})
}
