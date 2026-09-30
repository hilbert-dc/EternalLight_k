use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::model_sources::{ModelSourceScope, validated_source};
use crate::story_events::EventClaimRef;
use crate::{ProjectId, RepositoryError};

/// Resolve source-qualified prerequisites in the caller's fixed visibility scope.
pub(crate) fn resolve(
    connection: &Connection,
    project_id: &ProjectId,
    scope: ModelSourceScope<'_>,
    batch_id: &str,
    event: &Value,
    entities: &[Value],
) -> Result<HashMap<EventClaimRef, Value>, RepositoryError> {
    let references = crate::story_events::preconditions(event)?;
    let mut resolved = HashMap::new();
    if references
        .iter()
        .all(|reference| reference.source_batch_id.is_none())
    {
        return Ok(resolved);
    }
    let current = matches!(scope, ModelSourceScope::Current);
    let chapter: Option<i64> = connection.query_row(
        "SELECT CASE WHEN ?3 THEN c.number ELSE json_extract(r.content_json,'$.chapterNumber') END FROM story_modeling_batches_v2 b \
         JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.object_id=b.object_id AND r.revision_id=b.source_revision_id \
         LEFT JOIN story_chapters_v2 c ON c.project_id=b.project_id AND b.object_id='draft-'||c.chapter_id \
         WHERE b.project_id=?1 AND b.batch_id=?2",
        params![project_id.as_str(),batch_id,current],|row| row.get(0),
    ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?.flatten();
    let chapter = chapter.ok_or(RepositoryError::CorruptState)?;
    let mut sources = HashMap::new();
    for reference in references {
        let Some(source_id) = &reference.source_batch_id else {
            continue;
        };
        if source_id == batch_id {
            return Err(invalid());
        }
        if !sources.contains_key(source_id) {
            let source =
                validated_source(connection, project_id, scope, source_id)?.ok_or_else(invalid)?;
            if source.chapter_number.is_none_or(|prior| prior >= chapter) {
                return Err(invalid());
            }
            if source.entities.iter().any(|source_entity| {
                entities.iter().any(|entity| {
                    entity["id"] == source_entity["id"] && entity["kind"] != source_entity["kind"]
                })
            }) {
                return Err(RepositoryError::InvalidContent(
                    "event prerequisite source has a conflicting entity kind for an exact shared identity".into(),
                ));
            }
            sources.insert(source_id.clone(), source);
        }
        let claim = sources[source_id]
            .assertions
            .get(reference.assertion_index)
            .ok_or_else(invalid)?;
        crate::story_events::validate_reference_subject(event, claim)?;
        resolved.insert(reference, claim.clone());
    }
    Ok(resolved)
}

pub(crate) fn validate_batch(
    connection: &Connection,
    project_id: &ProjectId,
    scope: ModelSourceScope<'_>,
    batch_id: &str,
    assertions: &[Value],
    entities: &[Value],
) -> Result<(), RepositoryError> {
    if !assertions.iter().any(|claim| claim["kind"] == "event") {
        return Ok(());
    }
    // Validate this event's entire bound payload as strictly as every external source.
    // A queued publication supplies its new candidates, not the batch's empty stored arrays.
    let content_json: Option<String> = connection.query_row(
        "SELECT r.content_json FROM story_modeling_batches_v2 b JOIN story_revisions_v2 r \
         ON r.project_id=b.project_id AND r.object_id=b.object_id AND r.revision_id=b.source_revision_id \
         AND r.object_kind='draft' WHERE b.project_id=?1 AND b.batch_id=?2",
        params![project_id.as_str(), batch_id], |row|row.get(0),
    ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let content: Value = serde_json::from_str(
        content_json
            .as_deref()
            .ok_or(RepositoryError::CorruptState)?,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    let chars = content["body"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?
        .chars()
        .collect::<Vec<_>>();
    crate::modeling::validate_payload(entities, assertions, &chars)
        .map_err(|_| RepositoryError::CorruptState)?;
    for event in assertions.iter().filter(|claim| claim["kind"] == "event") {
        resolve(connection, project_id, scope, batch_id, event, entities)?;
    }
    Ok(())
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("event prerequisite must reference an exact non-event claim in an authorized prior chapter; use null for this batch".into())
}

#[cfg(test)]
#[path = "event_sources_tests.rs"]
mod tests;
