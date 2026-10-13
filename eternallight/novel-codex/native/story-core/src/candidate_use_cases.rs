//! Application use cases own candidate identity, envelopes and the saving transaction.
use crate::{
    BranchId, CandidateReceipt, ChapterProjection, CommitId, IdempotencyKey, ObjectId, ProjectId,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
    TaskOutputBinding,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct CandidateContext {
    pub project_id: ProjectId,
    pub expected_revision: i64,
    pub expected_candidate: Option<RevisionId>,
    pub idempotency_key: IdempotencyKey,
    pub author: bool,
    pub task_output: Option<TaskOutputBinding>,
}

pub struct DraftSubmission {
    pub context: CandidateContext,
    pub chapter_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub scene_groups: Vec<Value>,
    pub source_plan_refs: Value,
    pub completeness: Value,
    pub request_digest: String,
}

/// Domain normalization of a chapter draft, including generated chapter/scene identity.
pub struct DraftContent {
    pub chapter_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub scene_groups: Vec<Value>,
}
impl DraftContent {
    pub fn from_payload(value: &Value) -> Result<Self, CandidateWriteError> {
        let invalid = |code, message| CandidateWriteError::InvalidDraft { code, message };
        let payload = value
            .as_object()
            .ok_or_else(|| invalid("invalid_draft", "Draft payload must be an object"))?;
        let body = payload
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if body.chars().count() > 120_000 {
            return Err(CandidateWriteError::DraftTooLarge);
        }
        let number = payload
            .get("chapterNumber")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        let title = payload
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("未命名章节")
            .trim()
            .to_owned();
        let chapter_id = payload
            .get("chapterId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("chapter-{}", Uuid::new_v4().simple()));
        if number < 1 || title.chars().count() > 160 {
            return Err(invalid("invalid_chapter", "Chapter metadata is invalid"));
        }
        let mut scene_groups = payload
            .get("sceneGroups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if scene_groups.len() > 100 {
            return Err(invalid("invalid_scene_groups", "Too many scene groups"));
        }
        if scene_groups.is_empty() {
            scene_groups.push(json!({"id": format!("scene-group-{chapter_id}"), "sceneId": null, "body": body, "locked": false}));
        }
        if scene_groups
            .iter()
            .any(|group| group.get("body").and_then(Value::as_str).is_none())
        {
            return Err(invalid("invalid_scene_group", "Scene groups are invalid"));
        }
        let grouped_body = crate::scene_body::canonical_scene_body(
            scene_groups
                .iter()
                .filter_map(|group| group.get("body").and_then(Value::as_str)),
        );
        if grouped_body.trim() != body.trim() {
            return Err(invalid(
                "draft_scene_groups_mismatch",
                "Draft body and scene groups differ",
            ));
        }
        Ok(Self {
            chapter_id,
            number,
            title,
            body,
            scene_groups,
        })
    }
}

pub struct ArtifactSubmission {
    pub context: CandidateContext,
    pub kind: String,
    pub payload: Value,
    pub linked_impact_ids: Vec<String>,
}

#[derive(Debug)]
pub enum CandidateWriteError {
    Repository(RepositoryError),
    MissingExpectedRevision,
    ArtifactTooLarge,
    DraftTooLarge,
    InvalidDraft {
        code: &'static str,
        message: &'static str,
    },
}
impl From<RepositoryError> for CandidateWriteError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

/// A narrow borrowing store; the owning repository retains the single transaction authority.
pub struct CandidateStore<'a> {
    repository: &'a mut SqliteStoryRepository,
}
impl SqliteStoryRepository {
    pub fn candidates(&mut self) -> CandidateStore<'_> {
        CandidateStore { repository: self }
    }
}
impl CandidateStore<'_> {
    pub fn set_artifact_lock(
        &mut self,
        project_id: &ProjectId,
        kind: &str,
        locked: bool,
        expected_revision: i64,
    ) -> Result<i64, RepositoryError> {
        if !matches!(kind, "brief" | "outline" | "plan") {
            return Err(RepositoryError::InvalidObjectKind);
        }
        let object_id = ObjectId::parse(format!("artifact-{kind}"))
            .map_err(|source| RepositoryError::InvalidContent(source.to_string()))?;
        self.repository
            .set_object_lock(project_id, &object_id, locked, expected_revision)
    }

    pub fn save_draft(
        &mut self,
        submission: DraftSubmission,
    ) -> Result<CandidateReceipt, CandidateWriteError> {
        let DraftSubmission {
            context,
            chapter_id,
            number,
            title,
            body,
            scene_groups,
            source_plan_refs,
            completeness,
            request_digest,
        } = submission;
        if body.chars().count() > 120_000 {
            return Err(CandidateWriteError::DraftTooLarge);
        }
        let version = self.version(context.expected_candidate.as_ref())?;
        let envelope = json!({"chapterId": chapter_id, "chapterNumber": number, "title": title,
            "version": version, "baseProjectRevision": context.expected_revision,
            "body": body, "sceneGroups": scene_groups, "sourcePlanRefs": source_plan_refs,
            "completeness": completeness, "source": if context.author { "author" } else { "agent" }});
        let object = ObjectId::parse(format!("draft-{chapter_id}"))
            .map_err(|e| RepositoryError::InvalidContent(e.to_string()))?;
        self.save(
            context,
            object,
            "draft".into(),
            envelope,
            request_digest,
            Some(ChapterProjection {
                chapter_id,
                number,
                title,
            }),
            Vec::new(),
        )
    }

    pub fn save_artifact(
        &mut self,
        submission: ArtifactSubmission,
    ) -> Result<CandidateReceipt, CandidateWriteError> {
        let ArtifactSubmission {
            context,
            kind,
            payload,
            linked_impact_ids,
        } = submission;
        if !matches!(
            kind.as_str(),
            "brief" | "outline" | "plan" | "identity" | "propertyRules"
        ) || !payload.is_object()
        {
            return Err(RepositoryError::InvalidObjectKind.into());
        }
        let version = self.version(context.expected_candidate.as_ref())?;
        let request_digest = digest(&json!({"kind": kind, "payload": payload,
            "expectedRevision": context.expected_revision,
            "expectedCandidateRevisionId": context.expected_candidate.as_ref().map(RevisionId::as_str),
            "linkedImpactIds": linked_impact_ids}));
        let envelope = json!({"version": version, "baseProjectRevision": context.expected_revision,
            "payload": payload, "source": if context.author { "author" } else { "agent" }});
        if envelope.to_string().len() > 48 * 1024 {
            return Err(CandidateWriteError::ArtifactTooLarge);
        }
        let object = ObjectId::parse(format!("artifact-{kind}"))
            .map_err(|e| RepositoryError::InvalidContent(e.to_string()))?;
        self.save(
            context,
            object,
            kind,
            envelope,
            request_digest,
            None,
            linked_impact_ids,
        )
    }

    fn version(&self, expected: Option<&RevisionId>) -> Result<u64, CandidateWriteError> {
        let Some(expected) = expected else {
            return Ok(1);
        };
        let content = self
            .repository
            .revision_content(expected)?
            .ok_or(CandidateWriteError::MissingExpectedRevision)?;
        let previous: Value =
            serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
        previous["version"]
            .as_u64()
            .filter(|v| *v > 0)
            .and_then(|v| v.checked_add(1))
            .ok_or(RepositoryError::CorruptState.into())
    }

    fn save(
        &mut self,
        context: CandidateContext,
        object_id: ObjectId,
        kind: String,
        envelope: Value,
        request_digest: String,
        chapter_projection: Option<ChapterProjection>,
        linked_impact_ids: Vec<String>,
    ) -> Result<CandidateReceipt, CandidateWriteError> {
        let branch_id = BranchId::main();
        let head = self
            .repository
            .branch_head(&context.project_id, &branch_id)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        let suffix = Uuid::new_v4().simple().to_string();
        let revision_id = RevisionId::parse(format!("{kind}-{suffix}"))
            .map_err(|e| RepositoryError::InvalidContent(e.to_string()))?;
        let commit_id = CommitId::parse(format!("candidate-{suffix}"))
            .map_err(|e| RepositoryError::InvalidContent(e.to_string()))?;
        let next_snapshot_id = SnapshotId::parse(format!("snapshot-{suffix}"))
            .map_err(|e| RepositoryError::InvalidContent(e.to_string()))?;
        Ok(self.repository.save_candidate(SaveCandidateRequest {
            project_id: context.project_id,
            branch_id,
            object_id,
            object_kind: kind,
            revision_id,
            commit_id,
            idempotency_key: context.idempotency_key,
            request_digest,
            expected_project_revision: context.expected_revision,
            expected_snapshot_id: head.snapshot_id,
            next_snapshot_id,
            expected_candidate_revision_id: context.expected_candidate,
            content_digest: digest(&envelope),
            content_json: envelope.to_string(),
            chapter_projection,
            task_output: context.task_output,
            linked_impact_ids,
        })?)
    }
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

use crate::{ProjectRevisionRepository};
