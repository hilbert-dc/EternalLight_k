use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::Value;

use crate::{ProjectId, RepositoryError, TaskRecord};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SceneSelection {
    #[serde(rename = "type")]
    kind: String,
    source_revision_id: String,
    group_ids: Vec<String>,
}

pub(crate) fn selection(target: &Value) -> Result<SceneSelection, RepositoryError> {
    let scope: SceneSelection = serde_json::from_value(target["scope"].clone())
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    if scope.kind != "selectedScenes"
        || scope.source_revision_id.is_empty()
        || scope.source_revision_id.len() > 160
        || scope.group_ids.is_empty()
        || scope.group_ids.len() > 16
        || scope
            .group_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 160)
        || scope.group_ids.iter().collect::<HashSet<_>>().len() != scope.group_ids.len()
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(scope)
}

pub(crate) fn validate_target(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
) -> Result<(), RepositoryError> {
    if target["scope"]["type"] != "selectedScenes" {
        return Ok(());
    }
    let scope = selection(target)?;
    let chapter = target["chapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let current: Option<(String, bool)> = db.query_row(
        "SELECT COALESCE(candidate_revision_id, accepted_revision_id), locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
        params![project.as_str(), format!("draft-{chapter}")],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(storage_error)?;
    if current.as_ref().map(|(id, _)| id.as_str()) != Some(scope.source_revision_id.as_str()) {
        return Err(RepositoryError::TaskInputStale);
    }
    // Generation authority is checked by claim_generation_run_for_task against the
    // persisted decision and current chapter; narrowing to selected scenes adds no authority.
    if current.is_some_and(|(_, locked)| locked) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let base = load_base(db, project, target, &scope)?;
    if base["chapterNumber"] != target["chapterNumber"] {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let groups = base["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let mut ids = HashSet::new();
    for group in groups {
        let id = group["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        if !ids.insert(id)
            || (scope.group_ids.iter().any(|selected| selected == id) && group["locked"] != false)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    if scope.group_ids.iter().any(|id| !ids.contains(id.as_str())) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn validate_write(
    db: &Connection,
    task: &TaskRecord,
    target: &Value,
    content: &str,
) -> Result<(), RepositoryError> {
    let scope = selection(target)?;
    let base = load_base(db, &task.project_id, target, &scope)?;
    let output: Value =
        serde_json::from_str(content).map_err(|_| RepositoryError::TaskOutputForbidden)?;
    for key in ["chapterId", "chapterNumber", "title", "sourcePlanRefs"] {
        if output[key] != base[key] {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    let before = base["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let after = output["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::TaskOutputForbidden)?;
    if before.len() != after.len() {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    for (old, new) in before.iter().zip(after) {
        let mut expected = old.clone();
        if old["id"]
            .as_str()
            .is_some_and(|id| scope.group_ids.iter().any(|selected| selected == id))
        {
            if old["locked"] != false || new["body"].as_str().is_none() {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            expected["body"] = new["body"].clone();
        }
        if expected != *new {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    let body = after
        .iter()
        .map(|group| group["body"].as_str().map(str::trim))
        .collect::<Option<Vec<_>>>()
        .ok_or(RepositoryError::TaskOutputForbidden)?
        .join("\n\n---\n\n");
    if output["body"].as_str() != Some(body.as_str()) {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    Ok(())
}

fn load_base(
    db: &Connection,
    project: &ProjectId,
    target: &Value,
    scope: &SceneSelection,
) -> Result<Value, RepositoryError> {
    let chapter = target["chapterId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let content: String = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3",
        params![project.as_str(), format!("draft-{chapter}"), scope.source_revision_id],
        |row| row.get(0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
