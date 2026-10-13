use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;

use crate::BranchId;
use crate::CommitId;
use crate::CommitReceipt;
use crate::CommitRequest;
use crate::CommitStatus;
use crate::IdempotencyKey;
use crate::ObjectId;
use crate::ProjectId;
use crate::RepositoryError;
use crate::RevisionId;
use crate::RevisionRecord;
use crate::Snapshot;
use crate::SnapshotId;
use crate::UndoRequest;

/// Durable Story Core repository. Every state transition is committed in one
/// SQLite transaction; callers never need to mirror an in-memory authority.
pub struct SqliteStoryRepository {
    pub(crate) connection: crate::database::ConnectionLease,
    pub(crate) recovery_operation: Option<crate::RecoveryOperationBinding>,
    pub(crate) generation_work_control_guard: Option<(String, i64)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecord {
    pub project_id: ProjectId,
    pub title: String,
    pub logline: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectHead {
    pub project_id: ProjectId,
    pub object_id: ObjectId,
    pub object_kind: String,
    pub candidate_revision_id: Option<RevisionId>,
    pub accepted_revision_id: Option<RevisionId>,
    pub locked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveCandidateRequest {
    pub project_id: ProjectId,
    pub branch_id: BranchId,
    pub object_id: ObjectId,
    pub object_kind: String,
    pub revision_id: RevisionId,
    pub commit_id: CommitId,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: String,
    pub expected_project_revision: i64,
    pub expected_snapshot_id: SnapshotId,
    pub next_snapshot_id: SnapshotId,
    pub expected_candidate_revision_id: Option<RevisionId>,
    pub content_digest: String,
    pub content_json: String,
    pub chapter_projection: Option<ChapterProjection>,
    pub task_output: Option<TaskOutputBinding>,
    pub linked_impact_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateReceipt {
    pub revision_id: RevisionId,
    pub result_snapshot_id: SnapshotId,
    pub project_revision: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChapterProjection {
    pub chapter_id: String,
    pub number: i64,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChapterRecord {
    pub chapter_id: String,
    pub project_id: ProjectId,
    pub number: i64,
    pub title: String,
    pub status: String,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskOutputBinding {
    pub task_id: String,
    pub thread_id: String,
    pub output_kind: String,
    pub completeness: Option<String>,
}

impl SqliteStoryRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        let connection = Connection::open(path).map_err(storage_error)?;
        let repository = Self::from_connection(connection)?;
        // The file store serves foreground writes and background audits together.
        // Re-selecting WAL on every read connection would itself contend for locks.
        let mode: String = repository
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(storage_error)?;
        if mode != "wal" {
            let selected: String = repository
                .connection
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .map_err(storage_error)?;
            if selected != "wal" {
                return Err(RepositoryError::Storage(
                    "file story storage requires WAL journal mode".into(),
                ));
            }
        }
        Ok(repository)
    }

    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, RepositoryError> {
        crate::current_schema::validate(&connection)?;
        crate::database::configure(&connection)?;
        let transaction = connection.transaction().map_err(storage_error)?;
        transaction
            .execute_batch(crate::schema::ddl())
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(Self::with_connection(connection.into()))
    }

    pub(crate) fn with_connection(connection: crate::database::ConnectionLease) -> Self {
        Self {
            connection,
            recovery_operation: None,
            generation_work_control_guard: None,
        }
    }

}

fn replay_candidate_receipt(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    key: &IdempotencyKey,
    digest: &str,
) -> Result<Option<CandidateReceipt>, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT request_digest, revision_id, result_snapshot_id, project_revision FROM story_candidate_receipts_v2 WHERE project_id = ?1 AND branch_id = ?2 AND idempotency_key = ?3",
            params![project_id.as_str(), branch_id.as_str(), key.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((stored_digest, revision_id, snapshot_id, project_revision)) = row else {
        return Ok(None);
    };
    if stored_digest != digest {
        return Err(RepositoryError::IdempotencyConflict);
    }
    Ok(Some(CandidateReceipt {
        revision_id: RevisionId::parse(revision_id).map_err(|_| RepositoryError::CorruptState)?,
        result_snapshot_id: SnapshotId::parse(snapshot_id)
            .map_err(|_| RepositoryError::CorruptState)?,
        project_revision,
    }))
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRecord> {
    let project_id = row.get::<_, String>(0)?;
    Ok(ProjectRecord {
        project_id: ProjectId::parse(project_id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        title: row.get(1)?,
        logline: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn object_head_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectHead> {
    let parse_id = |index, value: String| {
        ProjectId::parse(value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    };
    let project_id = parse_id(0, row.get(0)?)?;
    let object_id = ObjectId::parse(row.get::<_, String>(1)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let candidate = row
        .get::<_, Option<String>>(3)?
        .map(RevisionId::parse)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let accepted = row
        .get::<_, Option<String>>(4)?
        .map(RevisionId::parse)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(ObjectHead {
        project_id,
        object_id,
        object_kind: row.get(2)?,
        candidate_revision_id: candidate,
        accepted_revision_id: accepted,
        locked: row.get(5)?,
    })
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn exists<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    parameters: P,
) -> Result<bool, RepositoryError> {
    connection
        .query_row(sql, parameters, |_| Ok(()))
        .optional()
        .map(|row| row.is_some())
        .map_err(storage_error)
}

fn require_unused_commit(
    connection: &Connection,
    commit_id: &CommitId,
) -> Result<(), RepositoryError> {
    if exists(
        connection,
        "SELECT 1 FROM story_commits_v2 WHERE commit_id = ?1",
        [commit_id.as_str()],
    )? {
        return Err(RepositoryError::AlreadyExists(
            "commit",
            commit_id.to_string(),
        ));
    }
    Ok(())
}

fn require_unused_snapshot(
    connection: &Connection,
    snapshot_id: &SnapshotId,
) -> Result<(), RepositoryError> {
    if exists(
        connection,
        "SELECT 1 FROM story_snapshots_v2 WHERE snapshot_id = ?1",
        [snapshot_id.as_str()],
    )? {
        return Err(RepositoryError::AlreadyExists(
            "snapshot",
            snapshot_id.to_string(),
        ));
    }
    Ok(())
}

fn require_head(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    expected: &SnapshotId,
) -> Result<Snapshot, RepositoryError> {
    let actual = connection
        .query_row(
            "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
            params![project_id.as_str(), branch_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::BranchNotFound)?;
    if actual != expected.as_str() {
        return Err(RepositoryError::StaleSnapshot {
            expected: expected.clone(),
            actual: SnapshotId::parse(actual).map_err(|_| RepositoryError::CorruptState)?,
        });
    }
    load_snapshot(connection, expected.as_str())
}

fn load_snapshot(connection: &Connection, snapshot_id: &str) -> Result<Snapshot, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT project_id, branch_id, parent_snapshot_id FROM story_snapshots_v2 WHERE snapshot_id = ?1",
            [snapshot_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::CorruptState)?;
    Ok(Snapshot {
        project_id: ProjectId::parse(row.0).map_err(|_| RepositoryError::CorruptState)?,
        branch_id: BranchId::parse(row.1).map_err(|_| RepositoryError::CorruptState)?,
        snapshot_id: SnapshotId::parse(snapshot_id).map_err(|_| RepositoryError::CorruptState)?,
        parent_snapshot_id: row
            .2
            .map(SnapshotId::parse)
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?,
        members: load_members(connection, snapshot_id)?,
    })
}

fn load_members(
    connection: &Connection,
    snapshot_id: &str,
) -> Result<BTreeMap<ObjectId, RevisionId>, RepositoryError> {
    let mut statement = connection
        .prepare("SELECT object_id, revision_id FROM story_snapshot_members_v2 WHERE snapshot_id = ?1 ORDER BY object_id")
        .map_err(storage_error)?;
    let rows = statement
        .query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    rows.map(|row| {
        let (object_id, revision_id) = row.map_err(storage_error)?;
        Ok((
            ObjectId::parse(object_id).map_err(|_| RepositoryError::CorruptState)?,
            RevisionId::parse(revision_id).map_err(|_| RepositoryError::CorruptState)?,
        ))
    })
    .collect()
}

fn insert_revision(
    connection: &Connection,
    revision: &RevisionRecord,
) -> Result<(), RepositoryError> {
    if revision.object_kind.trim().is_empty() {
        return Err(RepositoryError::InvalidObjectKind);
    }
    if revision.object_kind == "propertyRules" {
        crate::property_rules::validate_write(&revision.object_id, &revision.content_json)?;
    }
    if revision.object_kind == "commitment" {
        crate::commitment_establishment::validate_revision(connection, revision)?;
        crate::narrative_commitments::validate_write(
            connection,
            &revision.project_id,
            &revision.object_id,
            &revision.content_json,
        )?;
    }
    if revision.object_kind == "commitmentExtraction" {
        crate::commitment_extraction::validate_write(
            connection,
            &revision.project_id,
            &revision.object_id,
            &revision.content_json,
        )?;
    }
    if let Some(parent_revision_id) = &revision.parent_revision_id {
        let parent_kind: String = connection
            .query_row(
                "SELECT object_kind FROM story_revisions_v2 WHERE revision_id = ?1",
                [parent_revision_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if parent_kind != revision.object_kind {
            return Err(RepositoryError::InvalidObjectKind);
        }
    }
    serde_json::from_str::<serde_json::Value>(&revision.content_json)
        .map_err(|error| RepositoryError::Storage(format!("invalid revision JSON: {error}")))?;
    connection
        .execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, parent_revision_id, content_digest, content_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![revision.revision_id.as_str(), revision.project_id.as_str(), revision.object_id.as_str(), revision.object_kind, revision.parent_revision_id.as_ref().map(RevisionId::as_str), revision.content_digest, revision.content_json],
        )
        .map_err(storage_error)?;
    crate::search_index::index_revision(connection, revision)?;
    Ok(())
}

fn current_project_revision(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)
}

fn insert_snapshot(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    snapshot_id: &SnapshotId,
    parent_snapshot_id: Option<&SnapshotId>,
    members: &BTreeMap<ObjectId, RevisionId>,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot_id.as_str(), project_id.as_str(), branch_id.as_str(), parent_snapshot_id.map(SnapshotId::as_str)],
        )
        .map_err(storage_error)?;
    for (object_id, revision_id) in members {
        connection
            .execute(
                "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
                params![snapshot_id.as_str(), object_id.as_str(), revision_id.as_str()],
            )
            .map_err(storage_error)?;
    }
    Ok(())
}

fn replay_receipt(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    key: &IdempotencyKey,
    digest: &str,
) -> Result<Option<CommitReceipt>, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT request_digest, commit_id, result_snapshot_id, status FROM story_receipts_v2 WHERE project_id = ?1 AND branch_id = ?2 AND idempotency_key = ?3",
            params![project_id.as_str(), branch_id.as_str(), key.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((stored_digest, commit_id, snapshot_id, status)) = row else {
        return Ok(None);
    };
    if stored_digest != digest {
        return Err(RepositoryError::IdempotencyConflict);
    }
    Ok(Some(CommitReceipt {
        commit_id: CommitId::parse(commit_id).map_err(|_| RepositoryError::CorruptState)?,
        result_snapshot_id: SnapshotId::parse(snapshot_id)
            .map_err(|_| RepositoryError::CorruptState)?,
        status: match status.as_str() {
            "applied" => CommitStatus::Applied,
            "undone" => CommitStatus::Undone,
            _ => return Err(RepositoryError::CorruptState),
        },
    }))
}

fn insert_receipt(
    connection: &Connection,
    project_id: &ProjectId,
    branch_id: &BranchId,
    key: &IdempotencyKey,
    digest: &str,
    receipt: &CommitReceipt,
) -> Result<(), RepositoryError> {
    let status = match receipt.status {
        CommitStatus::Applied => "applied",
        CommitStatus::Undone => "undone",
    };
    connection
        .execute(
            "INSERT INTO story_receipts_v2(project_id, branch_id, idempotency_key, request_digest, commit_id, result_snapshot_id, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![project_id.as_str(), branch_id.as_str(), key.as_str(), digest, receipt.commit_id.as_str(), receipt.result_snapshot_id.as_str(), status],
        )
        .map_err(storage_error)?;
    Ok(())
}

/// Public sqlite repository capability; SQLite retains transaction ownership.
pub trait ProjectRevisionRepository {
    fn create_project(
        &mut self,
        project: ProjectRecord,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<ProjectRecord, RepositoryError>;
    fn project_revision(&self, project_id: &ProjectId) -> Result<i64, RepositoryError>;
    fn object_heads(&self, project_id: &ProjectId) -> Result<Vec<ObjectHead>, RepositoryError>;
    fn set_object_lock(
        &mut self,
        project_id: &ProjectId,
        object_id: &ObjectId,
        locked: bool,
        expected_project_revision: i64,
    ) -> Result<i64, RepositoryError>;
    fn save_candidate(
        &mut self,
        request: SaveCandidateRequest,
    ) -> Result<CandidateReceipt, RepositoryError>;
    fn chapters(&self, project_id: &ProjectId) -> Result<Vec<ChapterRecord>, RepositoryError>;
    fn project(&self, project_id: &ProjectId) -> Result<Option<ProjectRecord>, RepositoryError>;
    fn list_projects(&self) -> Result<Vec<ProjectRecord>, RepositoryError>;
    fn create_branch(
        &mut self,
        project_id: ProjectId,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<(), RepositoryError>;
    fn branch_head(
        &self,
        project_id: &ProjectId,
        branch_id: &BranchId,
    ) -> Result<Option<Snapshot>, RepositoryError>;
    fn revision_content(&self, revision_id: &RevisionId)
    -> Result<Option<String>, RepositoryError>;
    fn apply_commit(&mut self, request: CommitRequest) -> Result<CommitReceipt, RepositoryError>;
    fn undo_latest(&mut self, request: UndoRequest) -> Result<CommitReceipt, RepositoryError>;
}

impl ProjectRevisionRepository for SqliteStoryRepository {
    fn create_project(
        &mut self,
        project: ProjectRecord,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<ProjectRecord, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if exists(
            &transaction,
            "SELECT 1 FROM story_projects_v2 WHERE project_id = ?1",
            [project.project_id.as_str()],
        )? {
            return Err(RepositoryError::AlreadyExists(
                "project",
                project.project_id.to_string(),
            ));
        }
        require_unused_snapshot(&transaction, &snapshot_id)?;
        transaction
            .execute(
                "INSERT INTO story_projects_v2(project_id, title, logline, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project.project_id.as_str(), project.title, project.logline, project.created_at, project.updated_at],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_workspaces_v2(project_id, revision) VALUES (?1, 0)",
                [project.project_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, NULL)",
                params![snapshot_id.as_str(), project.project_id.as_str(), branch_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_branches_v2(project_id, branch_id, head_snapshot_id) VALUES (?1, ?2, ?3)",
                params![project.project_id.as_str(), branch_id.as_str(), snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        crate::multimedia::initialize_project(
            &transaction,
            project.project_id.as_str(),
            &project.title,
            branch_id.as_str(),
            crate::CreativeMedium::Novel,
            project.created_at,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(project)
    }

    fn project_revision(&self, project_id: &ProjectId) -> Result<i64, RepositoryError> {
        self.connection
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)
    }

    fn object_heads(&self, project_id: &ProjectId) -> Result<Vec<ObjectHead>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT project_id, object_id, object_kind, candidate_revision_id, accepted_revision_id, locked FROM story_object_heads_v2 WHERE project_id = ?1 ORDER BY object_kind, object_id",
            )
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], object_head_from_row)
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect()
    }

    fn set_object_lock(
        &mut self,
        project_id: &ProjectId,
        object_id: &ObjectId,
        locked: bool,
        expected_project_revision: i64,
    ) -> Result<i64, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let revision = current_project_revision(&transaction, project_id)?;
        if revision != expected_project_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: expected_project_revision,
                actual: revision,
            });
        }
        let accepted = transaction
            .query_row(
                "SELECT accepted_revision_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![project_id.as_str(), object_id.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(storage_error)?
            .flatten()
            .ok_or(RepositoryError::CandidateNotFound)?;
        let current = transaction
            .query_row(
                "SELECT locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2 AND accepted_revision_id = ?3",
                params![project_id.as_str(), object_id.as_str(), accepted],
                |row| row.get::<_, bool>(0),
            )
            .map_err(storage_error)?;
        if current == locked {
            return Ok(revision);
        }
        let next = revision + 1;
        transaction
            .execute(
                "UPDATE story_object_heads_v2 SET locked = ?1 WHERE project_id = ?2 AND object_id = ?3",
                params![locked, project_id.as_str(), object_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision = ?1 WHERE project_id = ?2",
                params![next, project_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(next)
    }

    fn save_candidate(
        &mut self,
        request: SaveCandidateRequest,
    ) -> Result<CandidateReceipt, RepositoryError> {
        serde_json::from_str::<serde_json::Value>(&request.content_json)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        if request.object_kind.trim().is_empty() {
            return Err(RepositoryError::InvalidObjectKind);
        }
        if request.object_kind == "sceneAssessment" && request.task_output.is_none() {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request.object_kind == "commitmentExtraction"
            && (request.branch_id.as_str() != "main"
                || request.chapter_projection.is_some()
                || request
                    .task_output
                    .as_ref()
                    .is_none_or(|binding| binding.output_kind != "commitmentExtraction"))
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "commitmentExtraction")
            && request.object_kind != "commitmentExtraction"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "sceneAssessment")
            && request.object_kind != "sceneAssessment"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request.object_kind == "revisionOutcome"
            && (request.branch_id.as_str() != "main"
                || request.chapter_projection.is_some()
                || request
                    .task_output
                    .as_ref()
                    .is_none_or(|binding| binding.output_kind != "revisionOutcome"))
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "revisionOutcome")
            && request.object_kind != "revisionOutcome"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request.object_kind == "identity" {
            if request.object_id.as_str() != "artifact-identity" {
                return Err(RepositoryError::InvalidObjectKind);
            }
            crate::identity_decisions::validate_identity_content(&request.content_json)?;
        }
        if request.object_kind == "propertyRules" {
            if request.branch_id.as_str() != "main" || request.chapter_projection.is_some() {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            crate::property_rules::validate_write(&request.object_id, &request.content_json)?;
            let content: serde_json::Value = serde_json::from_str(&request.content_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            let source_valid = match (content["source"].as_str(), request.task_output.as_ref()) {
                (Some("author"), None) => true,
                (Some("agent"), Some(binding)) => binding.output_kind == "propertyRules",
                _ => false,
            };
            if !source_valid {
                return Err(RepositoryError::TaskOutputForbidden);
            }
        }
        if request.object_kind == "commitment"
            && (request.branch_id.as_str() != "main"
                || request
                    .task_output
                    .as_ref()
                    .is_some_and(|binding| binding.output_kind != "commitment")
                || request.chapter_projection.is_some())
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if request
            .task_output
            .as_ref()
            .is_some_and(|binding| binding.output_kind == "commitment")
            && request.object_kind != "commitment"
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(receipt) = replay_candidate_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            crate::recovery_operations::commit_candidate(
                &transaction,
                self.recovery_operation.as_ref(),
                &request.project_id,
                request.task_output.as_ref(),
                &request.object_kind,
                receipt.revision_id.as_str(),
            )?;
            transaction.commit().map_err(storage_error)?;
            return Ok(receipt);
        }
        let project_revision = transaction
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [request.project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        if project_revision != request.expected_project_revision {
            return Err(RepositoryError::StaleProjectRevision {
                expected: request.expected_project_revision,
                actual: project_revision,
            });
        }
        if let Some(binding) = &request.task_output
            && let Err(error) = crate::tasks::validate_task_write(
                &transaction,
                &request.project_id,
                binding,
                project_revision,
                &request.object_id,
                &request.expected_candidate_revision_id,
                &request.content_json,
            )
        {
            // Task validation precedes every candidate domain write. Only these
            // constructor-owned rejections prove this exact call has no receipt.
            if matches!(
                &error,
                RepositoryError::InvalidContent(_)
                    | RepositoryError::PayloadValidation { .. }
                    | RepositoryError::TaskBindingInvalid
                    | RepositoryError::TaskInputStale
                    | RepositoryError::TaskNotWritable
                    | RepositoryError::TaskOutputForbidden
                    | RepositoryError::TaskOutputMissing
                    | RepositoryError::TaskOutputIncomplete
                    | RepositoryError::WorkTaskStale
                    | RepositoryError::AuthoringRunStale
                    | RepositoryError::StaleProjectRevision { .. }
            ) {
                crate::recovery_operations::reject_operation(
                    &transaction,
                    self.recovery_operation.as_ref(),
                    &request.project_id,
                    &binding.task_id,
                    crate::recovery_operations::candidate_tool(&request.object_kind)?,
                )?;
                transaction.commit().map_err(storage_error)?;
            }
            return Err(error);
        }
        let head = require_head(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.expected_snapshot_id,
        )?;
        require_unused_commit(&transaction, &request.commit_id)?;
        require_unused_snapshot(&transaction, &request.next_snapshot_id)?;
        if let Some(chapter) = &request.chapter_projection {
            crate::chapter_deletion::assert_not_deleted(&transaction, &chapter.chapter_id)?;
            if chapter.number < 1 || chapter.title.chars().count() > 160 {
                return Err(RepositoryError::InvalidChapter);
            }
            if exists(
                &transaction,
                "SELECT 1 FROM story_chapters_v2 WHERE project_id = ?1 AND number = ?2 AND chapter_id <> ?3",
                params![
                    request.project_id.as_str(),
                    chapter.number,
                    chapter.chapter_id
                ],
            )? {
                return Err(RepositoryError::InvalidChapter);
            }
            let existing_project = transaction
                .query_row(
                    "SELECT project_id FROM story_chapters_v2 WHERE chapter_id = ?1",
                    [&chapter.chapter_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(storage_error)?;
            match existing_project {
                Some(existing_project) if existing_project != request.project_id.as_str() => {
                    return Err(RepositoryError::CrossProjectReference);
                }
                Some(_) => {
                    transaction
                        .execute(
                            "UPDATE story_chapters_v2 SET number = ?1, title = ?2 WHERE project_id = ?3 AND chapter_id = ?4",
                            params![chapter.number, chapter.title, request.project_id.as_str(), chapter.chapter_id],
                        )
                        .map_err(storage_error)?;
                }
                None => {
                    transaction
                        .execute(
                            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
                            params![chapter.chapter_id, request.project_id.as_str(), chapter.number, chapter.title],
                        )
                        .map_err(storage_error)?;
                }
            }
        }
        if exists(
            &transaction,
            "SELECT 1 FROM story_revisions_v2 WHERE revision_id = ?1",
            [request.revision_id.as_str()],
        )? {
            return Err(RepositoryError::AlreadyExists(
                "revision",
                request.revision_id.to_string(),
            ));
        }
        let existing_head = transaction
            .query_row(
                "SELECT object_kind, candidate_revision_id, accepted_revision_id, locked FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![request.project_id.as_str(), request.object_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, bool>(3)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let (candidate, accepted) = if let Some((kind, candidate, accepted, locked)) = existing_head
        {
            if kind != request.object_kind {
                return Err(RepositoryError::InvalidObjectKind);
            }
            if locked {
                return Err(RepositoryError::ObjectLocked);
            }
            (candidate, accepted)
        } else {
            (None, None)
        };
        let current_candidate = candidate
            .map(RevisionId::parse)
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?;
        if current_candidate != request.expected_candidate_revision_id {
            return Err(RepositoryError::StaleCandidate {
                expected: request.expected_candidate_revision_id,
                actual: current_candidate,
            });
        }
        // Extraction forbids chapter projection, so this phase has no domain writes.
        // Only an explicit validation rejection can be settled without a receipt.
        if request.object_kind == "commitmentExtraction"
            && let Err(error) = crate::commitment_extraction::validate_write(
                &transaction,
                &request.project_id,
                &request.object_id,
                &request.content_json,
            )
        {
            if matches!(&error, RepositoryError::InvalidContent(_)) {
                let binding = request
                    .task_output
                    .as_ref()
                    .ok_or(RepositoryError::TaskOutputForbidden)?;
                crate::recovery_operations::reject_operation(
                    &transaction,
                    self.recovery_operation.as_ref(),
                    &request.project_id,
                    &binding.task_id,
                    "submit_commitment_extraction",
                )?;
                transaction.commit().map_err(storage_error)?;
            }
            return Err(error);
        }
        let parent_revision_id =
            current_candidate.or_else(|| accepted.and_then(|value| RevisionId::parse(value).ok()));
        let working_source_revision_id = parent_revision_id.clone();
        let revision = RevisionRecord {
            project_id: request.project_id.clone(),
            object_id: request.object_id.clone(),
            object_kind: request.object_kind.clone(),
            revision_id: request.revision_id.clone(),
            parent_revision_id,
            content_digest: request.content_digest,
            content_json: request.content_json,
        };
        insert_revision(&transaction, &revision)?;
        crate::impacts::link_candidate_impacts(
            &transaction,
            &request.project_id,
            &request.object_kind,
            &request.revision_id,
            &revision.content_json,
            &request.linked_impact_ids,
        )?;
        let mut members = head.members;
        members.insert(request.object_id.clone(), request.revision_id.clone());
        insert_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.next_snapshot_id,
            Some(&request.expected_snapshot_id),
            &members,
        )?;
        let changed = transaction
            .execute(
                "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
                params![request.next_snapshot_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.expected_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        if changed != 1 {
            return Err(RepositoryError::CorruptState);
        }
        transaction
            .execute(
                "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(project_id, object_id) DO UPDATE SET candidate_revision_id = excluded.candidate_revision_id",
                params![request.project_id.as_str(), request.object_id.as_str(), request.object_kind, request.revision_id.as_str()],
            )
            .map_err(storage_error)?;
        if request.object_kind == "draft" {
            crate::modeling::invalidate_source_modeling(
                &transaction,
                &request.project_id,
                &request.object_id,
            )?;
        }
        if let Some(binding) = &request.task_output {
            let working_base_revision_id = crate::workspace_selection::task_output_working_base(
                &transaction,
                &binding.task_id,
                request.object_id.as_str(),
                working_source_revision_id.as_ref(),
            )?;
            crate::tasks::record_task_output(
                &transaction,
                binding,
                &request.revision_id,
                &request.object_id,
                working_base_revision_id.as_ref(),
            )?;
            crate::workspace_selection::advance_task_working_source(
                &transaction,
                &request.project_id,
                &binding.task_id,
                &request.object_kind,
                request.object_id.as_str(),
                &request.revision_id,
            )?;
        } else {
            crate::workspace_selection::advance_author_working_source(
                &transaction,
                &request.project_id,
                &request.object_kind,
                request.object_id.as_str(),
                working_source_revision_id.as_ref(),
                &request.revision_id,
            )?;
        }
        transaction
            .execute(
                "INSERT INTO story_commits_v2(commit_id, project_id, branch_id, base_snapshot_id, result_snapshot_id, status, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, 'applied', NULL)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.expected_snapshot_id.as_str(), request.next_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        let commit_receipt = CommitReceipt {
            commit_id: request.commit_id,
            result_snapshot_id: request.next_snapshot_id.clone(),
            status: CommitStatus::Applied,
        };
        insert_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
            &commit_receipt,
        )?;
        transaction
            .execute(
                "INSERT INTO story_candidate_receipts_v2(project_id, branch_id, idempotency_key, request_digest, revision_id, result_snapshot_id, project_revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![request.project_id.as_str(), request.branch_id.as_str(), request.idempotency_key.as_str(), request.request_digest, request.revision_id.as_str(), request.next_snapshot_id.as_str(), project_revision],
            )
            .map_err(storage_error)?;
        crate::recovery_operations::commit_candidate(
            &transaction,
            self.recovery_operation.as_ref(),
            &request.project_id,
            request.task_output.as_ref(),
            &request.object_kind,
            request.revision_id.as_str(),
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(CandidateReceipt {
            revision_id: request.revision_id,
            result_snapshot_id: request.next_snapshot_id,
            project_revision,
        })
    }

    fn chapters(&self, project_id: &ProjectId) -> Result<Vec<ChapterRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT chapter_id, project_id, number, title, status, summary FROM story_chapters_v2 WHERE project_id = ?1 ORDER BY number")
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(storage_error)?
            .map(|row| {
                let row = row.map_err(storage_error)?;
                Ok(ChapterRecord {
                    chapter_id: row.0,
                    project_id: ProjectId::parse(row.1)
                        .map_err(|_| RepositoryError::CorruptState)?,
                    number: row.2,
                    title: row.3,
                    status: row.4,
                    summary: row.5,
                })
            })
            .collect()
    }

    fn project(&self, project_id: &ProjectId) -> Result<Option<ProjectRecord>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT project_id, title, logline, created_at, updated_at FROM story_projects_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                project_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    fn list_projects(&self) -> Result<Vec<ProjectRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT project_id, title, logline, created_at, updated_at FROM story_projects_v2 ORDER BY updated_at DESC, created_at DESC")
            .map_err(storage_error)?;
        statement
            .query_map([], project_from_row)
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect()
    }

    fn create_branch(
        &mut self,
        project_id: ProjectId,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if exists(
            &transaction,
            "SELECT 1 FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
            params![project_id.as_str(), branch_id.as_str()],
        )? {
            return Err(RepositoryError::AlreadyExists(
                "branch",
                branch_id.to_string(),
            ));
        }
        require_unused_snapshot(&transaction, &snapshot_id)?;
        transaction
            .execute(
                "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, ?3, NULL)",
                params![snapshot_id.as_str(), project_id.as_str(), branch_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_branches_v2(project_id, branch_id, head_snapshot_id) VALUES (?1, ?2, ?3)",
                params![project_id.as_str(), branch_id.as_str(), snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)
    }

    fn branch_head(
        &self,
        project_id: &ProjectId,
        branch_id: &BranchId,
    ) -> Result<Option<Snapshot>, RepositoryError> {
        let snapshot_id = self
            .connection
            .query_row(
                "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id = ?1 AND branch_id = ?2",
                params![project_id.as_str(), branch_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?;
        snapshot_id
            .map(|snapshot_id| load_snapshot(&self.connection, &snapshot_id))
            .transpose()
    }

    fn revision_content(
        &self,
        revision_id: &RevisionId,
    ) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT content_json FROM story_revisions_v2 WHERE revision_id = ?1",
                [revision_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)
    }

    fn apply_commit(&mut self, request: CommitRequest) -> Result<CommitReceipt, RepositoryError> {
        if request.revisions.is_empty() {
            return Err(RepositoryError::EmptyCommit);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(receipt) = replay_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        let head = require_head(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.base_snapshot_id,
        )?;
        require_unused_commit(&transaction, &request.commit_id)?;
        require_unused_snapshot(&transaction, &request.next_snapshot_id)?;

        let mut members = head.members;
        let mut staged = BTreeMap::new();
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
            if exists(
                &transaction,
                "SELECT 1 FROM story_revisions_v2 WHERE revision_id = ?1",
                [revision.revision_id.as_str()],
            )? {
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

        for revision in &request.revisions {
            insert_revision(&transaction, revision)?;
        }
        insert_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.next_snapshot_id,
            Some(&request.base_snapshot_id),
            &members,
        )?;
        transaction
            .execute(
                "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
                params![request.next_snapshot_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_commits_v2(commit_id, project_id, branch_id, base_snapshot_id, result_snapshot_id, status, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, 'applied', NULL)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str(), request.next_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        let receipt = CommitReceipt {
            commit_id: request.commit_id,
            result_snapshot_id: request.next_snapshot_id,
            status: CommitStatus::Applied,
        };
        insert_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
            &receipt,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(receipt)
    }

    fn undo_latest(&mut self, request: UndoRequest) -> Result<CommitReceipt, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(receipt) = replay_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
            return Ok(receipt);
        }
        require_head(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.base_snapshot_id,
        )?;
        require_unused_commit(&transaction, &request.commit_id)?;
        require_unused_snapshot(&transaction, &request.next_snapshot_id)?;
        let target = transaction
            .query_row(
                "SELECT project_id, branch_id, base_snapshot_id, result_snapshot_id FROM story_commits_v2 WHERE commit_id = ?1",
                [request.target_commit_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::CommitNotFound)?;
        if target.0 != request.project_id.as_str() || target.1 != request.branch_id.as_str() {
            return Err(RepositoryError::CrossProjectReference);
        }
        if target.3 != request.base_snapshot_id.as_str() {
            return Err(RepositoryError::UndoRequiresLatestCommit);
        }
        let restored = load_members(&transaction, &target.2)?;
        insert_snapshot(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.next_snapshot_id,
            Some(&request.base_snapshot_id),
            &restored,
        )?;
        transaction
            .execute(
                "UPDATE story_branches_v2 SET head_snapshot_id = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND head_snapshot_id = ?4",
                params![request.next_snapshot_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str()],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "INSERT INTO story_commits_v2(commit_id, project_id, branch_id, base_snapshot_id, result_snapshot_id, status, target_commit_id) VALUES (?1, ?2, ?3, ?4, ?5, 'undone', ?6)",
                params![request.commit_id.as_str(), request.project_id.as_str(), request.branch_id.as_str(), request.base_snapshot_id.as_str(), request.next_snapshot_id.as_str(), request.target_commit_id.as_str()],
            )
            .map_err(storage_error)?;
        let receipt = CommitReceipt {
            commit_id: request.commit_id,
            result_snapshot_id: request.next_snapshot_id,
            status: CommitStatus::Undone,
        };
        insert_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
            &receipt,
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}
