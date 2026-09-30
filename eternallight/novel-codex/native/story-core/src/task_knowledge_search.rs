use std::collections::BTreeMap;

use rusqlite::{params, params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};

use crate::model_sources::ModelSourceScope;
use crate::proposition_sources::proposition_source_visible;
use crate::relation_graph_index::epistemic_visibility;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

const MAX_NAME_CANDIDATES: usize = 100;
const MAX_NAME_CLAIMS: usize = 300;
const MAX_CANDIDATE_BYTES: usize = 12_000;
const MAX_RESULT_BYTES: usize = 20_000;
const MAX_RESPONSE_BYTES: usize = 32_000;
const MAX_SCANNED_ASSERTIONS: i64 = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterKnowledgeScope<'a> {
    PriorChapters(i64),
    CharacterPov {
        chapter: i64,
        character_id: &'a str,
        at_story_time: Option<i64>,
    },
}

impl WriterKnowledgeScope<'_> {
    fn chapter(self) -> i64 {
        match self {
            Self::PriorChapters(chapter) | Self::CharacterPov { chapter, .. } => chapter,
        }
    }
}

const VISIBLE_SOURCE: &str = "
    JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id AND b.project_id = c.project_id AND b.status = 'ready'
    JOIN story_knowledge_heads_v2 h ON h.project_id = b.project_id AND h.object_id = b.object_id AND h.batch_id = b.batch_id
    JOIN story_object_heads_v2 o ON o.project_id = b.project_id AND o.object_id = b.object_id AND o.object_kind = 'draft'
      AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id)
    JOIN story_chapters_v2 ch ON ch.project_id = b.project_id AND b.object_id = 'draft-' || ch.chapter_id
";

