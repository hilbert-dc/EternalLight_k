use serde_json::{Value, json};

pub(crate) fn validate(arguments: &Value) -> Result<(), String> {
    for field in ["sourceObjectId", "sourceRevisionId"] {
        if !arguments[field]
            .as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 160)
        {
            return Err(format!(
                "{field} must be a non-empty ID of at most 160 bytes"
            ));
        }
    }
    if !arguments["quote"]
        .as_str()
        .is_some_and(|quote| !quote.is_empty() && quote.chars().count() <= 2_000)
    {
        return Err("quote must be exact source text of 1-2000 Unicode characters".into());
    }
    Ok(())
}

pub(crate) fn spec() -> Value {
    json!({"name":"resolve_story_evidence",
    "description":"Resolve an exact literal quote in the task-authorized current draft source. Never correct typos, normalize whitespace, or guess Unicode positions. Read status/source/quote/matches: unique has one whole-body /body Unicode start/end anchor; ambiguous returns up to four anchors and truncated tells whether more exist. Choose only a source occurrence actually justified by the text, or use a longer exact quote; notFound is not permission to invent evidence. Copy objectId/revisionId and the selected start/end into existing evidence fields. source.unicodeLength is the authoritative whole-body character count, not a certificate of having read it. Read-only; does not save or confirm anything.",
    "inputSchema":{"type":"object","additionalProperties":false,
        "required":["taskId","projectId","threadId","expectedRevision","sourceObjectId","sourceRevisionId","quote"],
        "properties":{
            "taskId":{"type":"string","minLength":1,"maxLength":160},
            "projectId":{"type":"string","minLength":1,"maxLength":160},
            "threadId":{"type":"string","minLength":1,"maxLength":160},
            "expectedRevision":{"type":"integer","minimum":0},
            "sourceObjectId":{"type":"string","minLength":1,"maxLength":160},
            "sourceRevisionId":{"type":"string","minLength":1,"maxLength":160},
            "quote":{"type":"string","minLength":1,"maxLength":2000}
        }}})
}
