use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    BranchId, CandidateReceipt, CommitId, IdempotencyKey, ObjectId, ProjectId, RepositoryError,
    RevisionId, RevisionRecord, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
};

/// Immutable origin of an author-established commitment, not an author confirmation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitmentExtractionOrigin {
    pub chapter_id: String,
    pub report_revision_id: String,
    pub proposal_id: String,
}

pub struct EstablishCommitment {
    pub project_id: ProjectId,
    pub origin: CommitmentExtractionOrigin,
    pub expected_project_revision: i64,
    pub base_snapshot_id: SnapshotId,
    pub idempotency_key: IdempotencyKey,
}

pub struct EstablishedCommitment {
    pub commitment_id: String,
    pub receipt: CandidateReceipt,
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("invalid commitment extraction origin".into())
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn identity(project: &ProjectId, origin: &CommitmentExtractionOrigin) -> String {
    let hash = digest(
        &json!([
            project.as_str(),
            origin.report_revision_id,
            origin.proposal_id
        ])
        .to_string(),
    );
    format!("extracted-{hash}")
}

fn proposal(
    db: &Connection,
    project: &ProjectId,
    origin: &CommitmentExtractionOrigin,
) -> Result<Value, RepositoryError> {
    let content: Option<String> = db.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND revision_id=?2 AND object_id='commitment-extraction-' || ?3 AND object_kind='commitmentExtraction'",
        params![project.as_str(), origin.report_revision_id, origin.chapter_id], |row|row.get(0),
    ).optional().map_err(storage)?;
    let content: Value = serde_json::from_str(&content.ok_or_else(invalid)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    let candidate = content["payload"]["candidates"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
        .iter()
        .find(|candidate| candidate["id"] == origin.proposal_id)
        .ok_or_else(invalid)?;
    let mut candidate = candidate.clone();
    candidate["id"] = json!(identity(project, origin));
    Ok(candidate)
}

fn selected(
    db: &Connection,
    project: &ProjectId,
    snapshot: &str,
    origin: &CommitmentExtractionOrigin,
) -> Result<bool, RepositoryError> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_snapshots_v2 s JOIN story_snapshot_members_v2 m ON m.snapshot_id=s.snapshot_id WHERE s.project_id=?1 AND s.snapshot_id=?2 AND s.branch_id='main' AND m.object_id='commitment-extraction-' || ?3 AND m.revision_id=?4)",
        params![project.as_str(), snapshot, origin.chapter_id, origin.report_revision_id], |row|row.get(0),
    ).map_err(storage)
}

pub(crate) fn validate_origin(
    db: &Connection,
    project: &ProjectId,
    id: &str,
    origin: &CommitmentExtractionOrigin,
) -> Result<(), RepositoryError> {
    if id != identity(project, origin) {
        return Err(invalid());
    }
    proposal(db, project, origin)?;
    Ok(())
}

pub(crate) fn validate_revision(
    db: &Connection,
    revision: &RevisionRecord,
) -> Result<(), RepositoryError> {
    let content: Value = serde_json::from_str(&revision.content_json).map_err(|_| invalid())?;
    let origin = content
        .get("extractionOrigin")
        .filter(|origin| !origin.is_null());
    if let Some(parent) = &revision.parent_revision_id {
        let previous: String = db.query_row("SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",
            params![revision.project_id.as_str(), revision.object_id.as_str(),parent.as_str()], |row|row.get(0)).map_err(storage)?;
        let previous: Value =
            serde_json::from_str(&previous).map_err(|_| RepositoryError::CorruptState)?;
        if previous
            .get("extractionOrigin")
            .filter(|origin| !origin.is_null())
            != origin
        {
            return Err(invalid());
        }
    } else if let Some(origin) = origin {
        let origin: CommitmentExtractionOrigin =
            serde_json::from_value(origin.clone()).map_err(|_| invalid())?;
        let snapshot: String = db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [revision.project_id.as_str()], |row|row.get(0)).map_err(storage)?;
        if !selected(db, &revision.project_id, &snapshot, &origin)? {
            return Err(RepositoryError::TaskInputStale);
        }
        if content["payload"] != proposal(db, &revision.project_id, &origin)? {
            return Err(invalid());
        }
    }
    Ok(())
}

impl SqliteStoryRepository {
    /// Establishes one frozen proposal as an independent candidate. It never
    /// accepts a Review or changes Canon; normal author confirmation is separate.
    pub fn establish_commitment(
        &mut self,
        request: EstablishCommitment,
    ) -> Result<EstablishedCommitment, RepositoryError> {
        let payload = self.with_read_snapshot(|| {
            if !selected(
                &self.connection,
                &request.project_id,
                request.base_snapshot_id.as_str(),
                &request.origin,
            )? {
                return Err(RepositoryError::CandidateNotFound);
            }
            proposal(&self.connection, &request.project_id, &request.origin)
        })??;
        let commitment_id = identity(&request.project_id, &request.origin);
        let content = json!({"payload":payload,"extractionOrigin":request.origin}).to_string();
        let request_digest = digest(&json!({"operation":"establishCommitment","origin":request.origin,
            "baseSnapshotId":request.base_snapshot_id.as_str(),"expectedRevision":request.expected_project_revision}).to_string());
        let token = Uuid::new_v4().simple().to_string();
        let receipt = self.save_candidate(SaveCandidateRequest {
            project_id: request.project_id,
            branch_id: BranchId::parse("main").expect("static branch"),
            object_id: ObjectId::parse(format!("commitment-{commitment_id}"))
                .map_err(|_| invalid())?,
            object_kind: "commitment".into(),
            revision_id: RevisionId::parse(format!("established-{token}"))
                .expect("generated revision"),
            commit_id: CommitId::parse(format!("establish-{token}")).expect("generated commit"),
            next_snapshot_id: SnapshotId::parse(format!("establish-snapshot-{token}"))
                .expect("generated snapshot"),
            idempotency_key: request.idempotency_key,
            request_digest,
            expected_project_revision: request.expected_project_revision,
            expected_snapshot_id: request.base_snapshot_id,
            expected_candidate_revision_id: None,
            content_digest: digest(&content),
            content_json: content,
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: Vec::new(),
        })?;
        Ok(EstablishedCommitment {
            commitment_id,
            receipt,
        })
    }
}
