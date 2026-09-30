use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::identity_decisions::{
    IdentityResolution, identity_resolution, identity_resolution_at_snapshot,
};
use crate::{ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

impl SqliteStoryRepository {
    /// Current source-bound identity claims for modeling, not a canonical registry.
    /// Names and aliases are never used as implicit identity keys.
    pub fn modeling_entity_catalog(
        &self,
        project_id: &ProjectId,
        byte_budget: usize,
    ) -> Result<Value, RepositoryError> {
        self.entity_catalog(
            project_id,
            /*snapshot_id*/ None,
            /*name*/ None,
            /*through_chapter*/ None,
            byte_budget,
        )
    }

    /// Raw claims up to a trusted source chapter; later identity decisions are not applied.
    pub fn modeling_entity_catalog_through_chapter(
        &self,
        project_id: &ProjectId,
        chapter: i64,
        byte_budget: usize,
    ) -> Result<Value, RepositoryError> {
        if chapter < 0 {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.entity_catalog(
            project_id,
            /*snapshot_id*/ None,
            /*name*/ None,
            Some(chapter),
            byte_budget,
        )
    }

    pub fn resolve_entity_name(
        &self,
        project_id: &ProjectId,
        name: &str,
        byte_budget: usize,
    ) -> Result<Value, RepositoryError> {
        self.entity_catalog(
            project_id,
            /*snapshot_id*/ None,
            Some(name),
            /*through_chapter*/ None,
            byte_budget,
        )
    }

    pub fn modeling_entity_catalog_at_snapshot(
        &self,
        project_id: &ProjectId,
        snapshot_id: &SnapshotId,
        byte_budget: usize,
    ) -> Result<Value, RepositoryError> {
        self.entity_catalog(
            project_id,
            Some(snapshot_id),
            /*name*/ None,
            /*through_chapter*/ None,
            byte_budget,
        )
    }

    pub fn resolve_entity_name_at_snapshot(
        &self,
        project_id: &ProjectId,
        snapshot_id: &SnapshotId,
        name: &str,
        byte_budget: usize,
    ) -> Result<Value, RepositoryError> {
        self.entity_catalog(
            project_id,
            Some(snapshot_id),
            Some(name),
            /*through_chapter*/ None,
            byte_budget,
        )
    }

    fn entity_catalog(
        &self,
        project_id: &ProjectId,
        snapshot_id: Option<&SnapshotId>,
        name: Option<&str>,
        through_chapter: Option<i64>,
        byte_budget: usize,
    ) -> Result<Value, RepositoryError> {
        let normalized_name = name.map(|name| name.trim().to_lowercase());
        if normalized_name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.chars().count() > 160)
        {
            return Err(RepositoryError::InvalidContent(
                "invalid entity name query".into(),
            ));
        }
        let resolution = if through_chapter.is_some() {
            IdentityResolution::unresolved()
        } else if let Some(snapshot_id) = snapshot_id {
            let exists = self.connection.query_row(
                "SELECT 1 FROM story_snapshots_v2 WHERE project_id = ?1 AND branch_id = 'main' AND snapshot_id = ?2",
                params![project_id.as_str(), snapshot_id.as_str()],
                |row| row.get::<_, i64>(0),
            ).optional().map_err(storage_error)?;
            if exists.is_none() {
                return Err(RepositoryError::CandidateNotFound);
            }
            identity_resolution_at_snapshot(&self.connection, project_id, snapshot_id)?
        } else {
            identity_resolution(&self.connection, project_id)?
        };
        let sql = if snapshot_id.is_some() {
            "WITH selected AS (
                SELECT DISTINCT c.entity_id FROM story_entity_claims_v2 c
                JOIN story_snapshot_knowledge_v2 k ON k.batch_id = c.batch_id
                JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id
                JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id
                WHERE c.project_id = ?1 AND k.snapshot_id = ?2 AND m.revision_id = b.source_revision_id AND ?4 IS NULL
                  AND (?3 IS NULL OR EXISTS (SELECT 1 FROM story_entity_names_v2 n WHERE n.batch_id = c.batch_id AND n.entity_id = c.entity_id AND n.name = ?3))
                ORDER BY c.entity_id LIMIT 101
            )
            SELECT c.entity_id, c.kind, c.display_name, c.aliases_json, c.evidence_json,
                   b.object_id, b.source_revision_id, b.batch_id
            FROM selected s JOIN story_entity_claims_v2 c ON c.entity_id = s.entity_id
            JOIN story_snapshot_knowledge_v2 k ON k.batch_id = c.batch_id
            JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id
            JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id
            WHERE c.project_id = ?1 AND k.snapshot_id = ?2 AND m.revision_id = b.source_revision_id
            ORDER BY c.entity_id, b.object_id"
        } else {
            "WITH selected AS (
                SELECT DISTINCT c.entity_id FROM story_entity_claims_v2 c
                JOIN story_knowledge_heads_v2 h ON h.project_id = c.project_id AND h.batch_id = c.batch_id
                JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id
                WHERE c.project_id = ?1 AND b.status = 'ready' AND ?2 IS NULL
                  AND (?4 IS NULL OR EXISTS (SELECT 1 FROM story_chapters_v2 ch JOIN story_object_heads_v2 oh ON oh.project_id = ch.project_id AND oh.object_id = b.object_id WHERE ch.project_id = c.project_id AND b.object_id = 'draft-' || ch.chapter_id AND ch.number <= ?4 AND COALESCE(oh.candidate_revision_id, oh.accepted_revision_id) = b.source_revision_id))
                  AND (?3 IS NULL OR EXISTS (SELECT 1 FROM story_entity_names_v2 n WHERE n.batch_id = c.batch_id AND n.entity_id = c.entity_id AND n.name = ?3))
                ORDER BY c.entity_id LIMIT 101
            )
            SELECT c.entity_id, c.kind, c.display_name, c.aliases_json, c.evidence_json,
                   b.object_id, b.source_revision_id, b.batch_id
            FROM selected s JOIN story_entity_claims_v2 c ON c.entity_id = s.entity_id
            JOIN story_knowledge_heads_v2 h ON h.project_id = c.project_id AND h.batch_id = c.batch_id
            JOIN story_modeling_batches_v2 b ON b.batch_id = c.batch_id
            WHERE c.project_id = ?1 AND b.status = 'ready'
              AND (?4 IS NULL OR EXISTS (SELECT 1 FROM story_chapters_v2 ch JOIN story_object_heads_v2 oh ON oh.project_id = ch.project_id AND oh.object_id = b.object_id WHERE ch.project_id = c.project_id AND b.object_id = 'draft-' || ch.chapter_id AND ch.number <= ?4 AND COALESCE(oh.candidate_revision_id, oh.accepted_revision_id) = b.source_revision_id))
            ORDER BY c.entity_id, b.object_id"
        };
        let mut statement = self.connection.prepare(sql).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id.as_str(),
                    snapshot_id.map(SnapshotId::as_str),
                    normalized_name,
                    through_chapter
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .map_err(storage_error)?;
        let mut grouped: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
        for row in rows {
            let (id, kind, display_name, aliases, evidence, object_id, revision_id, batch_id) =
                row.map_err(storage_error)?;
            let aliases: Value =
                serde_json::from_str(&aliases).map_err(|_| RepositoryError::CorruptState)?;
            let evidence: Value =
                serde_json::from_str(&evidence).map_err(|_| RepositoryError::CorruptState)?;
            let name_match_evidence = normalized_name.as_ref().map(|name| {
                let claim_matches = display_name.trim().to_lowercase() == *name
                    || aliases.as_array().is_some_and(|aliases| {
                        aliases.iter().any(|alias| {
                            alias
                                .as_str()
                                .is_some_and(|alias| alias.trim().to_lowercase() == *name)
                        })
                    });
                if !claim_matches {
                    "notMatchedInClaim"
                } else if evidence["quote"]
                    .as_str()
                    .is_some_and(|quote| quote.to_lowercase().contains(name))
                {
                    "quoted"
                } else {
                    "unverifiedName"
                }
            });
            let entry = grouped
                .entry(id)
                .or_insert_with(|| (kind.clone(), Vec::new()));
            if entry.0 != kind {
                return Err(RepositoryError::CorruptState);
            }
            entry.1.push(json!({
                "displayName": display_name, "aliases": aliases,
                "sourceObjectId": object_id, "sourceRevisionId": revision_id,
                "modelingBatchId": batch_id,
                "evidence": evidence, "nameMatchEvidence": name_match_evidence,
            }));
        }
        let mut truncated = grouped.len() > 100;
        let mut entries = grouped
            .into_iter()
            .take(100)
            .map(|(id, (kind, claims))| {
                let canonical = resolution.canonical_id(&id);
                let merged = canonical != id;
                json!({"id": id, "canonicalId": canonical,
                    "resolutionState": if merged { "authorMerged" } else { "candidate" },
                    "kind": kind, "claims": claims})
            })
            .collect::<Vec<_>>();
        loop {
            let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for entry in &entries {
                let id = entry["id"].as_str().ok_or(RepositoryError::CorruptState)?;
                for claim in entry["claims"]
                    .as_array()
                    .ok_or(RepositoryError::CorruptState)?
                {
                    let display_name = claim["displayName"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?;
                    for name in std::iter::once(display_name).chain(
                        claim["aliases"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str),
                    ) {
                        let normalized = name.trim().to_lowercase();
                        if !normalized.is_empty() {
                            names.entry(normalized).or_default().insert(id.to_owned());
                        }
                    }
                }
            }
            let ambiguous_names = names
                .into_iter()
                .filter_map(|(name, ids)| {
                    if normalized_name.as_ref().is_some_and(|query| query != &name) {
                        return None;
                    }
                    let canonical_ids = ids
                        .iter()
                        .map(|id| resolution.canonical_id(id))
                        .collect::<BTreeSet<_>>();
                    (canonical_ids.len() > 1).then(|| {
                        let canonical_ids = canonical_ids.into_iter().collect::<Vec<_>>();
                        let author_decided_distinct =
                            canonical_ids.iter().enumerate().all(|(index, left)| {
                                canonical_ids
                                    .iter()
                                    .skip(index + 1)
                                    .all(|right| resolution.explicitly_distinct(left, right))
                            });
                        json!({"name": name, "candidateIds": ids,
                            "canonicalIds": canonical_ids,
                            "authorDecidedDistinct": author_decided_distinct})
                    })
                })
                .collect::<Vec<_>>();
            let result = json!({
                "entries": entries, "ambiguousNames": ambiguous_names,
                "snapshotId": snapshot_id.map(SnapshotId::as_str),
                "throughChapter": through_chapter,
                "queryName": normalized_name,
                "identityRevisionId": resolution.revision_id,
                "identityDecisions": resolution.decisions,
                "truncated": truncated, "ambiguityScanComplete": !truncated,
                "confirmationState": "candidate",
            });
            if result.to_string().len() <= byte_budget || entries.is_empty() {
                return Ok(result);
            }
            entries.pop();
            truncated = true;
        }
    }
}
