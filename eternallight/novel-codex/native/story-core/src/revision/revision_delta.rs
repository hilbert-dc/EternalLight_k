use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, RevisionId, SqliteStoryRepository};

const MAX_SCENE_CHANGES: usize = 100;
const EXCERPT_CHARS: usize = 120;



fn compare_scenes(from: &Value, to: &Value) -> Result<(Vec<Value>, bool, bool), RepositoryError> {
    let (Some(from_groups), Some(to_groups)) =
        (from["sceneGroups"].as_array(), to["sceneGroups"].as_array())
    else {
        return Ok((Vec::new(), false, false));
    };
    let map = |groups: &[Value]| -> Result<BTreeMap<String, (usize, String)>, RepositoryError> {
        let mut scenes = BTreeMap::new();
        for (order, group) in groups.iter().enumerate() {
            let id = group["sceneId"]
                .as_str()
                .or_else(|| group["id"].as_str())
                .filter(|id| !id.is_empty())
                .ok_or(RepositoryError::CorruptState)?;
            let body = group["body"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            if scenes
                .insert(id.to_owned(), (order, body.to_owned()))
                .is_some()
            {
                return Err(RepositoryError::CorruptState);
            }
        }
        Ok(scenes)
    };
    let before = map(from_groups)?;
    let after = map(to_groups)?;
    let ids = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    let mut truncated = false;
    for id in ids {
        let previous = before.get(id);
        let current = after.get(id);
        let status = match (previous, current) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some((old_order, old_body)), Some((new_order, new_body)))
                if old_order != new_order && old_body != new_body =>
            {
                "movedAndEdited"
            }
            (Some((old_order, _)), Some((new_order, _))) if old_order != new_order => "moved",
            (Some((_, old_body)), Some((_, new_body))) if old_body != new_body => "edited",
            (Some(_), Some(_)) => continue,
            (None, None) => return Err(RepositoryError::CorruptState),
        };
        if changes.len() == MAX_SCENE_CHANGES {
            truncated = true;
            break;
        }
        changes.push(json!({"sceneId": id, "status": status,
            "fromOrder": previous.map(|(order, _)| order + 1),
            "toOrder": current.map(|(order, _)| order + 1)}));
    }
    Ok((changes, true, truncated))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "revision_delta_tests.rs"]
mod tests;

/// Public revision delta capability; SQLite retains transaction ownership.
pub trait RevisionDeltaRepository {
/// Compare two immutable revisions of the same draft without inferring story semantics.
    fn draft_revision_delta(
        &self,
        project_id: &ProjectId,
        from_revision_id: &RevisionId,
        to_revision_id: &RevisionId,
    ) -> Result<Value, RepositoryError>;
}

impl RevisionDeltaRepository for SqliteStoryRepository {
/// Compare two immutable revisions of the same draft without inferring story semantics.
    fn draft_revision_delta(
        &self,
        project_id: &ProjectId,
        from_revision_id: &RevisionId,
        to_revision_id: &RevisionId,
    ) -> Result<Value, RepositoryError> {
        let load = |revision_id: &RevisionId| -> Result<(String, Value), RepositoryError> {
            let record = self
                .connection
                .query_row(
                    "SELECT object_id, object_kind, content_json FROM story_revisions_v2 WHERE project_id = ?1 AND revision_id = ?2",
                    params![project_id.as_str(), revision_id.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(storage_error)?
                .ok_or(RepositoryError::CandidateNotFound)?;
            if record.1 != "draft" {
                return Err(RepositoryError::InvalidObjectKind);
            }
            let value =
                serde_json::from_str(&record.2).map_err(|_| RepositoryError::CorruptState)?;
            Ok((record.0, value))
        };
        let (from_object, from) = load(from_revision_id)?;
        let (to_object, to) = load(to_revision_id)?;
        if from_object != to_object {
            return Err(RepositoryError::InvalidContent(
                "draft revision pair must belong to the same object".into(),
            ));
        }
        let from_body = from["body"].as_str().ok_or(RepositoryError::CorruptState)?;
        let to_body = to["body"].as_str().ok_or(RepositoryError::CorruptState)?;
        let from_chars = from_body.chars().collect::<Vec<_>>();
        let to_chars = to_body.chars().collect::<Vec<_>>();
        let prefix = from_chars
            .iter()
            .zip(&to_chars)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix = from_chars[prefix..]
            .iter()
            .rev()
            .zip(to_chars[prefix..].iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        let from_end = from_chars.len() - suffix;
        let to_end = to_chars.len() - suffix;
        let body_changed = prefix != from_end || prefix != to_end;
        let (scene_changes, scene_diff_available, scenes_truncated) = compare_scenes(&from, &to)?;
        Ok(json!({
            "projectId": project_id.as_str(), "objectId": from_object,
            "fromRevisionId": from_revision_id.as_str(),
            "toRevisionId": to_revision_id.as_str(),
            "bodyChanged": body_changed,
            "bodyDelta": if body_changed { Some(json!({
                "fromRange": {"start": prefix, "end": from_end},
                "toRange": {"start": prefix, "end": to_end},
                "fromExcerpt": from_chars[prefix..from_end].iter().take(EXCERPT_CHARS).collect::<String>(),
                "toExcerpt": to_chars[prefix..to_end].iter().take(EXCERPT_CHARS).collect::<String>(),
                "excerptTruncated": from_end - prefix > EXCERPT_CHARS || to_end - prefix > EXCERPT_CHARS
            })) } else { None },
            "chapterNumberChanged": from["chapterNumber"] != to["chapterNumber"],
            "titleChanged": from["title"] != to["title"],
            "sceneChanges": scene_changes,
            "coverage": {"sceneDiffAvailable": scene_diff_available,
                "scenesTruncated": scenes_truncated,
                "semanticChangeInferred": false}
        }))
    }
}
