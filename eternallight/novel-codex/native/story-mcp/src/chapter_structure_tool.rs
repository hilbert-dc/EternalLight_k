use serde_json::{Value, json};
pub(crate) fn validate(arguments: &Value) -> Result<(), String> {
    if !matches!(
        arguments["sourceRole"].as_str(),
        Some("saved" | "accepted" | "working")
    ) || arguments
        .get("limit")
        .is_some_and(|v| v.as_u64().is_none_or(|n| !(1..=1000).contains(&n)))
        || arguments
            .get("cursor")
            .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 96))
    {
        return Err("invalid chapter structure source page".into());
    }
    Ok(())
}
pub(crate) fn spec() -> Value {
    json!({"name":"read_chapter_structure_source","description":"Read exact removed chapter text from this assessment's immutable structure-change event. No caller revision/snapshot is accepted. saved is the last saved draft, accepted the separately published source, working the explicitly selected baseline; absent sources return unavailable, never latest substitution. Begin cursor null, copy nextCursor for contiguous pages. start/end/total are Unicode scalar offsets in /body; quote is unmodified text. End of this suffix is not complete reading or full impact coverage. Read-only: cannot restore, save, accept, publish or create tasks.","inputSchema":{"type":"object","additionalProperties":false,"required":["projectId","taskId","threadId","expectedRevision","sourceRole"],"properties":{
        "projectId":{"type":"string","minLength":1,"maxLength":160},"taskId":{"type":"string","minLength":1,"maxLength":160},"threadId":{"type":"string","minLength":1,"maxLength":160},"expectedRevision":{"type":"integer","minimum":0},
        "sourceRole":{"enum":["saved","accepted","working"]},"cursor":{"type":["string","null"],"maxLength":96},"limit":{"type":"integer","minimum":1,"maximum":1000,"description":"Unicode scalar count; default 600, maximum 1000. A budget error delivers no page."}}}})
}