impl SqliteStoryRepository {
    /// Search all prior chapters through source-bound indexes, without a first-N chapter scan.
    pub fn query_writer_knowledge_by_name(
        &self,
        project_id: &ProjectId,
        scope: WriterKnowledgeScope<'_>,
        name: &str,
        selected_entity_id: Option<&str>,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        let normalized = name.trim().to_lowercase();
        let reading_chapter = scope.chapter();
        if reading_chapter < 1
            || normalized.is_empty()
            || normalized.chars().count() > 160
            || !(1..=40).contains(&limit)
            || matches!(scope, WriterKnowledgeScope::CharacterPov { character_id, .. } if character_id.is_empty() || character_id.len() > 160)
        {
            return Err(RepositoryError::InvalidContent(
                "invalid Writer knowledge query".into(),
            ));
        }
        let visible_through = reading_chapter - 1;
        let (total_sources, modeled_sources, pending_sources, pending_truncated) =
            source_coverage(self, project_id, visible_through)?;
        let candidate_sql = format!(
            "SELECT DISTINCT c.entity_id FROM story_entity_names_v2 n
             JOIN story_entity_claims_v2 c ON c.project_id = n.project_id AND c.batch_id = n.batch_id AND c.entity_id = n.entity_id
             {VISIBLE_SOURCE}
             WHERE n.project_id = ?1 AND n.name = ?2 AND ch.number <= ?3
             ORDER BY c.entity_id LIMIT 101"
        );
        let mut statement = self
            .connection
            .prepare(&candidate_sql)
            .map_err(storage_error)?;
        let candidate_rows = statement
            .query_map(
                params![project_id.as_str(), normalized, visible_through],
                |row| row.get::<_, String>(0),
            )
            .map_err(storage_error)?;
        let raw_ids = candidate_rows
            .map(|row| row.map_err(storage_error))
            .collect::<Result<Vec<_>, _>>()?;
        let too_many_ids = raw_ids.len() > MAX_NAME_CANDIDATES;
        let ids = raw_ids
            .into_iter()
            .take(MAX_NAME_CANDIDATES)
            .collect::<Vec<_>>();
        let is_pov = matches!(scope, WriterKnowledgeScope::CharacterPov { .. });
        let selected_is_name_candidate =
            selected_entity_id.is_none_or(|selected| ids.iter().any(|id| id == selected));
        if !selected_is_name_candidate && !is_pov {
            return Err(RepositoryError::InvalidContent(
                "selected entity is not a visible name candidate".into(),
            ));
        }
        let mut by_id: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
        let mut claims_truncated = false;
        let mut candidate_bytes = 0;
        if !ids.is_empty()
            && !too_many_ids
            && matches!(scope, WriterKnowledgeScope::PriorChapters(_))
        {
            let placeholders = vec!["?"; ids.len()].join(", ");
            let sql = format!(
                "SELECT c.entity_id, c.kind, c.display_name, c.aliases_json, c.evidence_json,
                        b.object_id, b.source_revision_id FROM story_entity_names_v2 n
                 JOIN story_entity_claims_v2 c ON c.project_id = n.project_id AND c.batch_id = n.batch_id AND c.entity_id = n.entity_id
                 {VISIBLE_SOURCE}
                 WHERE n.project_id = ? AND n.name = ? AND ch.number <= ? AND c.entity_id IN ({placeholders})
                 ORDER BY c.entity_id, ch.number, b.batch_id LIMIT 301"
            );
            let mut args = vec![
                SqlValue::Text(project_id.as_str().to_owned()),
                SqlValue::Text(normalized.clone()),
                SqlValue::Integer(visible_through),
            ];
            args.extend(ids.iter().cloned().map(SqlValue::Text));
            let mut statement = self.connection.prepare(&sql).map_err(storage_error)?;
            let rows = statement
                .query_map(params_from_iter(args), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })
                .map_err(storage_error)?;
            for (claim_count, row) in rows.enumerate() {
                let (id, kind, display_name, aliases, evidence, object_id, revision_id) =
                    row.map_err(storage_error)?;
                if claim_count == MAX_NAME_CLAIMS {
                    claims_truncated = true;
                    break;
                }
                let aliases: Value =
                    serde_json::from_str(&aliases).map_err(|_| RepositoryError::CorruptState)?;
                let evidence: Value =
                    serde_json::from_str(&evidence).map_err(|_| RepositoryError::CorruptState)?;
                if display_name.trim().to_lowercase() != normalized
                    && !aliases.as_array().is_some_and(|aliases| {
                        aliases.iter().any(|alias| {
                            alias
                                .as_str()
                                .is_some_and(|alias| alias.trim().to_lowercase() == normalized)
                        })
                    })
                {
                    return Err(RepositoryError::CorruptState);
                }
                let claim = json!({"displayName": display_name, "aliases": aliases,
                    "sourceObjectId": object_id, "sourceRevisionId": revision_id, "evidence": evidence});
                let claim_bytes = claim.to_string().len();
                if candidate_bytes + claim_bytes > MAX_CANDIDATE_BYTES {
                    claims_truncated = true;
                    break;
                }
                candidate_bytes += claim_bytes;
                let entry = by_id
                    .entry(id)
                    .or_insert_with(|| (kind.clone(), Vec::new()));
                if entry.0 != kind {
                    return Err(RepositoryError::CorruptState);
                }
                entry.1.push(claim);
            }
        }
        let pov = match scope {
            WriterKnowledgeScope::PriorChapters(_) => None,
            WriterKnowledgeScope::CharacterPov {
                character_id,
                at_story_time,
                ..
            } => Some((character_id, at_story_time)),
        };
        let catalog_truncated = too_many_ids || claims_truncated;
        let candidates = if too_many_ids || pov.is_some() {
            Vec::new()
        } else {
            by_id
                .into_iter()
                .map(|(id, (kind, claims))| json!({"id": id, "kind": kind, "claims": claims}))
                .collect::<Vec<_>>()
        };
        let requires_disambiguation = selected_entity_id.is_none() && ids.len() > 1;
        let selected = if selected_is_name_candidate {
            selected_entity_id.or_else(|| ids.first().map(String::as_str))
        } else {
            None
        };
        let mut results = Vec::new();
        let mut truncated_matches = false;
        let mut result_bytes = 0;
        let mut unavailable_proposition_sources = 0;
        if !too_many_ids
            && !requires_disambiguation
            && let Some(selected) = selected
        {
            let sql = format!(
                "SELECT c.batch_id, c.assertion_index, c.assertion_json, b.object_id, b.source_revision_id, b.assertions_json
                 FROM story_assertion_claims_v1 c {VISIBLE_SOURCE}
                 WHERE c.project_id = ?1 AND (c.subject_id = ?2 OR c.target_id = ?2 OR c.owner_id = ?2)
                   AND ch.number <= ?3 ORDER BY ch.number, b.batch_id, c.assertion_index LIMIT ?4"
            );
            let mut statement = self.connection.prepare(&sql).map_err(storage_error)?;
            let rows = statement
                .query_map(
                    params![
                        project_id.as_str(),
                        selected,
                        visible_through,
                        MAX_SCANNED_ASSERTIONS + 1
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    },
                )
                .map_err(storage_error)?;
            for (scanned, row) in rows.enumerate() {
                let (batch_id, index, payload, object_id, revision_id, source_json) =
                    row.map_err(storage_error)?;
                if scanned as i64 == MAX_SCANNED_ASSERTIONS {
                    truncated_matches = true;
                    break;
                }
                if pov.is_none() && results.len() == limit {
                    truncated_matches = true;
                    break;
                }
                let assertion: Value =
                    serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
                let sources: Vec<Value> = serde_json::from_str(&source_json)
                    .map_err(|_| RepositoryError::CorruptState)?;
                if index < 0
                    || sources.get(index as usize) != Some(&assertion)
                    || (!["subjectId", "targetId", "epistemicOwnerId"]
                        .iter()
                        .any(|field| assertion[*field] == selected)
                        && crate::state_properties::target_entity(&assertion) != Some(selected))
                {
                    return Err(RepositoryError::CorruptState);
                }
                if assertion["kind"] == "state" {
                    crate::state_properties::schema(&assertion)
                        .map_err(|_| RepositoryError::CorruptState)?;
                }
                if let Some(source_batch_id) = assertion["propositionSourceBatchId"].as_str() {
                    let proposition_id = assertion["propositionId"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?;
                    if !proposition_source_visible(
                        &self.connection,
                        project_id,
                        ModelSourceScope::Current,
                        source_batch_id,
                        proposition_id,
                    )? {
                        unavailable_proposition_sources += 1;
                        continue;
                    }
                }
                if let Some((character_id, at_story_time)) = pov {
                    let visible = match (assertion["kind"].as_str(), at_story_time) {
                        (Some("epistemic"), Some(at)) => {
                            assertion["epistemicOwnerId"] == character_id
                                && crate::epistemic_claim_active_at(&assertion, at)
                        }
                        (Some("relation"), Some(at)) => {
                            let active = assertion["validFrom"]
                                .as_i64()
                                .or_else(|| assertion["storyTime"].as_i64())
                                .is_some_and(|start| start <= at)
                                && assertion["validUntilExclusive"]
                                    .as_i64()
                                    .is_none_or(|end| at < end);
                            if !active {
                                continue;
                            }
                            let proposition_id = assertion["propositionId"]
                                .as_str()
                                .ok_or(RepositoryError::CorruptState)?;
                            epistemic_visibility(
                                &self.connection,
                                project_id,
                                &batch_id,
                                reading_chapter,
                                character_id,
                                proposition_id,
                                Some(at),
                            )?
                        }
                        _ => false,
                    };
                    if !visible {
                        continue;
                    }
                    if results.len() == limit {
                        truncated_matches = true;
                        break;
                    }
                }
                let result = json!({"sourceObjectId": object_id, "sourceRevisionId": revision_id,
                    "sourcePath": "/body", "modelingBatchId": batch_id, "assertionIndex": index,
                    "propositionRef": assertion["propositionId"].as_str().map(|id| json!({"batchId": assertion["propositionSourceBatchId"].as_str().unwrap_or(&batch_id), "propositionId": id})),
                    "assertion": assertion, "evidence": assertion["evidence"], "confirmationState": "candidate"});
                let bytes = result.to_string().len();
                if result_bytes + bytes > MAX_RESULT_BYTES {
                    truncated_matches = true;
                    break;
                }
                result_bytes += bytes;
                results.push(result);
            }
        }
        let mut response = json!({
            "queryName": name, "readingChapter": reading_chapter, "visibleThroughChapter": visible_through,
            "povStoryTime": pov.and_then(|(_, at_story_time)| at_story_time),
            "selectedEntityId": if requires_disambiguation || too_many_ids || (pov.is_some() && results.is_empty()) { None } else { selected },
            "candidates": candidates, "results": results,
            "coverage": {"complete": total_sources == modeled_sources && !catalog_truncated && pov.is_none() && !requires_disambiguation && !truncated_matches && unavailable_proposition_sources == 0,
                "totalSources": total_sources, "modeledSources": modeled_sources,
                "pendingSources": pending_sources, "pendingSourcesTruncated": pending_truncated,
                "catalogTruncated": catalog_truncated && !is_pov,
                "claimsTruncated": claims_truncated && !is_pov,
                "requiresDisambiguation": requires_disambiguation && !is_pov,
                "truncatedMatches": truncated_matches && !is_pov, "responseTruncated": false,
                "unavailablePropositionSources": unavailable_proposition_sources,
                "povEvidenceUnverified": pov.is_some(), "povCandidateDetailsWithheld": pov.is_some()}
        });
        if response.to_string().len() > MAX_RESPONSE_BYTES {
            response["selectedEntityId"] = Value::Null;
            response["candidates"] = json!([]);
            response["results"] = json!([]);
            response["coverage"]["complete"] = json!(false);
            response["coverage"]["responseTruncated"] = json!(true);
        }
        Ok(response)
    }
}

