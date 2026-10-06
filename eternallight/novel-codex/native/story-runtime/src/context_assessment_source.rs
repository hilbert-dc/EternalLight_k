use novel_codex_story_core::RepositoryError;
use serde_json::{Value, json};

/// Keeps frozen source identity in context; text is delivered by the scoped passage query.
pub(crate) fn project(draft: &mut Value) -> Result<Value, RepositoryError> {
    let content = draft["content"]
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?;
    let body = content
        .get("body")
        .and_then(Value::as_str)
        .ok_or(RepositoryError::CorruptState)?;
    let length = body.chars().count();
    let groups = content
        .get("sceneGroups")
        .and_then(Value::as_array)
        .ok_or(RepositoryError::CorruptState)?;
    let bodies = groups
        .iter()
        .map(|group| group["body"].as_str().map(str::trim))
        .collect::<Option<Vec<_>>>()
        .ok_or(RepositoryError::CorruptState)?;
    let ranges_known = bodies.join("\n\n---\n\n") == body;
    let mut offset = 0;
    let references: Vec<_> = groups.iter().zip(bodies).map(|(group, text)| {
        let end = offset + text.chars().count();
        let reference = json!({"id":group["id"],"sceneId":group["sceneId"],"locked":group["locked"],
            "start":ranges_known.then_some(offset),"end":ranges_known.then_some(end)});
        offset = end + "\n\n---\n\n".chars().count();
        reference
    }).collect();
    content.remove("body");
    content.insert("sceneGroups".into(), json!(references));
    draft["textRepresentation"] = json!("sourceReference");
    Ok(json!({
        "objectId":draft["objectId"],"sourceRevisionId":draft["sourceRevisionId"],
        "chapterId":draft["content"]["chapterId"],"chapterNumber":draft["content"]["chapterNumber"],
        "unicodeLength":length,"textDelivered":false,"method":"query_story",
        "plan":{"question":"逐页读取本任务冻结章节正文","byteBudget":9000,"ranking":{"type":"literal"},
            "steps":[{"id":"assessment-source","operation":{"type":"passages","cursor":null,"maxChars":400,"limit":2}}]}
    }))
}
