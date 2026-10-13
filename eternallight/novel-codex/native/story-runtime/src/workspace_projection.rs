
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::WorkingBaseline;
use serde_json::Value;
use serde_json::json;

pub(crate) fn artifact_revision(
    repository: &SqliteStoryRepository,
    revision_id: &RevisionId,
) -> Result<Value, RepositoryError> {
    let content = repository
        .revision_content(revision_id)?
        .ok_or(RepositoryError::CorruptState)?;
    let envelope: Value = serde_json::from_str(&content)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    Ok(json!({
        "id": revision_id.as_str(),
        "version": envelope["version"],
        "parentRevisionId": null,
        "baseProjectRevision": envelope["baseProjectRevision"],
        "payload": envelope["payload"],
        "source": envelope["source"],
        "createdAt": null
    }))
}

pub(crate) fn draft_revision(
    repository: &SqliteStoryRepository,
    revision_id: &RevisionId,
) -> Result<Value, RepositoryError> {
    let content = repository
        .revision_content(revision_id)?
        .ok_or(RepositoryError::CorruptState)?;
    let envelope: Value = serde_json::from_str(&content)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    let title = envelope
        .get("title")
        .and_then(Value::as_str)
        .ok_or(RepositoryError::CorruptState)?;
    Ok(json!({
        "id": revision_id.as_str(),
        "chapterId": envelope["chapterId"],
        "title": title,
        "version": envelope["version"],
        "parentRevisionId": null,
        "sourcePlanRefs": envelope["sourcePlanRefs"],
        "baseProjectRevision": envelope["baseProjectRevision"],
        "body": envelope["body"],
        "sceneGroups": envelope["sceneGroups"],
        "completeness": envelope["completeness"],
        "source": envelope["source"],
        "createdAt": null
    }))
}

pub(crate) fn working_baseline_json(selection: &WorkingBaseline) -> Value {
    json!({
        "objectKind": selection.object_kind,
        "objectId": selection.object_id,
        "revisionId": selection.revision_id.as_ref().map(RevisionId::as_str),
        "selectionRevision": selection.selection_revision,
        "selectedAt": selection.selected_at.to_string()
    })
}

use novel_codex_story_core::{ProjectRevisionRepository};
