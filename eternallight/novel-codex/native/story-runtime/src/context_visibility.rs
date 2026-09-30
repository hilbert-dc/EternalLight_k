use novel_codex_story_core::RepositoryError;
pub(crate) use novel_codex_story_core::writer_scene_limit;
use serde_json::{Value, json};

pub(crate) fn visible_draft_prefix(
    content: &mut Value,
    end_scene_order: usize,
) -> Result<(), RepositoryError> {
    let groups = content["sceneGroups"]
        .as_array_mut()
        .ok_or(RepositoryError::CorruptState)?;
    groups.truncate(end_scene_order);
    let body = groups
        .iter()
        .map(|group| {
            group["body"]
                .as_str()
                .map(str::trim)
                .ok_or(RepositoryError::CorruptState)
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("\n\n---\n\n");
    content["body"] = json!(body);
    Ok(())
}

pub(crate) fn visible_scene_plan(chapter: &Value, end_scene_order: usize) -> Value {
    let Some(scenes) = chapter["scenes"].as_array() else {
        return Value::Null;
    };
    json!({"chapters": [{
        "chapterNumber": chapter["chapterNumber"],
        "scenes": scenes.iter().take(end_scene_order).collect::<Vec<_>>()
    }]})
}

pub(crate) fn merge_visible_draft_prefix(
    groups: &mut Vec<Value>,
    prior: Option<&Value>,
    end_scene_order: usize,
) -> Result<String, RepositoryError> {
    if groups.len() > end_scene_order {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    if let Some(prior) = prior {
        let prior_groups = prior["sceneGroups"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        if prior_groups.len() > end_scene_order {
            if groups.len() != end_scene_order {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            groups.extend(prior_groups.iter().skip(end_scene_order).cloned());
        }
    }
    groups
        .iter()
        .map(|group| {
            group["body"]
                .as_str()
                .map(str::trim)
                .ok_or(RepositoryError::CorruptState)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|bodies| bodies.join("\n\n---\n\n"))
}
