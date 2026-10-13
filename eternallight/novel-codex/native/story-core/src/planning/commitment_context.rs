use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::narrative_commitments::validate_content;
use crate::query_scope::TaskQueryScope;
use crate::{ObjectId, RepositoryError, SearchScope, SqliteStoryRepository, TaskRecord};

const ENTRY_LIMIT: usize = 16;
const BYTE_LIMIT: usize = 12_000;

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

impl SqliteStoryRepository {
    

    fn select_commitments(
        &self,
        task: &TaskRecord,
        scope: &TaskQueryScope,
    ) -> Result<Value, RepositoryError> {
        let snapshot: String = self.connection.query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [task.project_id.as_str()], |row| row.get(0),
        ).map_err(storage)?;
        let (first, last, source_revision) = match scope {
            TaskQueryScope::Project { .. } => (1, i64::MAX, None),
            TaskQueryScope::Bounded(bound) => {
                let range = match bound.text_scope {
                    SearchScope::All => (1, i64::MAX),
                    SearchScope::Chapter(chapter) => (chapter, chapter),
                    SearchScope::ThroughChapter(chapter)
                    | SearchScope::ThroughScene { chapter, .. } => (1, chapter),
                };
                (range.0, range.1, bound.source_revision_id.as_deref())
            }
        };
        // Authorization and source currency precede selection limits. Hidden future
        // records cannot consume the slots intended for eligible earlier promises.
        let mut statement = self.connection.prepare(
            "SELECT r.object_id,r.revision_id,r.content_json,h.accepted_revision_id
             FROM story_snapshot_members_v2 m
             JOIN story_revisions_v2 r ON r.revision_id=m.revision_id AND r.project_id=?1
             JOIN story_object_heads_v2 h ON h.project_id=r.project_id AND h.object_id=r.object_id
             WHERE m.snapshot_id=?2 AND r.object_kind='commitment'
             AND NOT EXISTS (
               SELECT 1 FROM (
                 SELECT value FROM json_each(r.content_json,'$.payload.setupEvidenceRefs')
                 UNION ALL SELECT value FROM json_each(r.content_json,'$.payload.fulfillmentEvidenceRefs')
               ) a LEFT JOIN story_snapshot_members_v2 sm
                 ON sm.snapshot_id=?2 AND sm.object_id=json_extract(a.value,'$.objectId')
               LEFT JOIN story_revisions_v2 sr ON sr.revision_id=sm.revision_id
                 AND sr.project_id=?1 AND sr.object_kind='draft'
               WHERE sr.revision_id IS NULL OR sr.revision_id != json_extract(a.value,'$.revisionId')
                 OR COALESCE(json_extract(sr.content_json,'$.chapterNumber'),0) NOT BETWEEN ?3 AND ?4
                 OR (?5 IS NOT NULL AND sr.revision_id != ?5)
             ) ORDER BY CASE json_extract(r.content_json,'$.payload.status')
                 WHEN 'open' THEN 0 WHEN 'partiallyFulfilled' THEN 1 ELSE 2 END,r.object_id"
        ).map_err(storage)?;
        let mut rows = statement
            .query(params![
                task.project_id.as_str(),
                snapshot,
                first,
                last,
                source_revision
            ])
            .map_err(storage)?;
        let mut entries = Vec::new();
        let mut bytes = 2;
        let mut truncated = false;
        while let Some(row) = rows.next().map_err(storage)? {
            let object: String = row.get(0).map_err(storage)?;
            let revision: String = row.get(1).map_err(storage)?;
            let content: String = row.get(2).map_err(storage)?;
            let accepted: Option<String> = row.get(3).map_err(storage)?;
            let value = validate_content(
                &self.connection,
                &task.project_id,
                &ObjectId::parse(object.clone()).map_err(|_| RepositoryError::CorruptState)?,
                &content,
            )?;
            if !self.commitment_scene_visible(&value, scope)? {
                continue;
            }
            if entries.len() == ENTRY_LIMIT {
                truncated = true;
                break;
            }
            let mut projection = json!({
                "objectId":object,"revisionId":revision,"snapshotId":snapshot,
                "id":value.id,"kind":value.kind,"summary":value.summary,
                "status":value.status,"fulfillmentCriteria":value.fulfillment_criteria,
                "setupEvidenceRefs":value.setup_evidence_refs,
                "fulfillmentEvidenceRefs":value.fulfillment_evidence_refs,
                "confirmationState":if accepted.as_deref()==Some(revision.as_str()) {"confirmed"} else {"candidate"},
                "authority":"candidateNarrativeInterpretation",
                "sourceCurrent":true,"literaryFulfillmentVerified":false
            });
            if matches!(scope, TaskQueryScope::Project { .. }) {
                projection["authorMetadata"] = json!({
                    "relatedEntityIds":value.related_entity_ids,"plotThreadId":value.plot_thread_id,
                    "characterArcId":value.character_arc_id,"targetWindow":value.target_window,
                    "decisionReason":value.decision_reason
                });
            }
            let size = projection.to_string().len() + usize::from(!entries.is_empty());
            if bytes + size > BYTE_LIMIT {
                truncated = true;
                continue;
            }
            bytes += size;
            entries.push(projection);
        }
        Ok(json!({"entries":entries,"coverage":{
            "complete":!truncated,"truncated":truncated,"includedEntries":entries.len(),
            "sourceCurrentOnly":true,"globalCompletenessClaimed":false,
            "boundedMetadataWithheld":matches!(scope,TaskQueryScope::Bounded(_)),
            "semanticCompleteness":"unknown","entryLimit":ENTRY_LIMIT,"byteLimit":BYTE_LIMIT,
            "selectionPolicy":"openThenPartialThenClosedStableId"
        }}))
    }

    fn commitment_scene_visible(
        &self,
        value: &crate::NarrativeCommitment,
        scope: &TaskQueryScope,
    ) -> Result<bool, RepositoryError> {
        let TaskQueryScope::Bounded(bound) = scope else {
            return Ok(true);
        };
        let SearchScope::ThroughScene {
            chapter,
            end_scene_order,
        } = bound.text_scope
        else {
            return Ok(true);
        };
        for anchor in value
            .setup_evidence_refs
            .iter()
            .chain(&value.fulfillment_evidence_refs)
        {
            let source: Option<String> = self
                .connection
                .query_row(
                    "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
                    [&anchor.revision_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage)?;
            let source: Value = serde_json::from_str(&source.ok_or(RepositoryError::CorruptState)?)
                .map_err(|_| RepositoryError::CorruptState)?;
            if source["chapterNumber"].as_i64() != Some(chapter) {
                continue;
            }
            let groups = source["sceneGroups"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?;
            let prefix = groups
                .iter()
                .take(end_scene_order)
                .map(|group| {
                    group["body"]
                        .as_str()
                        .map(str::trim)
                        .ok_or(RepositoryError::CorruptState)
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("\n\n---\n\n");
            let body = source["body"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            // Offsets are in the original /body; a reconstructed prefix may only
            // authorize them when it is actually a prefix of that immutable body.
            if !body.starts_with(&prefix) || anchor.end > prefix.chars().count() {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Public commitment context capability; SQLite retains transaction ownership.
pub trait CommitmentContextRepository {
    /// Projects source-bound narrative interpretations using only the persisted task's
    /// authority and text scope. This does not grant model write or confirmation rights.
    fn task_context_commitments(&self, supplied: &TaskRecord) -> Result<Value, RepositoryError>;
}

impl CommitmentContextRepository for SqliteStoryRepository {
    /// Projects source-bound narrative interpretations using only the persisted task's
    /// authority and text scope. This does not grant model write or confirmation rights.
    fn task_context_commitments(&self, supplied: &TaskRecord) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let task = crate::tasks::load_task(&self.connection, &supplied.task_id)?;
            if task.project_id != supplied.project_id
                || task.thread_id != supplied.thread_id
                || task.operation != supplied.operation
                || task.target_json != supplied.target_json
                || task.input_manifest_json != supplied.input_manifest_json
                || task.input_revision != supplied.input_revision
                || task.capability_hash != supplied.capability_hash
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            if !matches!(task.status.as_str(), "queued" | "running" | "waiting")
                || task.capability_revoked_at.is_some()
            {
                return Err(RepositoryError::TaskNotWritable);
            }
            self.assert_task_inputs_current(&task)?;
            let scope = TaskQueryScope::from_task(self, &task)?;
            if matches!(&scope, TaskQueryScope::Bounded(bound) if bound.pov.is_some()) {
                // Summary and criteria are not character-knowledge assertions. Do not
                // disclose even catalog size until their POV evidence is modeled.
                return Ok(json!({"entries":[],"proposalContext":{"entries":[],"coverage":{"requested":false,"complete":true,"includedEntries":0}},"coverage":{
                    "complete":false,"povEvidenceUnverified":true,
                    "semanticCompleteness":"unknown","includedEntries":0
                }}));
            }
            let mut selected = self.select_commitments(&task, &scope)?;
            selected["proposalContext"] = self.select_commitment_proposals(&task, &scope)?;
            selected["assessmentContext"] = if task.operation == "assessSceneOutcome" {
                crate::scene_commitment_context::context(&self.connection, &task)?
            } else {
                Value::Null
            };
            Ok(selected)
        })?
    }
}

use crate::{ReadSnapshotRepository, TaskInputRepository};
