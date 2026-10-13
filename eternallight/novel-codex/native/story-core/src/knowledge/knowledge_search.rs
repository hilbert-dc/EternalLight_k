use std::collections::BTreeSet;

use crate::model_sources::ModelSourceScope;
use crate::proposition_sources::proposition_source_visible;
use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

const MAX_MATCHING_BATCHES: usize = 200;



fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public knowledge search capability; SQLite retains transaction ownership.
pub trait KnowledgeSearchRepository {
    /// Resolve an exact source-bound name before returning evidence from one frozen snapshot.
    /// Ambiguous names never cause assertions from different identities to be merged.
    fn query_knowledge_by_name(
        &self,
        project_id: &ProjectId,
        snapshot_id: &SnapshotId,
        name: &str,
        selected_entity_id: Option<&str>,
        limit: usize,
    ) -> Result<Value, RepositoryError>;
}

impl KnowledgeSearchRepository for SqliteStoryRepository {
    /// Resolve an exact source-bound name before returning evidence from one frozen snapshot.
    /// Ambiguous names never cause assertions from different identities to be merged.
    fn query_knowledge_by_name(
        &self,
        project_id: &ProjectId,
        snapshot_id: &SnapshotId,
        name: &str,
        selected_entity_id: Option<&str>,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if !(1..=40).contains(&limit) {
            return Err(RepositoryError::InvalidContent(
                "invalid knowledge query limit".into(),
            ));
        }
        let catalog =
            self.resolve_entity_name_at_snapshot(project_id, snapshot_id, name, 200_000)?;
        let missing_source_count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM story_snapshot_members_v2 m \
                 JOIN story_revisions_v2 r ON r.revision_id = m.revision_id AND r.project_id = ?1 AND r.object_id = m.object_id \
                 WHERE m.snapshot_id = ?2 AND r.object_kind = 'draft' AND NOT EXISTS (\
                   SELECT 1 FROM story_snapshot_knowledge_v2 k \
                   JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id \
                   WHERE k.snapshot_id = m.snapshot_id AND k.object_id = m.object_id \
                     AND b.source_revision_id = m.revision_id)",
                rusqlite::params![project_id.as_str(), snapshot_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let truncated_catalog = catalog["truncated"].as_bool().unwrap_or(true);
        let options = catalog["entries"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?;
        let candidates = options
            .iter()
            .map(|entry| {
                Ok((
                    entry["id"].as_str().ok_or(RepositoryError::CorruptState)?,
                    entry["canonicalId"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?,
                ))
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
        if selected_entity_id.is_some_and(|id| !candidates.iter().any(|(raw, _)| *raw == id)) {
            return Err(RepositoryError::InvalidContent(
                "selected entity is not a name candidate".into(),
            ));
        }
        let canonical_ids = candidates
            .iter()
            .map(|(_, canonical)| *canonical)
            .collect::<BTreeSet<_>>();
        let requires_disambiguation = selected_entity_id.is_none() && canonical_ids.len() > 1;
        let selected_canonical = selected_entity_id
            .and_then(|selected| {
                candidates
                    .iter()
                    .find(|(raw, _)| *raw == selected)
                    .map(|(_, canonical)| *canonical)
            })
            .or_else(|| canonical_ids.iter().next().copied());
        let identity_revision_id = catalog["identityRevisionId"].clone();
        if truncated_catalog || requires_disambiguation || selected_canonical.is_none() {
            return Ok(json!({
                "snapshotId": snapshot_id.as_str(), "queryName": name,
                "identityRevisionId": identity_revision_id,
                "candidates": options, "results": [],
                "coverage": {"complete": !truncated_catalog && !requires_disambiguation && missing_source_count == 0,
                    "requiresDisambiguation": requires_disambiguation,
                    "missingSourceCount": missing_source_count,
                    "catalogTruncated": truncated_catalog, "candidateBatchCapReached": false,
                    "truncatedMatches": false}
            }));
        }
        let selected_canonical = selected_canonical.expect("checked selected identity");
        let raw_ids = candidates
            .iter()
            .filter_map(|(raw, canonical)| (*canonical == selected_canonical).then_some(*raw))
            .collect::<Vec<_>>();
        let placeholders = vec!["?"; raw_ids.len()].join(", ");
        let sql = format!(
            "WITH matching AS (SELECT DISTINCT batch_id FROM story_entity_claims_v2 WHERE project_id = ? AND entity_id IN ({placeholders})) \
             SELECT b.batch_id, b.object_id, b.source_revision_id, b.assertions_json, \
               CASE WHEN json_type(r.content_json, '$.chapterNumber') = 'integer' THEN json_extract(r.content_json, '$.chapterNumber') END \
             FROM matching x JOIN story_modeling_batches_v2 b ON b.batch_id = x.batch_id AND b.project_id = ? \
             JOIN story_snapshot_knowledge_v2 k ON k.batch_id = b.batch_id AND k.object_id = b.object_id AND k.snapshot_id = ? \
             JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = b.object_id AND m.revision_id = b.source_revision_id \
             JOIN story_revisions_v2 r ON r.revision_id = m.revision_id AND r.project_id = b.project_id AND r.object_id = b.object_id \
             ORDER BY b.object_id, b.batch_id LIMIT ?"
        );
        let mut values = vec![SqlValue::Text(project_id.as_str().to_owned())];
        values.extend(raw_ids.iter().map(|id| SqlValue::Text((*id).to_owned())));
        values.push(SqlValue::Text(project_id.as_str().to_owned()));
        values.push(SqlValue::Text(snapshot_id.as_str().to_owned()));
        values.push(SqlValue::Integer((MAX_MATCHING_BATCHES + 1) as i64));
        let mut statement = self.connection.prepare(&sql).map_err(storage_error)?;
        let batches = statement
            .query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let candidate_batch_cap_reached = batches.len() > MAX_MATCHING_BATCHES;
        let scanned_batches = batches.len().min(MAX_MATCHING_BATCHES);
        let confirmed = self.trace_confirmed_facts(project_id, Some(snapshot_id))?;
        let mut results = Vec::new();
        let mut truncated_matches = false;
        let mut unavailable_proposition_sources = 0;
        for (batch_id, object_id, revision_id, assertions_json, chapter) in
            batches.into_iter().take(MAX_MATCHING_BATCHES)
        {
            let assertions: Vec<Value> = serde_json::from_str(&assertions_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            for (index, assertion) in assertions.into_iter().enumerate() {
                let subject = assertion["subjectId"].as_str();
                let target = assertion["targetId"].as_str();
                let owner = assertion["epistemicOwnerId"].as_str();
                if !raw_ids
                    .iter()
                    .any(|id| Some(*id) == subject || Some(*id) == target || Some(*id) == owner)
                {
                    continue;
                }
                if let Some(source_batch_id) = assertion["propositionSourceBatchId"].as_str() {
                    let proposition_id = assertion["propositionId"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?;
                    if !proposition_source_visible(
                        &self.connection,
                        project_id,
                        ModelSourceScope::Snapshot(snapshot_id),
                        source_batch_id,
                        proposition_id,
                    )? {
                        unavailable_proposition_sources += 1;
                        continue;
                    }
                }
                if results.len() == limit {
                    truncated_matches = true;
                    break;
                }
                let confirmed_id = confirmed.get(&(batch_id.clone(), index));
                results.push(json!({
                    "sourceObjectId": object_id, "sourceRevisionId": revision_id,
                    "sourcePath": "/body",
                    "sourceChapter": chapter, "modelingBatchId": batch_id,
                    "propositionRef": assertion["propositionId"].as_str().map(|id| json!({"batchId": assertion["propositionSourceBatchId"].as_str().unwrap_or(&batch_id), "propositionId": id})),
                    "assertionIndex": index, "assertion": assertion,
                    "evidence": assertion["evidence"],
                    "confirmationState": if confirmed_id.is_some() { "confirmed" } else { "candidate" },
                    "confirmedFactId": confirmed_id
                }));
            }
            if truncated_matches {
                break;
            }
        }
        Ok(json!({
            "snapshotId": snapshot_id.as_str(), "queryName": name,
            "identityRevisionId": identity_revision_id,
            "selectedCanonicalId": selected_canonical,
            "matchedRawIds": raw_ids, "candidates": options, "results": results,
            "coverage": {"complete": !candidate_batch_cap_reached && !truncated_matches && missing_source_count == 0 && unavailable_proposition_sources == 0,
                "requiresDisambiguation": false, "catalogTruncated": false,
                "missingSourceCount": missing_source_count,
                "unavailablePropositionSources": unavailable_proposition_sources,
                "candidateBatchCapReached": candidate_batch_cap_reached,
                "scannedBatches": scanned_batches, "truncatedMatches": truncated_matches}
        }))
    }
}

use crate::EntityIdentityRepository;
