use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::identity_decisions::{
    IdentityResolution, identity_resolution, identity_resolution_at_snapshot,
};
use crate::{BranchId, ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

#[cfg(test)]
#[path = "evolution_tests.rs"]
mod tests;

const MAX_TRACE_ENTRIES: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionDimension {
    World,
    Narrative,
    Creative,
}

impl EvolutionDimension {
    pub fn parse(value: &str) -> Result<Self, RepositoryError> {
        match value {
            "world" => Ok(Self::World),
            "narrative" => Ok(Self::Narrative),
            "creative" => Ok(Self::Creative),
            _ => Err(RepositoryError::InvalidContent(
                "invalid evolution dimension".into(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Narrative => "narrative",
            Self::Creative => "creative",
        }
    }
}

pub struct EvolutionTraceQuery<'a> {
    pub project_id: &'a ProjectId,
    pub expected_revision: Option<i64>,
    pub snapshot_id: Option<&'a SnapshotId>,
    pub dimension: EvolutionDimension,
    pub limit: usize,
    pub cursor: Option<&'a str>,
}

pub(crate) struct TraceSource {
    pub chapter: i64,
    pub object_id: String,
    pub revision_id: Option<String>,
    pub batch_id: Option<String>,
    pub ready: bool,
    pub payload_digest: String,
}

pub(crate) struct TraceManifest {
    pub sources: Vec<TraceSource>,
    pub digest: String,
    pub pending_count: usize,
    pub pending_sources: Vec<String>,
}

impl SqliteStoryRepository {
    pub(crate) fn trace_confirmed_facts(
        &self,
        project_id: &ProjectId,
        snapshot_id: Option<&SnapshotId>,
    ) -> Result<HashMap<(String, usize), String>, RepositoryError> {
        let read = || {
            let facts = if let Some(snapshot_id) = snapshot_id {
                crate::fact_integrity::validate_fact_ledger(&self.connection, project_id)?;
                let mut statement = self.connection.prepare(
                "WITH RECURSIVE ancestors(snapshot_id) AS (\
                   SELECT ?2 UNION ALL \
                   SELECT s.parent_snapshot_id FROM story_snapshots_v2 s JOIN ancestors a ON a.snapshot_id = s.snapshot_id WHERE s.project_id = ?1 AND s.parent_snapshot_id IS NOT NULL\
                 ) SELECT f.payload_json FROM story_facts_v2 f \
                 JOIN story_acceptances_v2 accepted ON accepted.commit_id = f.accepted_by_commit_id \
                 WHERE f.project_id = ?1 AND accepted.project_id = ?1 AND accepted.operation = 'accept' \
                   AND accepted.result_snapshot_id IN (SELECT snapshot_id FROM ancestors) \
                   AND (accepted.undone_by_commit_id IS NULL OR NOT EXISTS (\
                     SELECT 1 FROM story_acceptances_v2 reverted \
                     WHERE reverted.commit_id = accepted.undone_by_commit_id \
                       AND reverted.result_snapshot_id IN (SELECT snapshot_id FROM ancestors))) \
                 ORDER BY accepted.result_revision, f.fact_id"
            ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
                statement
                    .query_map(params![project_id.as_str(), snapshot_id.as_str()], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|error| RepositoryError::Storage(error.to_string()))?
                    .map(|row| {
                        let raw =
                            row.map_err(|error| RepositoryError::Storage(error.to_string()))?;
                        serde_json::from_str::<Value>(&raw)
                            .map_err(|_| RepositoryError::CorruptState)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                self.facts(project_id)?
            };
            Ok(facts
                .into_iter()
                .filter_map(|fact| {
                    Some((
                        (
                            fact["modelingBatchId"].as_str()?.to_owned(),
                            usize::try_from(fact["modelingAssertionIndex"].as_u64()?).ok()?,
                        ),
                        fact["id"].as_str()?.to_owned(),
                    ))
                })
                .collect())
        };
        if self.connection.is_autocommit() {
            self.with_read_snapshot(read)?
        } else {
            read()
        }
    }

    pub fn evolution_trace(
        &self,
        query: EvolutionTraceQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| self.evolution_trace_in_snapshot(query))?
    }

    fn evolution_trace_in_snapshot(
        &self,
        query: EvolutionTraceQuery<'_>,
    ) -> Result<Value, RepositoryError> {
        if !(1..=MAX_TRACE_ENTRIES).contains(&query.limit) {
            return Err(RepositoryError::InvalidContent(
                "evolution trace limit is invalid".into(),
            ));
        }
        let revision = self.project_revision(query.project_id)?;
        match (query.expected_revision, query.snapshot_id) {
            (Some(expected), None) if expected != revision => {
                return Err(RepositoryError::StaleProjectRevision {
                    expected,
                    actual: revision,
                });
            }
            (Some(_), None) | (None, Some(_)) => {}
            (Some(_), Some(_)) | (None, None) => {
                return Err(RepositoryError::InvalidContent(
                    "specify projectRevision for current state or snapshotId for history".into(),
                ));
            }
        }
        let branch = BranchId::parse("main").expect("static branch ID");
        let snapshot_id = if let Some(snapshot_id) = query.snapshot_id {
            let exists = self.connection.query_row(
                "SELECT 1 FROM story_snapshots_v2 WHERE project_id = ?1 AND branch_id = 'main' AND snapshot_id = ?2",
                params![query.project_id.as_str(), snapshot_id.as_str()],
                |row| row.get::<_, i64>(0),
            ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
            if exists.is_none() {
                return Err(RepositoryError::CandidateNotFound);
            }
            snapshot_id.clone()
        } else {
            self.branch_head(query.project_id, &branch)?
                .ok_or(RepositoryError::CorruptState)?
                .snapshot_id
        };
        let identity = if let Some(historical) = query.snapshot_id {
            identity_resolution_at_snapshot(&self.connection, query.project_id, historical)?
        } else {
            identity_resolution(&self.connection, query.project_id)?
        };
        let cursor_position = if let Some(cursor) = query.cursor {
            if query.dimension == EvolutionDimension::Creative {
                let (prefix, acceptance_rowid) = cursor.rsplit_once(':').ok_or_else(|| {
                    RepositoryError::InvalidContent("invalid evolution cursor".into())
                })?;
                let (cursor_snapshot_id, rowid) = prefix.rsplit_once(':').ok_or_else(|| {
                    RepositoryError::InvalidContent("invalid evolution cursor".into())
                })?;
                let expected = SnapshotId::parse(cursor_snapshot_id).map_err(|_| {
                    RepositoryError::InvalidContent("invalid evolution cursor".into())
                })?;
                if expected != snapshot_id {
                    return Err(RepositoryError::StaleSnapshot {
                        expected,
                        actual: snapshot_id,
                    });
                }
                let rowid = rowid
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        RepositoryError::InvalidContent("invalid evolution cursor".into())
                    })?;
                let acceptance_rowid = acceptance_rowid
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| {
                        RepositoryError::InvalidContent("invalid evolution cursor".into())
                    })?;
                Some((rowid, acceptance_rowid))
            } else {
                None
            }
        } else {
            None
        };
        if !matches!(query.dimension, EvolutionDimension::Creative) {
            crate::model_sources::pending_sources(
                &self.connection,
                query.project_id,
                query.snapshot_id.map_or(
                    crate::model_sources::ModelSourceScope::Current,
                    crate::model_sources::ModelSourceScope::Snapshot,
                ),
            )?;
        }
        let (entries, coverage, next_cursor) = match query.dimension {
            EvolutionDimension::World => {
                self.world_evolution(&query, snapshot_id.as_str(), &identity)?
            }
            EvolutionDimension::Narrative => {
                self.knowledge_evolution(&query, snapshot_id.as_str(), &identity)?
            }
            EvolutionDimension::Creative => {
                self.creative_evolution(&query, cursor_position, snapshot_id.as_str())?
            }
        };
        Ok(json!({
            "projectId": query.project_id.as_str(),
            "projectRevision": query.expected_revision,
            "snapshotId": snapshot_id.as_str(),
            "identityRevisionId": identity.revision_id.as_deref(),
            "dimension": query.dimension.as_str(),
            "entries": entries,
            "coverage": coverage,
            "nextCursor": next_cursor
        }))
    }

    pub(crate) fn trace_manifest(
        &self,
        project_id: &ProjectId,
        snapshot_id: Option<&SnapshotId>,
    ) -> Result<TraceManifest, RepositoryError> {
        let sql = if snapshot_id.is_some() {
            "SELECT CASE WHEN json_type(r.content_json, '$.chapterNumber') = 'integer' THEN json_extract(r.content_json, '$.chapterNumber') END, m.object_id, m.revision_id, k.batch_id, b.published_at, b.source_revision_id, b.entities_json, b.assertions_json, r.content_json FROM story_snapshot_members_v2 m JOIN story_revisions_v2 r ON r.project_id = ?1 AND r.object_id = m.object_id AND r.revision_id = m.revision_id AND r.object_kind = 'draft' LEFT JOIN story_snapshot_knowledge_v2 k ON k.snapshot_id = m.snapshot_id AND k.object_id = m.object_id LEFT JOIN story_modeling_batches_v2 b ON b.project_id=?1 AND b.object_id=m.object_id AND b.batch_id = k.batch_id WHERE m.snapshot_id = ?2 ORDER BY 1, m.object_id"
        } else {
            "SELECT c.number, o.object_id, COALESCE(o.candidate_revision_id, o.accepted_revision_id), h.batch_id, CASE WHEN b.published_at IS NOT NULL THEN b.status END, b.source_revision_id, b.entities_json, b.assertions_json, r.content_json FROM story_object_heads_v2 o LEFT JOIN story_chapters_v2 c ON o.project_id=c.project_id AND o.object_id='draft-'||c.chapter_id LEFT JOIN story_knowledge_heads_v2 h ON h.project_id=o.project_id AND h.object_id=o.object_id LEFT JOIN story_modeling_batches_v2 b ON b.project_id=h.project_id AND b.object_id=o.object_id AND b.batch_id=h.batch_id LEFT JOIN story_revisions_v2 r ON r.project_id=o.project_id AND r.object_id=o.object_id AND r.revision_id=COALESCE(o.candidate_revision_id,o.accepted_revision_id) AND r.object_kind='draft' WHERE o.project_id=?1 AND o.object_kind='draft' AND ?2 IS NULL ORDER BY c.number,o.object_id"
        };
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let rows = statement
            .query_map(
                params![project_id.as_str(), snapshot_id.map(SnapshotId::as_str)],
                |row| {
                    let chapter = row.get::<_, Option<i64>>(0)?.unwrap_or_default();
                    let object_id: String = row.get(1)?;
                    let revision_id: Option<String> = row.get(2)?;
                    let batch_id: Option<String> = row.get(3)?;
                    let batch_source: Option<String> = row.get(5)?;
                    let published = if snapshot_id.is_some() {
                        row.get::<_, Option<i64>>(4)?.is_some()
                    } else {
                        row.get::<_, Option<String>>(4)?.as_deref() == Some("ready")
                    };
                    let ready = chapter > 0 && published && revision_id == batch_source;
                    let mut payload = Sha256::new();
                    for column in 6..=8 {
                        match row.get_ref(column)? {
                            rusqlite::types::ValueRef::Null => payload.update([0]),
                            rusqlite::types::ValueRef::Text(bytes) => {
                                payload.update([1]);
                                payload.update((bytes.len() as u64).to_be_bytes());
                                payload.update(bytes);
                            }
                            rusqlite::types::ValueRef::Blob(bytes) => {
                                payload.update([2]);
                                payload.update((bytes.len() as u64).to_be_bytes());
                                payload.update(bytes);
                            }
                            rusqlite::types::ValueRef::Integer(value) => {
                                payload.update([3]);
                                payload.update(value.to_be_bytes());
                            }
                            rusqlite::types::ValueRef::Real(value) => {
                                payload.update([4]);
                                payload.update(value.to_be_bytes());
                            }
                        }
                    }
                    Ok(TraceSource {
                        chapter,
                        object_id,
                        revision_id,
                        batch_id,
                        ready,
                        payload_digest: format!("{:x}", payload.finalize()),
                    })
                },
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let sources = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut hasher = Sha256::new();
        let mut pending_sources = Vec::new();
        let mut pending_count = 0;
        for source in &sources {
            let marker = json!([
                source.chapter,
                source.object_id,
                source.revision_id,
                source.batch_id,
                source.ready,
                source.payload_digest
            ]);
            hasher.update(marker.to_string().as_bytes());
            if !source.ready {
                pending_count += 1;
                if pending_sources.len() < 20 {
                    pending_sources.push(source.object_id.clone());
                }
            }
        }
        Ok(TraceManifest {
            sources,
            digest: format!("{:x}", hasher.finalize()),
            pending_count,
            pending_sources,
        })
    }

    fn knowledge_evolution(
        &self,
        query: &EvolutionTraceQuery<'_>,
        snapshot_id: &str,
        identity: &IdentityResolution,
    ) -> Result<(Vec<Value>, Value, Option<String>), RepositoryError> {
        let TraceManifest {
            sources,
            digest: manifest_digest,
            pending_count,
            pending_sources,
        } = self.trace_manifest(query.project_id, query.snapshot_id)?;
        let (start_source, start_assertion) = if let Some(cursor) = query.cursor {
            let parts = cursor.split(':').collect::<Vec<_>>();
            if parts.len() != 5 || parts[1] != "narrative" {
                return Err(RepositoryError::InvalidContent(
                    "invalid evolution cursor".into(),
                ));
            }
            let expected = SnapshotId::parse(parts[0])
                .map_err(|_| RepositoryError::InvalidContent("invalid evolution cursor".into()))?;
            let actual = SnapshotId::parse(snapshot_id).expect("repository snapshot ID is valid");
            if expected != actual {
                return Err(RepositoryError::StaleSnapshot { expected, actual });
            }
            if parts[2] != manifest_digest {
                return Err(RepositoryError::StaleKnowledgeManifest);
            }
            let source_index = parts[3]
                .parse::<usize>()
                .map_err(|_| RepositoryError::InvalidContent("invalid evolution cursor".into()))?;
            let assertion_index = parts[4]
                .parse::<usize>()
                .map_err(|_| RepositoryError::InvalidContent("invalid evolution cursor".into()))?;
            if source_index >= sources.len()
                || !sources[source_index].ready
                || assertion_index >= 100
            {
                return Err(RepositoryError::InvalidContent(
                    "invalid evolution cursor".into(),
                ));
            }
            (source_index, assertion_index)
        } else {
            (0, 0)
        };
        let confirmed = self.trace_confirmed_facts(query.project_id, query.snapshot_id)?;
        let mut entries = Vec::new();
        let mut next_cursor = None;
        'sources: for (source_index, source) in sources.iter().enumerate().skip(start_source) {
            if !source.ready {
                continue;
            }
            let batch_id = source
                .batch_id
                .as_deref()
                .ok_or(RepositoryError::CorruptState)?;
            let assertions = crate::model_sources::validated_source(
                &self.connection,
                query.project_id,
                query.snapshot_id.map_or(
                    crate::model_sources::ModelSourceScope::Current,
                    crate::model_sources::ModelSourceScope::Snapshot,
                ),
                batch_id,
            )?
            .ok_or(RepositoryError::CorruptState)?
            .assertions;
            if query.cursor.is_some()
                && source_index == start_source
                && start_assertion >= assertions.len()
            {
                return Err(RepositoryError::InvalidContent(
                    "invalid evolution cursor".into(),
                ));
            }
            for (index, assertion) in
                assertions
                    .iter()
                    .enumerate()
                    .skip(if source_index == start_source {
                        start_assertion
                    } else {
                        0
                    })
            {
                if entries.len() == query.limit {
                    next_cursor = Some(format!(
                        "{snapshot_id}:narrative:{manifest_digest}:{source_index}:{index}"
                    ));
                    break 'sources;
                }
                let confirmed_id = confirmed.get(&(batch_id.to_owned(), index));
                let state = if confirmed_id.is_some() {
                    "confirmed"
                } else {
                    "candidate"
                };
                let common = json!({
                    "targetId": assertion["subjectId"],
                    "canonicalTargetId": assertion["subjectId"].as_str().map(|id| identity.canonical_id(id)),
                    "predicate": assertion["predicate"],
                    "relationType": assertion["relationType"],
                    "relatedEntityId": assertion["targetId"],
                    "canonicalRelatedEntityId": assertion["targetId"].as_str().map(|id| identity.canonical_id(id)),
                    "canonicalEpistemicOwnerId": assertion["epistemicOwnerId"].as_str().map(|id| identity.canonical_id(id)),
                    "propositionId": assertion["propositionId"],
                    "sourceChapter": source.chapter,
                    "sourceObjectId": source.object_id, "sourceRevisionId": source.revision_id,
                    "modelingBatchId":batch_id,"assertionIndex":index,
                    "confirmationState": state, "confirmedFactId": confirmed_id
                });
                let mut entry = json!({"dimension": "narrative", "change": "disclosedInDraft",
                        "before": null, "after": "textuallyDisclosed",
                        "readingChapter": source.chapter, "evidence": assertion["evidence"]});
                entry
                    .as_object_mut()
                    .ok_or(RepositoryError::CorruptState)?
                    .extend(
                        common
                            .as_object()
                            .ok_or(RepositoryError::CorruptState)?
                            .clone(),
                    );
                entries.push(entry);
            }
        }
        let coverage = json!({
            "complete": pending_count == 0 && next_cursor.is_none(),
            "truncated": next_cursor.is_some(),
            "pendingSourceCount": pending_count,
            "pendingSources": pending_sources,
            "manifestDigest": manifest_digest
        });
        Ok((entries, coverage, next_cursor))
    }

    fn creative_evolution(
        &self,
        query: &EvolutionTraceQuery<'_>,
        cursor_position: Option<(i64, i64)>,
        snapshot_id: &str,
    ) -> Result<(Vec<Value>, Value, Option<String>), RepositoryError> {
        let mut statement = self.connection.prepare(
            "WITH RECURSIVE ancestors(snapshot_id) AS (SELECT ?4 WHERE ?4 IS NOT NULL UNION ALL SELECT parent_snapshot_id FROM story_snapshots_v2 s JOIN ancestors a ON s.snapshot_id = a.snapshot_id WHERE parent_snapshot_id IS NOT NULL) SELECT s.rowid, COALESCE(a.rowid, 0), COALESCE(c.commit_id, a.commit_id), s.parent_snapshot_id, s.snapshot_id, r.revision_id, r.object_id, r.parent_revision_id, a.operation, a.object_id, a.object_kind, a.accepted_revision_id, a.previous_accepted_revision_id, a.target_commit_id, target.accepted_revision_id, target.previous_accepted_revision_id, COALESCE(m.application_id, target_member.application_id) FROM story_snapshots_v2 s LEFT JOIN story_commits_v2 c ON c.project_id = s.project_id AND c.result_snapshot_id = s.snapshot_id LEFT JOIN story_candidate_receipts_v2 receipt ON receipt.project_id = s.project_id AND receipt.result_snapshot_id = s.snapshot_id LEFT JOIN story_revisions_v2 r ON r.revision_id = receipt.revision_id LEFT JOIN story_acceptances_v2 a ON a.project_id = s.project_id AND a.result_snapshot_id = s.snapshot_id LEFT JOIN story_acceptances_v2 target ON target.commit_id = a.target_commit_id LEFT JOIN story_proposal_application_members_v1 m ON m.acceptance_commit_id = a.commit_id LEFT JOIN story_proposal_application_members_v1 target_member ON target_member.acceptance_commit_id = target.commit_id WHERE s.project_id = ?1 AND s.branch_id = 'main' AND s.parent_snapshot_id IS NOT NULL AND (c.commit_id IS NOT NULL OR a.commit_id IS NOT NULL) AND (?3 IS NULL OR s.rowid < ?3 OR (s.rowid = ?3 AND COALESCE(a.rowid, 0) < ?5)) AND (?4 IS NULL OR s.snapshot_id IN (SELECT snapshot_id FROM ancestors)) ORDER BY s.rowid DESC, COALESCE(a.rowid, 0) DESC LIMIT ?2"
        ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut entries = statement
            .query_map(
                params![
                    query.project_id.as_str(),
                    (query.limit + 1) as i64,
                    cursor_position.map(|position| position.0),
                    query.snapshot_id.map(SnapshotId::as_str),
                    cursor_position.map(|position| position.1)
                ],
                |row| {
                    let rowid: i64 = row.get(0)?;
                    let acceptance_rowid: i64 = row.get(1)?;
                    let commit_id: String = row.get(2)?;
                    let base_snapshot_id: String = row.get(3)?;
                    let result_snapshot_id: String = row.get(4)?;
                    let revision_id: Option<String> = row.get(5)?;
                    let candidate_object_id: Option<String> = row.get(6)?;
                    let parent_revision_id: Option<String> = row.get(7)?;
                    let operation: Option<String> = row.get(8)?;
                    let accepted_object_id: Option<String> = row.get(9)?;
                    let object_kind: Option<String> = row.get(10)?;
                    let accepted_revision_id: Option<String> = row.get(11)?;
                    let previous_accepted_revision_id: Option<String> = row.get(12)?;
                    let target_commit_id: Option<String> = row.get(13)?;
                    let target_accepted_revision_id: Option<String> = row.get(14)?;
                    let target_previous_revision_id: Option<String> = row.get(15)?;
                    let proposal_application_id: Option<String> = row.get(16)?;
                    let (change, object_id, before, after) = match operation.as_deref() {
                        Some("accept") => (
                            "accepted",
                            accepted_object_id,
                            previous_accepted_revision_id,
                            accepted_revision_id,
                        ),
                        Some("undo") => (
                            "undone",
                            accepted_object_id,
                            target_accepted_revision_id,
                            target_previous_revision_id,
                        ),
                        _ => (
                            "candidateSaved",
                            candidate_object_id,
                            parent_revision_id,
                            revision_id,
                        ),
                    };
                    Ok((
                        (rowid, acceptance_rowid),
                        json!({
                            "dimension": "creative", "change": change, "targetId": object_id,
                            "beforeRevisionId": before, "afterRevisionId": after,
                            "objectKind": object_kind, "commitId": commit_id,
                            "targetCommitId": target_commit_id,
                            "proposalApplicationId": proposal_application_id,
                            "baseSnapshotId": base_snapshot_id,
                            "resultSnapshotId": result_snapshot_id
                        }),
                    ))
                },
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let truncated = entries.len() > query.limit;
        entries.truncate(query.limit);
        let next_cursor = if truncated {
            entries.last().map(|((rowid, acceptance_rowid), _)| {
                format!("{snapshot_id}:{rowid}:{acceptance_rowid}")
            })
        } else {
            None
        };
        let entries = entries.into_iter().rev().map(|(_, value)| value).collect();
        Ok((
            entries,
            json!({"complete": !truncated, "truncated": truncated}),
            next_cursor,
        ))
    }
}
