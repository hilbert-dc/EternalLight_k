use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    BranchId, ProjectId, RepositoryError, RevisionRecord, SnapshotId, SqliteStoryRepository,
    TaskRecord,
};

const MAX_INDEX_FIELDS: usize = 1_024;
const MAX_INDEX_TEXT_BYTES: usize = 8_000_000;
const MAX_CANDIDATES: usize = 1_000;

pub struct SearchQuery<'a> {
    pub project_id: &'a ProjectId,
    pub snapshot_id: Option<&'a SnapshotId>,
    pub text: &'a str,
    pub kind: Option<&'a str>,
    pub scope: SearchScope,
    pub limit: usize,
    pub cursor: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchScope {
    All,
    Chapter(i64),
    ThroughChapter(i64),
    ThroughScene {
        chapter: i64,
        end_scene_order: usize,
    },
}

pub(crate) fn index_revision(
    connection: &Connection,
    revision: &RevisionRecord,
) -> Result<(), RepositoryError> {
    let fields = revision_index_fields(&revision.content_json)?;
    let mut insert_gram = connection
        .prepare_cached(
            "INSERT INTO story_search_grams_v2(project_id, gram, doc_id) VALUES (?1, ?2, ?3)",
        )
        .map_err(storage_error)?;
    for (field_path, text) in fields {
        connection.execute(
            "INSERT INTO story_search_documents_v2(project_id, object_id, revision_id, field_path, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![revision.project_id.as_str(), revision.object_id.as_str(), revision.revision_id.as_str(), field_path, text],
        ).map_err(storage_error)?;
        let doc_id = connection.last_insert_rowid();
        for gram in grams(&text.to_lowercase()) {
            insert_gram
                .execute(params![revision.project_id.as_str(), gram, doc_id])
                .map_err(storage_error)?;
        }
    }
    Ok(())
}

pub(crate) fn revision_index_fields(
    content_json: &str,
) -> Result<Vec<(String, String)>, RepositoryError> {
    let content: Value =
        serde_json::from_str(content_json).map_err(|_| RepositoryError::CorruptState)?;
    let mut fields = Vec::new();
    let mut total_bytes = 0;
    collect_text_fields(&content, String::new(), &mut fields, &mut total_bytes)?;
    fields.sort_by_key(|(path, _)| !path.starts_with("/sceneGroups/"));
    let mut indexed_text = HashSet::new();
    Ok(fields
        .into_iter()
        .filter(|(_, text)| indexed_text.insert(text.clone()))
        .collect())
}

