//! Locations in already-authorized immutable prose. An anchor grants no write authority.
use serde_json::{Value, json};

// Match only Native prose envelopes. A match reads exactly its quote, never a
// whole source or a writable scope. Repeated/overlapping occurrences stay distinct.
pub(crate) fn locate(content: &Value, quote: &str, group_id: Option<&str>) -> Vec<Value> {
    let sources: Vec<(String, &Value)> = match content.as_array() {
        Some(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("/{i}"), v))
            .collect(),
        None => vec![(String::new(), content)],
    };
    let mut matches = Vec::new();
    for (prefix, source) in sources {
        let Some(groups) = source["content"]["sceneGroups"].as_array() else {
            continue;
        };
        for (i, group) in groups.iter().enumerate() {
            if group_id.is_some_and(|id| group["id"] != id) {
                continue;
            }
            let Some(body) = group["body"].as_str() else {
                continue;
            };
            let total = body.chars().count();
            let mut template = json!({"path":format!("{prefix}/content/sceneGroups/{i}/body"),"text":quote,
                "characterRange":{"start":0,"end":0,"total":total}});
            attach(content, std::slice::from_mut(&mut template));
            if template.get("editAnchor").is_none() {
                continue;
            }
            let length = quote.chars().count();
            for (start, (byte, _)) in body.char_indices().enumerate() {
                if !body[byte..].starts_with(quote) {
                    continue;
                }
                let mut entry = template.clone();
                for key in ["characterRange", "editAnchor"] {
                    entry[key]["start"] = json!(start);
                    entry[key]["end"] = json!(start + length);
                }
                entry["evidence"] = json!({"groupId":group["id"],"quote":quote,"start":start,"end":start+length});
                matches.push(entry);
            }
        }
    }
    matches
}

/// Exact boundary text from the first/last non-empty visible prose group.
/// No semantic summary or complete-source reading credit is produced.
pub(crate) fn boundary_excerpt(content: &Value, edge: &str, characters: usize) -> Option<Value> {
    let groups = content["content"]["sceneGroups"].as_array()?;
    let nonempty = |(_, g): &(usize, &Value)| g["body"].as_str().is_some_and(|s| !s.trim().is_empty());
    let (index, group) = if edge == "start" {
        groups.iter().enumerate().find(nonempty)?
    } else {
        groups.iter().enumerate().rfind(nonempty)?
    };
    let body = group["body"].as_str()?;
    let total = body.chars().count();
    let start = if edge == "end" { total.saturating_sub(characters) } else { 0 };
    let end = (start + characters).min(total);
    let text: String = body.chars().skip(start).take(end - start).collect();
    let mut entry = json!({"path":format!("/content/sceneGroups/{index}/body"),"text":text,
        "characterRange":{"start":start,"end":end,"total":total}});
    attach(content, std::slice::from_mut(&mut entry));
    entry.get("editAnchor")?;
    Some(entry)
}

