use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::Value;

use crate::{RepositoryError, TaskRecord};

/// Planned whole-chapter agent delivery needs explicit source ranges, not proof that
/// every planned scene or effect happened. Author saves and partial revisions stay independent.
pub(crate) fn validate(
    db: &Connection,
    task: &TaskRecord,
    content_json: &str,
    completeness: Option<&str>,
) -> Result<(), RepositoryError> {
    if task.operation != "generateChapter" || completeness != Some("chapterComplete") {
        return Ok(());
    }
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if !target["scope"].is_null() && target["scope"]["type"] != "wholeChapter" {
        return Ok(());
    }
    let chapter_id = target["chapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let Some(plan) =
        crate::bound_writer_plan::bound_chapter_plan_for_chapter(db, &task.project_id, chapter_id)?
    else {
        return Ok(());
    };
    let scenes = plan["scenes"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let ids = scenes
        .iter()
        .map(|scene| {
            scene["id"]
                .as_str()
                .filter(|id| !id.trim().is_empty())
                .ok_or(RepositoryError::CorruptState)
        })
        .collect::<Result<HashSet<_>, _>>()?;
    if ids.is_empty() || ids.len() != scenes.len() {
        return Err(RepositoryError::CorruptState);
    }
    let invalid = || {
        RepositoryError::InvalidContent(
        "Planned chapterComplete requires explicit sceneGroups: at least one nonempty group must copy a bound plan sceneId; all non-null sceneIds must belong to that plan. Mapping does not certify planned effects.".into(),
    )
    };
    let content: Value = serde_json::from_str(content_json).map_err(|_| invalid())?;
    if content["chapterId"] != chapter_id || content["chapterNumber"] != plan["chapterNumber"] {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let groups = content["sceneGroups"].as_array().ok_or_else(invalid)?;
    let mut mapped = false;
    let mut texts = Vec::new();
    for group in groups {
        let text = group["body"].as_str().ok_or_else(invalid)?.trim();
        texts.push(text);
        match &group["sceneId"] {
            Value::Null => {}
            Value::String(id) if ids.contains(id.as_str()) => mapped |= !text.is_empty(),
            _ => return Err(invalid()),
        }
    }
    if !mapped
        || content["body"].as_str().map(str::trim) != Some(texts.join("\n\n---\n\n").as_str())
    {
        return Err(invalid());
    }
    Ok(())
}