fn source_coverage(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    visible_through: i64,
) -> Result<(i64, i64, Vec<String>, bool), RepositoryError> {
    let sql = "FROM story_object_heads_v2 o
        JOIN story_chapters_v2 ch ON ch.project_id = o.project_id AND o.object_id = 'draft-' || ch.chapter_id
        LEFT JOIN story_knowledge_heads_v2 h ON h.project_id = o.project_id AND h.object_id = o.object_id
        LEFT JOIN story_modeling_batches_v2 b ON b.batch_id = h.batch_id AND b.project_id = o.project_id
            AND b.status = 'ready' AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id)
        WHERE o.project_id = ?1 AND o.object_kind = 'draft' AND ch.number <= ?2";
    let totals = repository
        .connection
        .query_row(
            &format!("SELECT COUNT(*), COUNT(b.batch_id) {sql}"),
            params![project_id.as_str(), visible_through],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(storage_error)?;
    let mut statement = repository
        .connection
        .prepare(&format!(
            "SELECT o.object_id {sql} AND b.batch_id IS NULL ORDER BY ch.number LIMIT 101"
        ))
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![project_id.as_str(), visible_through], |row| {
            row.get::<_, String>(0)
        })
        .map_err(storage_error)?;
    let mut pending = rows
        .map(|row| row.map_err(storage_error))
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = pending.len() > 100;
    pending.truncate(100);
    Ok((totals.0, totals.1, pending, truncated))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "task_knowledge_search_tests.rs"]
mod tests;
