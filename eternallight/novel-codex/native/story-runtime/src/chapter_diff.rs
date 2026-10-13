use serde_json::{Value, json};
use std::collections::BTreeMap;

fn key(group: &Value) -> &str {
    group["sceneId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| group["id"].as_str().unwrap_or(""))
}

pub(super) fn diff(chapter: &str, accepted: &Value, candidate: &Value) -> Vec<Value> {
    if candidate.is_null() {
        return vec![];
    }
    let from = accepted["body"].as_str().unwrap_or("");
    let to = candidate["body"].as_str().unwrap_or("");
    let before = accepted["sceneGroups"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, group)| (key(group), (index, group)))
        .collect::<BTreeMap<_, _>>();
    let after = candidate["sceneGroups"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, group)| (key(group), (index, group)))
        .collect::<BTreeMap<_, _>>();
    let mut changes = Vec::new();
    for (scene, (index, group)) in &after {
        let prior = before.get(scene);
        let kind = match prior {
            None => "added",
            Some((_, previous))
                if previous["body"] != group["body"] || previous["locked"] != group["locked"] =>
            {
                "modified"
            }
            Some((previous, _)) if previous != index => "moved",
            Some(_) => continue,
        };
        changes.push(json!({"sceneId":scene,"sceneGroupId":group["id"],"kind":kind,"fromIndex":prior.map(|(index,_)|index),"toIndex":index,"before":prior.map(|(_,group)|group),"after":group}));
    }
    for (scene, (index, group)) in &before {
        if !after.contains_key(scene) {
            changes.push(json!({"sceneId":scene,"sceneGroupId":group["id"],"kind":"deleted","fromIndex":index,"toIndex":null,"before":group,"after":null}));
        }
    }
    let metadata = ["title", "sourcePlanRefs"]
        .into_iter()
        .filter(|key| accepted[*key] != candidate[*key])
        .map(|key| json!({"field":key,"before":accepted[key],"after":candidate[key]}))
        .collect::<Vec<_>>();
    if from == to && changes.is_empty() && metadata.is_empty() {
        return vec![];
    }
    vec![
        json!({"id":format!("diff-{chapter}"),"chapter_id":chapter,"fromText":from,"toText":to,
        "summary":"候选版本与正式版本不同","createdAt":null,"addedChars":to.chars().count().saturating_sub(from.chars().count()),"removedChars":from.chars().count().saturating_sub(to.chars().count()),
        "sceneChanges":changes,"metadataChanges":metadata}),
    ]
}
