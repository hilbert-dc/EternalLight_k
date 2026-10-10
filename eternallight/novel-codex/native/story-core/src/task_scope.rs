use serde_json::Value;

use crate::{RepositoryError, TaskRecord};

/// Reading is independent of mutation authority. Absent an explicit reading
/// fence, writers may inspect their current chapter and earlier chapters.
pub fn reader_scene_limit(target: &Value) -> Result<Option<usize>, RepositoryError> {
    let Some(scope) = target.get("readScope") else { return Ok(None); };
    let fields = scope.as_object().ok_or(RepositoryError::TaskBindingInvalid)?;
    match scope["type"].as_str() {
        Some("throughChapter") if fields.len() == 1 => Ok(None),
        Some("throughScene") if fields.len() == 2 => scope["endSceneOrder"].as_u64()
            .filter(|n| (1..=100).contains(n)).map(|n| Some(n as usize))
            .ok_or(RepositoryError::TaskBindingInvalid),
        _ => Err(RepositoryError::TaskBindingInvalid),
    }
}

/// Returns the authorized scene prefix for a Writer task, or `None` for a whole chapter.
pub fn writer_scene_limit(target: &Value) -> Result<Option<usize>, RepositoryError> {
    reader_scene_limit(target)?;
    let scope = &target["scope"];
    if scope.is_null() {
        return Ok(None);
    }
    if !scope.is_object() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    match scope["type"].as_str() {
        Some("wholeChapter") => Ok(None),
        Some("selectedScenes") => {
            crate::scene_revision::selection(target)?;
            Ok(None)
        }
        Some("throughScene") => {
            let end = scope["endSceneOrder"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|order| (1..=100).contains(order))
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            if target["sceneOrder"].as_u64() != Some(end as u64) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            Ok(Some(end))
        }
        Some(_) | None => Err(RepositoryError::TaskBindingInvalid),
    }
}

pub(crate) fn validate_scoped_write(
    connection: &rusqlite::Connection,
    task: &TaskRecord,
    content_json: &str,
) -> Result<(), RepositoryError> {
    if task.operation == "generatePlan" {
        crate::plan_contract::validate_generated_plan(content_json)?;
        return crate::plan_commitments::validate_links(connection, task, content_json);
    }
    if task.operation == "assessSceneOutcome" {
        return crate::scene_assessment::validate_scene_assessment_write(
            connection,
            task,
            content_json,
        );
    }
    if task.operation == "assessRevisionOutcome" {
        return crate::revision_outcome_validation::validate_write(connection, task, content_json);
    }
    if task.operation == "extractCommitments" {
        return crate::commitment_extraction::validate_task_write(connection, task, content_json);
    }
    if task.operation == "reviseCommitment" {
        return crate::commitment_revision::validate_write(connection, task, content_json);
    }
    if task.operation != "generateChapter" {
        return Ok(());
    }
    crate::protected_scenes::validate_write(connection, task, content_json)?;
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if target["scope"]["type"] == "selectedScenes" {
        return crate::scene_revision::validate_write(connection, task, &target, content_json);
    }
    let Some(end) = writer_scene_limit(&target)? else {
        return Ok(());
    };
    let output: Value =
        serde_json::from_str(content_json).map_err(|_| RepositoryError::CorruptState)?;
    let groups = output["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::TaskOutputForbidden)?;
    let joined = groups
        .iter()
        .map(|group| group["body"].as_str().map(str::trim))
        .collect::<Option<Vec<_>>>()
        .ok_or(RepositoryError::TaskOutputForbidden)?
        .join("\n\n---\n\n");
    if output["body"].as_str() != Some(joined.as_str())
        || output["chapterNumber"] != target["chapterNumber"]
    {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    if let Some(previous) = crate::protected_scenes::draft_base(connection, task)? {
        let prior_groups = previous["sceneGroups"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        if output["title"] != previous["title"]
            || output["sourcePlanRefs"] != previous["sourcePlanRefs"]
            || groups.get(end..) != prior_groups.get(end..)
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    } else if groups.len() > end {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    Ok(())
}
