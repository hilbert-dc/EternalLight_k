use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::BranchId;
use crate::CommitId;
use crate::IdempotencyKey;
use crate::ObjectId;
use crate::ProjectId;
use crate::RevisionId;
use crate::SnapshotId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionRecord {
    pub project_id: ProjectId,
    pub object_id: ObjectId,
    pub revision_id: RevisionId,
    pub parent_revision_id: Option<RevisionId>,
    pub content_digest: String,
    /// Canonical JSON owned by Story Core. The digest is supplied separately so
    /// callers can bind request identities without reparsing the document.
    pub content_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub snapshot_id: SnapshotId,
    pub parent_snapshot_id: Option<SnapshotId>,
    pub members: BTreeMap<ObjectId, RevisionId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRequest {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub base_snapshot_id: SnapshotId,
    pub next_snapshot_id: SnapshotId,
    pub revisions: Vec<RevisionRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoRequest {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub commit_id: CommitId,
    pub target_commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub base_snapshot_id: SnapshotId,
    pub next_snapshot_id: SnapshotId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitStatus {
    Applied,
    Undone,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    pub commit_id: CommitId,
    pub result_snapshot_id: SnapshotId,
    pub status: CommitStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryError {
    AlreadyExists(&'static str, String),
    BranchNotFound,
    CandidateNotFound,
    CommitNotFound,
    CorruptState,
    CrossProjectReference,
    DuplicateObjectRevision(ObjectId),
    EmptyCommit,
    InvalidContent(String),
    InvalidChapter,
    InvalidObjectKind,
    IdempotencyConflict,
    ObjectLocked,
    ProjectNotFound,
    ReviewNotFound,
    ReviewChecksFailed,
    TaskBindingInvalid,
    TaskCapabilityInvalid,
    TaskNotWritable,
    TaskOutputForbidden,
    TaskOutputMissing,
    TaskNotFound,
    InvalidParent {
        object_id: ObjectId,
        expected: Option<RevisionId>,
        actual: Option<RevisionId>,
    },
    StaleSnapshot {
        expected: SnapshotId,
        actual: SnapshotId,
    },
    StaleProjectRevision {
        expected: i64,
        actual: i64,
    },
    StaleCandidate {
        expected: Option<RevisionId>,
        actual: Option<RevisionId>,
    },
    UndoRequiresLatestCommit,
    Storage(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for RepositoryError {}

#[derive(Clone, Debug)]
struct CommitRecord {
    project_id: ProjectId,
    branch_id: BranchId,
    base_snapshot_id: SnapshotId,
    result_snapshot_id: SnapshotId,
}

#[derive(Default)]
pub struct StoryRepository {
    revisions: HashMap<RevisionId, RevisionRecord>,
    snapshots: HashMap<SnapshotId, Snapshot>,
    branch_heads: HashMap<(ProjectId, BranchId), SnapshotId>,
    commits: HashMap<CommitId, CommitRecord>,
    receipts: HashMap<(ProjectId, BranchId, IdempotencyKey), (String, CommitReceipt)>,
}

impl StoryRepository {
    pub fn create_branch(
        &mut self,
        project_id: ProjectId,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<(), RepositoryError> {
        let key = (project_id.clone(), branch_id.clone());
        if self.branch_heads.contains_key(&key) {
            return Err(RepositoryError::AlreadyExists(
                "branch",
                branch_id.to_string(),
            ));
        }
        if self.snapshots.contains_key(&snapshot_id) {
            return Err(RepositoryError::AlreadyExists(
                "snapshot",
                snapshot_id.to_string(),
            ));
        }
        self.branch_heads.insert(key, snapshot_id.clone());
        self.snapshots.insert(
            snapshot_id.clone(),
            Snapshot {
                project_id,
                branch_id,
                snapshot_id,
                parent_snapshot_id: None,
                members: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn branch_head(&self, project_id: &ProjectId, branch_id: &BranchId) -> Option<&Snapshot> {
        self.branch_heads
            .get(&(project_id.clone(), branch_id.clone()))
            .and_then(|snapshot_id| self.snapshots.get(snapshot_id))
    }

    pub fn apply_commit(
        &mut self,
        request: CommitRequest,
    ) -> Result<CommitReceipt, RepositoryError> {
        if request.revisions.is_empty() {
            return Err(RepositoryError::EmptyCommit);
        }
        if let Some(receipt) = self.replay_receipt(
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        let head = self.require_head(
            &request.project_id,
            &request.branch_id,
            &request.base_snapshot_id,
        )?;
        self.require_unused_ids(&request.commit_id, &request.next_snapshot_id)?;

        let mut members = head.members;
        let mut staged = HashMap::new();
        for revision in &request.revisions {
            if revision.project_id != request.project_id {
                return Err(RepositoryError::CrossProjectReference);
            }
            if staged
                .insert(revision.object_id.clone(), revision.revision_id.clone())
                .is_some()
            {
                return Err(RepositoryError::DuplicateObjectRevision(
                    revision.object_id.clone(),
                ));
            }
            if self.revisions.contains_key(&revision.revision_id) {
                return Err(RepositoryError::AlreadyExists(
                    "revision",
                    revision.revision_id.to_string(),
                ));
            }
            let expected = members.get(&revision.object_id).cloned();
            if revision.parent_revision_id != expected {
                return Err(RepositoryError::InvalidParent {
                    object_id: revision.object_id.clone(),
                    expected,
                    actual: revision.parent_revision_id.clone(),
                });
            }
            members.insert(revision.object_id.clone(), revision.revision_id.clone());
        }

        for revision in request.revisions {
            self.revisions
                .insert(revision.revision_id.clone(), revision);
        }
        let snapshot = Snapshot {
            project_id: request.project_id.clone(),
            branch_id: request.branch_id.clone(),
            snapshot_id: request.next_snapshot_id.clone(),
            parent_snapshot_id: Some(request.base_snapshot_id.clone()),
            members,
        };
        self.snapshots
            .insert(snapshot.snapshot_id.clone(), snapshot);
        self.branch_heads.insert(
            (request.project_id.clone(), request.branch_id.clone()),
            request.next_snapshot_id.clone(),
        );
        self.commits.insert(
            request.commit_id.clone(),
            CommitRecord {
                project_id: request.project_id.clone(),
                branch_id: request.branch_id.clone(),
                base_snapshot_id: request.base_snapshot_id,
                result_snapshot_id: request.next_snapshot_id.clone(),
            },
        );
        Ok(self.store_receipt(
            request.project_id,
            request.branch_id,
            request.idempotency_key,
            request.request_digest,
            CommitReceipt {
                commit_id: request.commit_id,
                result_snapshot_id: request.next_snapshot_id,
                status: CommitStatus::Applied,
            },
        ))
    }

    pub fn undo_latest(&mut self, request: UndoRequest) -> Result<CommitReceipt, RepositoryError> {
        if let Some(receipt) = self.replay_receipt(
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        self.require_head(
            &request.project_id,
            &request.branch_id,
            &request.base_snapshot_id,
        )?;
        self.require_unused_ids(&request.commit_id, &request.next_snapshot_id)?;
        let target = self
            .commits
            .get(&request.target_commit_id)
            .cloned()
            .ok_or(RepositoryError::CommitNotFound)?;
        if target.project_id != request.project_id || target.branch_id != request.branch_id {
            return Err(RepositoryError::CrossProjectReference);
        }
        if target.result_snapshot_id != request.base_snapshot_id {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        let restored_members = self
            .snapshots
            .get(&target.base_snapshot_id)
            .ok_or(RepositoryError::CorruptState)?
            .members
            .clone();
        self.snapshots.insert(
            request.next_snapshot_id.clone(),
            Snapshot {
                project_id: request.project_id.clone(),
                branch_id: request.branch_id.clone(),
                snapshot_id: request.next_snapshot_id.clone(),
                parent_snapshot_id: Some(request.base_snapshot_id.clone()),
                members: restored_members,
            },
        );
        self.branch_heads.insert(
            (request.project_id.clone(), request.branch_id.clone()),
            request.next_snapshot_id.clone(),
        );
        self.commits.insert(
            request.commit_id.clone(),
            CommitRecord {
                project_id: request.project_id.clone(),
                branch_id: request.branch_id.clone(),
                base_snapshot_id: request.base_snapshot_id,
                result_snapshot_id: request.next_snapshot_id.clone(),
            },
        );
        Ok(self.store_receipt(
            request.project_id,
            request.branch_id,
            request.idempotency_key,
            request.request_digest,
            CommitReceipt {
                commit_id: request.commit_id,
                result_snapshot_id: request.next_snapshot_id,
                status: CommitStatus::Undone,
            },
        ))
    }

    fn require_head(
        &self,
        project_id: &ProjectId,
        branch_id: &BranchId,
        expected: &SnapshotId,
    ) -> Result<Snapshot, RepositoryError> {
        let actual = self
            .branch_heads
            .get(&(project_id.clone(), branch_id.clone()))
            .ok_or(RepositoryError::BranchNotFound)?;
        if actual != expected {
            return Err(RepositoryError::StaleSnapshot {
                expected: expected.clone(),
                actual: actual.clone(),
            });
        }
        self.snapshots
            .get(actual)
            .cloned()
            .ok_or(RepositoryError::CorruptState)
    }

    fn require_unused_ids(
        &self,
        commit_id: &CommitId,
        snapshot_id: &SnapshotId,
    ) -> Result<(), RepositoryError> {
        if self.commits.contains_key(commit_id) {
            return Err(RepositoryError::AlreadyExists(
                "commit",
                commit_id.to_string(),
            ));
        }
        if self.snapshots.contains_key(snapshot_id) {
            return Err(RepositoryError::AlreadyExists(
                "snapshot",
                snapshot_id.to_string(),
            ));
        }
        Ok(())
    }

    fn replay_receipt(
        &self,
        project_id: &ProjectId,
        branch_id: &BranchId,
        key: &IdempotencyKey,
        digest: &str,
    ) -> Result<Option<CommitReceipt>, RepositoryError> {
        let Some((stored_digest, receipt)) =
            self.receipts
                .get(&(project_id.clone(), branch_id.clone(), key.clone()))
        else {
            return Ok(None);
        };
        if stored_digest != digest {
            return Err(RepositoryError::IdempotencyConflict);
        }
        Ok(Some(receipt.clone()))
    }

    fn store_receipt(
        &mut self,
        project_id: ProjectId,
        branch_id: BranchId,
        key: IdempotencyKey,
        digest: String,
        receipt: CommitReceipt,
    ) -> CommitReceipt {
        self.receipts
            .insert((project_id, branch_id, key), (digest, receipt.clone()));
        receipt
    }
}