pub(crate) fn attach(content: &Value, entries: &mut [Value]) {
    for entry in entries {
        let Some(path) = entry["path"].as_str() else {
            continue;
        };
        let Some((parent, suffix)) = path.split_once("/content/sceneGroups/") else {
            continue;
        };
        // Only Native's source envelope or an item in relevantText. Never interpret
        // similarly named fields embedded inside an author's payload as provenance.
        if !parent.is_empty()
            && !parent
                .strip_prefix('/')
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        let Some(index) = suffix
            .strip_suffix("/body")
            .and_then(|s| s.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(source) = content.pointer(parent) else {
            continue;
        };
        let draft = &source["content"];
        let Some(group) = draft["sceneGroups"].as_array().and_then(|g| g.get(index)) else {
            continue;
        };
        let identity = |v: &Value| {
            v.as_str().is_some_and(|s| {
                !s.is_empty() && s.len() <= 160 && !s.chars().any(char::is_control)
            })
        };
        if !identity(&source["sourceRevisionId"])
            || !identity(&draft["chapterId"])
            || !identity(&group["id"])
        {
            continue;
        }
        let Some(body) = group["body"].as_str() else {
            continue;
        };
        let total = body.chars().count();
        let range = entry
            .get("characterRange")
            .cloned()
            .unwrap_or_else(|| json!({"start":0,"end":total,"total":total}));
        entry["characterRange"] = range.clone();
        entry["editAnchor"] = json!({"chapterId":draft["chapterId"],"sourceRevisionId":source["sourceRevisionId"],
            "groupId":group["id"],"start":range["start"],"end":range["end"],"offsetUnit":"unicodeScalar",
            "locked":group["locked"],"writeAuthority":"notGranted"});
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_excerpts_keep_original_unicode_whitespace_and_immutable_coordinates() {
        let content = json!({"sourceRevisionId":"r", "content":{"chapterId":"c","sceneGroups":[
            {"id":"empty","body":"  "},{"id":"first","body":"风🙂雨\n","locked":true},
            {"id":"last","body":"旧末句。新末句🙂","locked":false},{"id":"blank","body":"\n"}]}});
        let first = boundary_excerpt(&content,"start",2).unwrap();
        assert_eq!(first["text"],"风🙂");
        assert_eq!(first["editAnchor"]["locked"],true);
        let last = boundary_excerpt(&content,"end",4).unwrap();
        assert_eq!(last["text"],"新末句🙂");
        assert_eq!(last["characterRange"],json!({"start":4,"end":8,"total":8}));
        assert_eq!(last["editAnchor"]["groupId"],"last");
        assert_eq!(last["editAnchor"]["sourceRevisionId"],"r");
        assert!(boundary_excerpt(&json!({"notes":content}),"end",4).is_none());
    }
    #[test]
    fn exact_locator_keeps_overlap_unicode_and_locked_identity() {
        let content = json!({"sourceRevisionId":"revision","content":{"chapterId":"chapter","sceneGroups":[
            {"id":"locked","locked":true,"body":"🌙哈哈哈"},{"id":"other","locked":false,"body":"哈哈"}]}});
        let matches = locate(&content, "哈哈", Some("locked"));
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["evidence"],json!({"groupId":"locked","quote":"哈哈","start":1,"end":3}));
        assert!(crate::source_evidence::resolve(&json!({"quote":matches[0]["evidence"]["quote"],"start":1,"end":3}),"🌙哈哈哈").is_ok());
        assert_eq!(matches[0]["editAnchor"]["start"], 1);
        assert_eq!(matches[1]["editAnchor"]["end"], 4);
        assert_eq!(matches[0]["editAnchor"]["locked"], true);
        assert_eq!(matches[0]["editAnchor"]["writeAuthority"], "notGranted");
        assert_eq!(locate(&content, "哈哈", None).len(), 3);
        assert!(locate(&content, "不存在", None).is_empty());
        assert!(locate(&json!({"notes":content}), "哈哈", None).is_empty());
    }
    #[test]
    fn anchors_preserve_unicode_offsets_and_only_native_envelope_provenance() {
        let content = json!([{ "sourceRevisionId":"revision", "content":{"chapterId":"chapter", "sceneGroups":[{"id":"group","body":"风🌙雨","locked":true}]}}]);
        let mut entries = vec![json!({"path":"/0/content/sceneGroups/0/body","value":"风🌙雨"})];
        attach(&content, &mut entries);
        assert_eq!(entries[0]["editAnchor"]["end"], 3);
        assert_eq!(entries[0]["editAnchor"]["locked"], true);
        assert_eq!(entries[0]["editAnchor"]["sourceRevisionId"], "revision");
        let mut partial = vec![
            json!({"path":"/0/content/sceneGroups/0/body","text":"🌙","characterRange":{"start":1,"end":2,"total":3}}),
        ];
        attach(&content, &mut partial);
        assert_eq!(partial[0]["editAnchor"]["start"], 1);
        assert_eq!(partial[0]["editAnchor"]["end"], 2);
        let mut forged =
            vec![json!({"path":"/content/notes/content/sceneGroups/0/body","value":"假"})];
        attach(&json!({"content":{"notes":content[0]}}), &mut forged);
        assert!(forged[0].get("editAnchor").is_none());
    }
}
