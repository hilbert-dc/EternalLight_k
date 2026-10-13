//! One lossless prose representation for already-authorized draft envelopes.
use serde_json::{Value, json};

/// Omit only an exactly reconstructible aggregate body. Never change scene text,
/// metadata, visibility or persisted drafts. This operates on one Native envelope.
pub fn project_draft_source_text(draft: &mut Value) {
    let Some(content) = draft.get_mut("content").and_then(Value::as_object_mut) else {
        return;
    };
    let reconstructed = content
        .get("sceneGroups")
        .and_then(Value::as_array)
        .filter(|groups| !groups.is_empty())
        .and_then(|groups| {
            groups
                .iter()
                .map(|group| group["body"].as_str().map(str::trim))
                .collect::<Option<Vec<_>>>()
        })
        .map(|bodies| bodies.join("\n\n---\n\n"));
    if reconstructed.as_deref().is_some_and(|text| {
        !content.contains_key("body") || content.get("body").and_then(Value::as_str) == Some(text)
    }) {
        content.remove("body");
        draft["textRepresentation"] = json!("sceneGroups");
    } else {
        draft["textRepresentation"] = json!("body");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_projection_preserves_unicode_metadata_empty_groups_and_is_idempotent() {
        let groups = json!([{"id":"a","body":" 𠮷é🙂 ","locked":true,"extra":{"a":1}}, {"id":"b","body":"  ","locked":false}]);
        let mut source = json!({"sourceRevisionId":"r","content":{"title":"原题","body":"𠮷é🙂\n\n---\n\n","sceneGroups":groups}});
        project_draft_source_text(&mut source);
        assert!(source["content"].get("body").is_none());
        assert_eq!(source["content"]["sceneGroups"], groups);
        assert_eq!(source["sourceRevisionId"], "r");
        let once = source.clone();
        project_draft_source_text(&mut source);
        assert_eq!(source, once);
    }
    #[test]
    fn non_equivalent_or_malformed_groups_never_discard_text() {
        for content in [
            json!({"body":"distinct","sceneGroups":[{"body":"other"}]}),
            json!({"body":" text ","sceneGroups":[{"body":" text "}]}),
            json!({"body":"text","sceneGroups":[]}),
            json!({"body":"text","sceneGroups":[{"body":null}]}),
            json!({"body":"text"}),
        ] {
            let mut source = json!({"content":content});
            project_draft_source_text(&mut source);
            assert_eq!(source["content"], content);
            assert_eq!(source["textRepresentation"], "body");
        }
    }
}
