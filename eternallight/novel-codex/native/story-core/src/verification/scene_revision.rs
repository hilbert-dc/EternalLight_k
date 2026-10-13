use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, TaskRecord};

#[derive(Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SceneSelection {
    #[serde(rename = "type")]
    kind: String,
    source_revision_id: String,
    group_ids: Vec<String>,
    body_ranges: Option<Vec<BodyRange>>,
}

#[derive(Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BodyRange {
    group_id: String,
    start: usize,
    end: usize,
}

fn outside_unchanged(before: &str, after: &str, range: &BodyRange) -> bool {
    let before: Vec<char> = before.chars().collect();
    let after: Vec<char> = after.chars().collect();
    range.start <= range.end && range.end <= before.len()
        && after.len() >= range.start + before.len() - range.end
        && before[..range.start] == after[..range.start]
        && before[range.end..] == after[after.len() - (before.len() - range.end)..]
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
    if let Some(ranges) = &scope.body_ranges {
        if ranges.len() != scope.group_ids.len()
            || ranges.iter().map(|r| &r.group_id).collect::<HashSet<_>>().len() != ranges.len()
            || ranges.iter().any(|r| r.start > r.end || !scope.group_ids.contains(&r.group_id))
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
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
    validate_selection_groups(&base, &scope)
}

fn validate_selection_groups(base: &Value, scope: &SceneSelection) -> Result<(), RepositoryError> {
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
    if let Some(ranges) = &scope.body_ranges {
        for range in ranges {
            let body = groups.iter().find(|g| g["id"] == range.group_id)
                .and_then(|g| g["body"].as_str()).ok_or(RepositoryError::TaskBindingInvalid)?;
            if range.end > body.chars().count() {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
    }
    Ok(())
}

/// Load only the source captured when this selected-scene task was registered.
/// Reconciliation must not reinterpret its immutable selection using today's head.
pub(crate) fn frozen_scoped_base(
    db: &Connection,
    task: &TaskRecord,
    target: &Value,
    object: &str,
    frozen_candidate: Option<&str>,
) -> Result<Value, RepositoryError> {
    let scope = selection(target)?;
    let chapter = target["chapterId"].as_str().ok_or(RepositoryError::TaskBindingInvalid)?;
    if object != format!("draft-{chapter}") {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let frozen: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_task_input_heads_v2 i
         JOIN story_task_source_heads_v2 s ON s.task_id=i.task_id AND s.object_id=i.object_id
         WHERE i.task_id=?1 AND i.object_id=?2 AND s.object_kind='draft' AND s.locked=0
           AND COALESCE(i.candidate_revision_id,i.accepted_revision_id)=?3
           AND i.candidate_revision_id IS ?4
           AND i.candidate_revision_id IS s.candidate_revision_id
           AND i.accepted_revision_id IS s.accepted_revision_id
           AND i.working_revision_id IS s.working_revision_id
           AND i.working_selection_revision IS s.working_selection_revision)",
        params![task.task_id, object, scope.source_revision_id, frozen_candidate], |row| row.get(0),
    ).map_err(storage_error)?;
    if !frozen {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let base = load_base(db, &task.project_id, target, &scope)?;
    if base["chapterId"] != chapter || base["chapterNumber"] != target["chapterNumber"] {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    validate_selection_groups(&base, &scope)?;
    let bodies = base["sceneGroups"].as_array().ok_or(RepositoryError::CorruptState)?
        .iter().map(|group| group["body"].as_str()).collect::<Option<Vec<_>>>()
        .ok_or(RepositoryError::CorruptState)?;
    if base["body"].as_str().map(str::trim) != Some(crate::scene_body::canonical_scene_body(bodies).trim()) {
        return Err(RepositoryError::CorruptState);
    }
    Ok(base)
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
            if let Some(ranges) = &scope.body_ranges {
                let range = ranges.iter().find(|r| old["id"] == r.group_id)
                    .ok_or(RepositoryError::TaskOutputForbidden)?;
                if !outside_unchanged(old["body"].as_str().ok_or(RepositoryError::CorruptState)?,
                    new["body"].as_str().ok_or(RepositoryError::TaskOutputForbidden)?, range) {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
            }
            expected["body"] = new["body"].clone();
        }
        if expected != *new {
            return Err(RepositoryError::TaskOutputForbidden);
        }
    }
    let bodies = after
        .iter()
        .map(|group| group["body"].as_str())
        .collect::<Option<Vec<_>>>()
        .ok_or(RepositoryError::TaskOutputForbidden)?;
    let body = crate::scene_body::canonical_scene_body(bodies);
    if output["body"].as_str() != Some(body.as_str()) {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    Ok(())
}

/// Rebase only a proven replacement of the originally authorized text. Changed
/// protected context is a conflict, never a reason to reuse stale coordinates.
pub(crate) fn rebase_ranges(db: &Connection, project: &ProjectId, target: &mut Value, revision: &str) -> Result<(), RepositoryError> {
    if target["scope"].get("bodyRanges").is_none() { return Ok(()); }
    let mut scope = selection(target)?;
    let before = load_base(db, project, target, &scope)?;
    validate_selection_groups(&before, &scope)?;
    scope.source_revision_id = revision.into();
    let after = load_base(db, project, target, &scope)?;
    for key in ["chapterId", "chapterNumber", "title", "sourcePlanRefs"] {
        if before[key] != after[key] { return Err(RepositoryError::TaskInputStale); }
    }
    let old = before["sceneGroups"].as_array().ok_or(RepositoryError::CorruptState)?;
    let new = after["sceneGroups"].as_array().ok_or(RepositoryError::CorruptState)?;
    if old.len() != new.len() { return Err(RepositoryError::TaskInputStale); }
    let ranges = scope.body_ranges.as_mut().ok_or(RepositoryError::TaskBindingInvalid)?;
    for (a,b) in old.iter().zip(new) {
        let mut expected = a.clone();
        if let Some(range) = ranges.iter_mut().find(|r| a["id"] == r.group_id) {
            let ab = a["body"].as_str().ok_or(RepositoryError::CorruptState)?;
            let bb = b["body"].as_str().ok_or(RepositoryError::CorruptState)?;
            if !outside_unchanged(ab, bb, range) { return Err(RepositoryError::TaskInputStale); }
            range.end = bb.chars().count() - (ab.chars().count() - range.end);
            expected["body"] = b["body"].clone();
        }
        if expected != *b { return Err(RepositoryError::TaskInputStale); }
    }
    target["scope"]["sourceRevisionId"] = revision.into();
    target["scope"]["bodyRanges"] = serde_json::to_value(ranges).map_err(|_| RepositoryError::CorruptState)?;
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
        "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3 AND object_kind='draft'",
        params![project.as_str(), format!("draft-{chapter}"), scope.source_revision_id],
        |row| row.get(0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
mod range_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn unicode_replacement_preserves_exact_context_and_rebases_only_proven_changes() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_revisions_v2(project_id TEXT,object_id TEXT,revision_id TEXT,object_kind TEXT,content_json TEXT);").unwrap();
        for (id,body) in [("before","甲🙂。旧句。尾。"),("after","甲🙂。新句更长。尾。"),("conflict","甲🙂！新句更长。尾。")] {
            let value=json!({"chapterId":"c","chapterNumber":1,"title":"章","sourcePlanRefs":{},"sceneGroups":[{"id":"a","body":body,"locked":false}]});
            db.execute("INSERT INTO story_revisions_v2 VALUES('p','draft-c',?1,'draft',?2)",params![id,value.to_string()]).unwrap();
        }
        let source=json!({"chapterId":"c","scope":{"type":"selectedScenes","sourceRevisionId":"before","groupIds":["a"],"bodyRanges":[{"groupId":"a","start":3,"end":6}]}});
        let mut target=source.clone();
        rebase_ranges(&db,&ProjectId::parse("p").unwrap(),&mut target,"after").unwrap();
        assert_eq!(target["scope"]["bodyRanges"][0],json!({"groupId":"a","start":3,"end":8}));
        assert!(rebase_ranges(&db,&ProjectId::parse("p").unwrap(),&mut source.clone(),"conflict").is_err());
    }
}