fn collect_text_fields(
    value: &Value,
    path: String,
    fields: &mut Vec<(String, String)>,
    total_bytes: &mut usize,
) -> Result<(), RepositoryError> {
    match value {
        Value::String(text) if !text.is_empty() => {
            *total_bytes += text.len();
            if *total_bytes > MAX_INDEX_TEXT_BYTES || fields.len() == MAX_INDEX_FIELDS {
                return Err(RepositoryError::InvalidContent(
                    "revision exceeds searchable text limits".into(),
                ));
            }
            fields.push((path, text.clone()));
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_text_fields(item, format!("{path}/{index}"), fields, total_bytes)?;
            }
        }
        Value::Object(items) => {
            for (key, item) in items {
                if matches!(
                    key.as_str(),
                    "id" | "chapterId"
                        | "chapterPlanId"
                        | "sceneId"
                        | "beatId"
                        | "clientKey"
                        | "parentId"
                        | "parentClientKey"
                        | "primaryNodeId"
                        | "dependencyKeys"
                        | "beatIds"
                        | "sourceRevisionId"
                        | "kind"
                        | "status"
                        | "certainty"
                        | "protection"
                        | "completeness"
                ) {
                    continue;
                }
                let escaped = key.replace('~', "~0").replace('/', "~1");
                collect_text_fields(item, format!("{path}/{escaped}"), fields, total_bytes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn grams(text: &str) -> Vec<String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut unique = HashSet::new();
    for character in &chars {
        unique.insert(character.to_string());
    }
    for pair in chars.windows(2) {
        unique.insert(pair.iter().collect::<String>());
    }
    let mut result = unique.into_iter().collect::<Vec<_>>();
    result.sort();
    result
}

fn query_grams(text: &str) -> Vec<String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut unique = HashSet::new();
    if chars.len() == 1 {
        unique.insert(chars[0].to_string());
    } else {
        for pair in chars.windows(2) {
            unique.insert(pair.iter().collect::<String>());
        }
    }
    let mut result = unique.into_iter().collect::<Vec<_>>();
    result.sort();
    result
}

fn find_evidence(text: &str, query: &str) -> Option<(usize, usize, String, String)> {
    let original = text.chars().collect::<Vec<_>>();
    let mut lowered = Vec::new();
    let mut original_positions = Vec::new();
    for (position, character) in original.iter().enumerate() {
        for lower in character.to_lowercase() {
            lowered.push(lower);
            original_positions.push(position);
        }
    }
    let needle = query.chars().collect::<Vec<_>>();
    let start = lowered
        .windows(needle.len())
        .position(|window| window == needle)?;
    let source_start = original_positions[start];
    let source_end = original_positions[start + needle.len() - 1] + 1;
    let quote = original[source_start..source_end]
        .iter()
        .collect::<String>();
    let snippet = original[source_start.saturating_sub(35)..(source_end + 35).min(original.len())]
        .iter()
        .collect::<String>();
    Some((source_start, source_end, quote, snippet))
}

impl SqliteStoryRepository {
    pub fn search_documents(&self, query: SearchQuery<'_>) -> Result<Value, RepositoryError> {
        self.search_documents_with_source(query, false)
    }

    pub fn search_documents_for_task(
        &self,
        task: &TaskRecord,
        query: SearchQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        if task.project_id != *query.project_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.assert_task_inputs_current(task)?;
        let candidate_first = crate::query_scope::uses_candidate_source(task)?;
        self.search_documents_with_source(query, candidate_first)
    }

    fn search_documents_with_source(
        &self,
        query: SearchQuery<'_>,
        candidate_first: bool,
    ) -> Result<Value, RepositoryError> {
        let normalized = query.text.trim().to_lowercase();
        if normalized.chars().count() > 240
            || !(1..=50).contains(&query.limit)
            || (query.snapshot_id.is_some() && query.scope != SearchScope::All)
            || matches!(query.scope, SearchScope::Chapter(chapter) if chapter < 1)
            || matches!(query.scope, SearchScope::ThroughScene { end_scene_order, .. } if !(1..=100).contains(&end_scene_order))
        {
            return Err(RepositoryError::InvalidContent(
                "invalid search query".into(),
            ));
        }
        self.project_revision(query.project_id)?;
        let branch = BranchId::parse("main").expect("static branch ID");
        let current_snapshot = self
            .branch_head(query.project_id, &branch)?
            .ok_or(RepositoryError::CorruptState)?;
        let snapshot_id = if let Some(snapshot_id) = query.snapshot_id {
            let exists = self.connection.query_row(
                "SELECT 1 FROM story_snapshots_v2 WHERE project_id = ?1 AND branch_id = 'main' AND snapshot_id = ?2",
                params![query.project_id.as_str(), snapshot_id.as_str()],
                |row| row.get::<_, i64>(0),
            ).optional().map_err(storage_error)?;
            if exists.is_none() {
                return Err(RepositoryError::CandidateNotFound);
            }
            snapshot_id.clone()
        } else {
            current_snapshot.snapshot_id
        };
        let scope = match query.scope {
            SearchScope::All => json!({"type": "all"}),
            SearchScope::Chapter(chapter) => {
                json!({"type":"chapter", "chapterNumber":chapter, "objectKinds":["draft"]})
            }
            SearchScope::ThroughChapter(chapter) => {
                json!({"type": "throughChapter", "chapterNumber": chapter, "objectKinds": ["draft"]})
            }
            SearchScope::ThroughScene {
                chapter,
                end_scene_order,
            } => {
                json!({"type": "throughScene", "chapterNumber": chapter,
                    "endSceneOrder": end_scene_order, "objectKinds": ["draft"]})
            }
        };
        let source_digest = query
            .snapshot_id
            .is_none()
            .then(|| {
                self.project_source_watermarks(query.project_id)
                    .and_then(|value| {
                        value["digest"]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or(RepositoryError::CorruptState)
                    })
            })
            .transpose()?;
        let query_digest = format!(
            "{:x}",
            Sha256::digest(
                json!([normalized, query.kind, scope, candidate_first])
                    .to_string()
                    .as_bytes()
            )
        );
        let source_cursor_digest = source_digest.as_deref().unwrap_or("-");
        let index_generation = self.search_index_generation(query.project_id)?;
        let cursor_doc_id = if let Some(cursor) = query.cursor {
            let parts = cursor.split(':').collect::<Vec<_>>();
            if parts.len() != 5
                || parts[1] != query_digest
                || parts[3].parse::<i64>().ok() != Some(index_generation)
            {
                return Err(RepositoryError::InvalidContent(
                    "invalid search cursor".into(),
                ));
            }
            if parts[2] != source_cursor_digest {
                return Err(RepositoryError::StaleSearchCursor);
            }
            let expected = SnapshotId::parse(parts[0])
                .map_err(|_| RepositoryError::InvalidContent("invalid search cursor".into()))?;
            if expected != snapshot_id {
                return Err(RepositoryError::StaleSnapshot {
                    expected,
                    actual: snapshot_id,
                });
            }
            Some(
                parts[4]
                    .parse::<i64>()
                    .ok()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| {
                        RepositoryError::InvalidContent("invalid search cursor".into())
                    })?,
            )
        } else {
            None
        };
        if normalized.is_empty() {
            return Ok(json!({"results": [], "coverage": {"complete": true,
                "snapshotId": snapshot_id.as_str(), "scannedCandidates": 0,
                "matchedDocuments": 0, "candidateCapReached": false,
                "truncatedMatches": false, "indexVersion": 1,
                "indexGeneration": index_generation, "matchMode": "contiguousSubstring",
                "scope": scope}, "nextCursor": null}));
        }
        let grams = query_grams(&normalized);
        let placeholders = vec!["?"; grams.len()].join(", ");
        let scope_clause = match query.scope {
            SearchScope::All => "1 = 1".to_owned(),
            SearchScope::Chapter(_) => "h.object_kind = 'draft' AND c.number = ?".to_owned(),
            SearchScope::ThroughChapter(_) => {
                "h.object_kind = 'draft' AND c.number <= ?".to_owned()
            }
            SearchScope::ThroughScene {
                end_scene_order, ..
            } => format!(
                "h.object_kind = 'draft' AND (c.number < ? OR (c.number = ? AND d.field_path IN ({})))",
                vec!["?"; end_scene_order].join(", ")
            ),
        };
        let sql = format!(
            "SELECT d.doc_id, d.object_id, d.revision_id, d.field_path, d.text, r.object_kind, CASE WHEN json_type(r.content_json, '$.title') = 'text' THEN json_extract(r.content_json, '$.title') ELSE r.object_kind END FROM story_search_grams_v2 g JOIN story_search_documents_v2 d ON d.doc_id = g.doc_id AND d.project_id = g.project_id JOIN story_revisions_v2 r ON r.revision_id = d.revision_id AND r.project_id = d.project_id AND r.object_id = d.object_id LEFT JOIN story_object_heads_v2 h ON h.project_id = d.project_id AND h.object_id = d.object_id LEFT JOIN story_chapters_v2 c ON c.project_id = h.project_id AND h.object_id = 'draft-' || c.chapter_id LEFT JOIN story_working_baselines_v2 b ON b.project_id=h.project_id AND b.object_kind=h.object_kind AND b.object_id=CASE WHEN h.object_kind='draft' THEN c.chapter_id ELSE h.object_kind END WHERE g.project_id = ? AND g.gram IN ({placeholders}) AND (? IS NULL OR r.object_kind = ?) AND ((? IS NULL AND h.object_kind = r.object_kind AND d.revision_id = CASE WHEN ? THEN COALESCE(h.candidate_revision_id,h.accepted_revision_id) ELSE COALESCE(b.revision_id,h.accepted_revision_id,h.candidate_revision_id) END) OR (? IS NOT NULL AND EXISTS (SELECT 1 FROM story_snapshot_members_v2 m WHERE m.snapshot_id = ? AND m.object_id = d.object_id AND m.revision_id = d.revision_id))) AND ({scope_clause}) AND (? IS NULL OR d.doc_id > ?) GROUP BY d.doc_id HAVING COUNT(*) = ? ORDER BY d.doc_id LIMIT ?"
        );
        let mut values = vec![SqlValue::Text(query.project_id.as_str().to_owned())];
        values.extend(grams.iter().cloned().map(SqlValue::Text));
        let kind = query
            .kind
            .map(str::to_owned)
            .map(SqlValue::Text)
            .unwrap_or(SqlValue::Null);
        values.push(kind.clone());
        values.push(kind);
        let snapshot_value = query
            .snapshot_id
            .map(|id| SqlValue::Text(id.as_str().to_owned()))
            .unwrap_or(SqlValue::Null);
        values.extend([
            snapshot_value.clone(),
            SqlValue::Integer(i64::from(candidate_first)),
            snapshot_value.clone(),
            snapshot_value,
        ]);
        match query.scope {
            SearchScope::All => {}
            SearchScope::Chapter(chapter) => values.push(SqlValue::Integer(chapter)),
            SearchScope::ThroughChapter(chapter) => values.push(SqlValue::Integer(chapter)),
            SearchScope::ThroughScene {
                chapter,
                end_scene_order,
            } => {
                values.push(SqlValue::Integer(chapter));
                values.push(SqlValue::Integer(chapter));
                values.extend(
                    (0..end_scene_order)
                        .map(|index| SqlValue::Text(format!("/sceneGroups/{index}/body"))),
                );
            }
        }
        let cursor_value = cursor_doc_id
            .map(SqlValue::Integer)
            .unwrap_or(SqlValue::Null);
        values.push(cursor_value.clone());
        values.push(cursor_value);
        values.push(SqlValue::Integer(grams.len() as i64));
        values.push(SqlValue::Integer((MAX_CANDIDATES + 1) as i64));
        let mut statement = self.connection.prepare(&sql).map_err(storage_error)?;
        let rows = statement
            .query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(storage_error)?;
        let candidates = rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
        let candidate_cap_reached = candidates.len() > MAX_CANDIDATES;
        let scanned_candidates = candidates.len().min(MAX_CANDIDATES);
        let mut results = Vec::new();
        let mut matched_documents = 0;
        let mut last_scanned_id = cursor_doc_id;
        let mut last_returned_id = None;
        for (doc_id, object_id, revision_id, field_path, text, kind, title) in
            candidates.into_iter().take(MAX_CANDIDATES)
        {
            last_scanned_id = Some(doc_id);
            if let Some((start, end, quote, snippet)) = find_evidence(&text, &normalized) {
                matched_documents += 1;
                if results.len() < query.limit {
                    last_returned_id = Some(doc_id);
                    results.push(json!({
                        "id": object_id, "title": title, "kind": kind,
                        "snippet": snippet, "source": "story-revision",
                        "sourceRevisionId": revision_id, "sourcePath": field_path,
                        "evidence": {"quote": quote, "start": start, "end": end}
                    }));
                }
            }
        }
        if query.snapshot_id.is_none() {
            let current = self
                .branch_head(query.project_id, &branch)?
                .ok_or(RepositoryError::CorruptState)?;
            if current.snapshot_id != snapshot_id {
                return Err(RepositoryError::StaleSnapshot {
                    expected: snapshot_id,
                    actual: current.snapshot_id,
                });
            }
            if self.project_source_watermarks(query.project_id)?["digest"].as_str()
                != source_digest.as_deref()
            {
                return Err(RepositoryError::InvalidContent(
                    "search source changed during query".into(),
                ));
            }
        }
        if self.search_index_generation(query.project_id)? != index_generation {
            return Err(RepositoryError::InvalidContent(
                "search index changed during query".into(),
            ));
        }
        let more = candidate_cap_reached || matched_documents > query.limit;
        let next_cursor = if more {
            last_returned_id.or(last_scanned_id).map(|id| {
                format!(
                    "{snapshot_id}:{query_digest}:{source_cursor_digest}:{index_generation}:{id}"
                )
            })
        } else {
            None
        };
        Ok(json!({
            "results": results,
            "nextCursor": next_cursor,
            "coverage": {
                "complete": !more,
                "snapshotId": snapshot_id.as_str(),
                "scannedCandidates": scanned_candidates,
                "matchedDocuments": matched_documents,
                "candidateCapReached": candidate_cap_reached,
                "truncatedMatches": matched_documents > query.limit,
                "scope": scope,
                "indexVersion": 1,
                "indexGeneration": index_generation,
                "matchMode": "contiguousSubstring"
            }
        }))
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
