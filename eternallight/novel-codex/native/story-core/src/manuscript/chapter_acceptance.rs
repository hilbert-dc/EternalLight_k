use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{RepositoryError, ReviewRecord, RevisionId, RevisionRecord};

/// Direct chapter review carries author UI decisions as audit data. Proposal-set choices
/// deliberately expose only scene and fact selections; neither path lets UI metadata alter scope.
pub(super) fn validate_choices(
    connection: &Connection,
    review: &ReviewRecord,
    choices: &Value,
) -> Result<(), RepositoryError> {
    let fields = choices
        .as_object()
        .ok_or_else(|| RepositoryError::InvalidContent("invalid review selections".into()))?;
    if choices.to_string().len() > 16_000
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "sceneGroupIds"
                    | "factDecisions"
                    | "noConfirmedChanges"
                    | "factDependencyLinks"
                    | "directionChangeDecisions"
                    | "manualDirectionDecisions"
                    | "modelingBatchId"
            )
        })
    {
        return Err(RepositoryError::InvalidContent(
            "invalid chapter review choice fields".into(),
        ));
    }
    if fields
        .get("noConfirmedChanges")
        .is_some_and(|value| !value.is_boolean())
        || fields.get("factDependencyLinks").is_some_and(|value| {
            value
                .as_array()
                .is_none_or(|links| links.len() > 200 || links.iter().any(|link| !link.is_object()))
        })
        || ["directionChangeDecisions", "manualDirectionDecisions"]
            .iter()
            .any(|key| fields.get(*key).is_some_and(|value| !value.is_object()))
        || fields.get("modelingBatchId").is_some_and(|value| {
            value
                .as_str()
                .is_none_or(|id| id.is_empty() || id.len() > 160)
        })
        || (choices["noConfirmedChanges"] == true
            && choices["factDecisions"]
                .as_object()
                .is_some_and(|decisions| {
                    decisions.values().any(|decision| decision == "confirmed")
                }))
    {
        return Err(RepositoryError::InvalidContent(
            "invalid chapter review audit decisions".into(),
        ));
    }
    let selections = fields
        .iter()
        .filter(|(key, _)| matches!(key.as_str(), "sceneGroupIds" | "factDecisions"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    crate::proposal_selection::validate_choices(connection, review, &Value::Object(selections))
}

fn load(
    connection: &Connection,
    review: &ReviewRecord,
    revision: &str,
) -> Result<Value, RepositoryError> {
    let content: String = connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3 AND object_kind='draft'",
        params![review.project_id.as_str(), review.object_id.as_str(), revision], |row| row.get(0),
    ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn candidate_and_base(
    connection: &Connection,
    review: &ReviewRecord,
    previous: Option<&str>,
) -> Result<(Value, Option<Value>), RepositoryError> {
    Ok((
        load(connection, review, review.revision_id.as_str())?,
        previous
            .map(|id| load(connection, review, id))
            .transpose()?,
    ))
}

fn groups(content: &Value) -> Result<&Vec<Value>, RepositoryError> {
    let groups = content["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let mut seen = BTreeSet::new();
    let mut identities = BTreeSet::new();
    if groups.len() > 100
        || groups.iter().any(|group| {
            let id = group["id"].as_str().unwrap_or("");
            id.is_empty()
                || id.len() > 160
                || !seen.insert(id)
                || !identities.insert(scene_key(group))
                || group["body"].as_str().is_none()
        })
    {
        return Err(RepositoryError::CorruptState);
    }
    Ok(groups)
}

fn scene_key(group: &Value) -> &str {
    group["sceneId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| group["id"].as_str().unwrap_or(""))
}

/// All candidate scenes selected means whole-chapter adoption, including metadata and deletions.
/// Selected scenes use candidate order; unselected scenes retain their values and relative order.
/// IDs present only in the formal revision select deletions. Missing selection means whole chapter.
pub(crate) fn selected_content(
    candidate: &Value,
    base: Option<&Value>,
    choices: &Value,
) -> Result<Value, RepositoryError> {
    if base.is_some_and(|base| {
        base["chapterNumber"] != candidate["chapterNumber"]
            || base["chapterId"] != candidate["chapterId"]
    }) {
        return Err(RepositoryError::InvalidChapter);
    }
    let Some(selected) = choices.get("sceneGroupIds") else {
        if let Some(before) = base.and_then(|base| base["sceneGroups"].as_array())
            && before.iter().any(|group| group["locked"] == true)
        {
            validate_locked(before, groups(candidate)?)?;
        }
        return Ok(candidate.clone());
    };
    let selected = selected
        .as_array()
        .ok_or_else(|| RepositoryError::InvalidContent("invalid scene selection".into()))?;
    let after = groups(candidate)?;
    let before = base.map(groups).transpose()?.cloned().unwrap_or_default();
    let mut identities = BTreeMap::new();
    for group in before.iter().chain(after) {
        let id = group["id"].as_str().ok_or(RepositoryError::CorruptState)?;
        if identities
            .insert(id, scene_key(group))
            .is_some_and(|old| old != scene_key(group))
        {
            return Err(RepositoryError::CorruptState);
        }
    }
    let known = before
        .iter()
        .chain(after)
        .filter_map(|group| group["id"].as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    for value in selected {
        let id = value
            .as_str()
            .ok_or_else(|| RepositoryError::InvalidContent("invalid scene id".into()))?;
        if !known.contains(id) || !ids.insert(id) {
            return Err(RepositoryError::InvalidContent(
                "unknown or repeated source scene".into(),
            ));
        }
    }
    if ids.is_empty() {
        return Err(RepositoryError::InvalidContent(
            "select at least one scene".into(),
        ));
    }
    if !after.is_empty()
        && after
            .iter()
            .all(|group| ids.contains(group["id"].as_str().unwrap()))
    {
        validate_locked(&before, after)?;
        return Ok(candidate.clone());
    }
    let selected_keys = before
        .iter()
        .chain(after)
        .filter(|group| ids.contains(group["id"].as_str().unwrap()))
        .map(scene_key)
        .collect::<BTreeSet<_>>();
    let mut merged = before
        .iter()
        .filter(|group| !selected_keys.contains(scene_key(group)))
        .cloned()
        .collect::<Vec<_>>();
    let mut last_selected = None;
    for (index, group) in after
        .iter()
        .enumerate()
        .filter(|(_, group)| selected_keys.contains(scene_key(group)))
    {
        let predecessor = after[..index].iter().rev().find_map(|anchor| {
            merged
                .iter()
                .position(|item| scene_key(item) == scene_key(anchor))
        });
        let successor = after[index + 1..].iter().find_map(|anchor| {
            merged
                .iter()
                .position(|item| scene_key(item) == scene_key(anchor))
        });
        let position = predecessor
            .map(|pos| pos + 1)
            .unwrap_or_else(|| successor.unwrap_or(merged.len()));
        let position = last_selected.map_or(position, |pos| position.max(pos + 1));
        merged.insert(position, group.clone());
        last_selected = Some(position);
    }
    validate_locked(&before, &merged)?;
    if merged.len() > 100 {
        return Err(RepositoryError::InvalidContent(
            "merged chapter has too many scenes".into(),
        ));
    }
    let mut result = base.unwrap_or(candidate).clone();
    if base.is_none() {
        // A partial first adoption cannot claim whole-chapter plan lineage.
        result["sourcePlanRefs"] = json!({"lineageStatus":"current","localOnly":true});
    }
    result["sceneGroups"] = json!(merged);
    result["body"] = json!(
        merged
            .iter()
            .map(|group| group["body"].as_str().unwrap().trim())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    );
    Ok(result)
}

fn validate_locked(before: &[Value], after: &[Value]) -> Result<(), RepositoryError> {
    if before
        .iter()
        .enumerate()
        .any(|(index, group)| group["locked"] == true && after.get(index) != Some(group))
    {
        return Err(RepositoryError::ObjectLocked);
    }
    Ok(())
}

pub(super) fn materialize(
    connection: &Connection,
    review: &ReviewRecord,
    previous: Option<&str>,
    choices: &Value,
) -> Result<RevisionId, RepositoryError> {
    let (candidate, base) = candidate_and_base(connection, review, previous)?;
    let mut result = selected_content(&candidate, base.as_ref(), choices)?;
    if result == candidate {
        return Ok(review.revision_id.clone());
    }
    result["version"] = json!(
        candidate["version"]
            .as_u64()
            .unwrap_or(0)
            .max(
                base.as_ref()
                    .and_then(|base| base["version"].as_u64())
                    .unwrap_or(0)
            )
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?
    );
    result["baseProjectRevision"] = json!(review.project_revision);
    result["source"] = json!("author");
    if base.is_none() {
        result["completeness"] = json!("partial");
    }
    let content_json = result.to_string();
    let revision = RevisionRecord {
        project_id: review.project_id.clone(),
        object_id: review.object_id.clone(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(format!("revision-{}", uuid::Uuid::new_v4().simple()))
            .map_err(|_| RepositoryError::CorruptState)?,
        parent_revision_id: previous
            .map(RevisionId::parse)
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?,
        content_digest: format!("{:x}", Sha256::digest(content_json.as_bytes())),
        content_json,
    };
    connection.execute(
        "INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,parent_revision_id,content_digest,content_json) VALUES(?1,?2,?3,'draft',?4,?5,?6)",
        params![revision.revision_id.as_str(),review.project_id.as_str(),review.object_id.as_str(),previous,revision.content_digest,revision.content_json],
    ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    crate::search_index::index_revision(connection, &revision)?;
    Ok(revision.revision_id)
}

