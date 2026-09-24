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

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_branches_v2 (
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  head_snapshot_id TEXT NOT NULL,
  PRIMARY KEY(project_id, branch_id)
);
CREATE TABLE IF NOT EXISTS story_projects_v2 (
  project_id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  logline TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS story_workspaces_v2 (
  project_id TEXT PRIMARY KEY REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  revision INTEGER NOT NULL DEFAULT 0,
  plan_window INTEGER NOT NULL DEFAULT 3 CHECK(plan_window BETWEEN 1 AND 5)
);
CREATE TABLE IF NOT EXISTS story_object_heads_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL,
  candidate_revision_id TEXT,
  accepted_revision_id TEXT,
  locked INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(project_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_chapters_v2 (
  chapter_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  UNIQUE(project_id, number)
);
CREATE TABLE IF NOT EXISTS story_revisions_v2 (
  revision_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  parent_revision_id TEXT,
  content_digest TEXT NOT NULL,
  content_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_snapshots_v2 (
  snapshot_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  parent_snapshot_id TEXT
);
CREATE TABLE IF NOT EXISTS story_snapshot_members_v2 (
  snapshot_id TEXT NOT NULL REFERENCES story_snapshots_v2(snapshot_id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  revision_id TEXT NOT NULL REFERENCES story_revisions_v2(revision_id),
  PRIMARY KEY(snapshot_id, object_id)
);
CREATE TABLE IF NOT EXISTS story_commits_v2 (
  commit_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  base_snapshot_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('applied', 'undone')),
  target_commit_id TEXT
);
CREATE TABLE IF NOT EXISTS story_receipts_v2 (
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  commit_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('applied', 'undone')),
  PRIMARY KEY(project_id, branch_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_candidate_receipts_v2 (
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  PRIMARY KEY(project_id, branch_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_reviews_v2 (
  review_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  checks_json TEXT NOT NULL,
  proposals_json TEXT NOT NULL,
  selections_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('ready', 'accepted', 'stale', 'rejected'))
);
CREATE TABLE IF NOT EXISTS story_facts_v2 (
  fact_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  payload_json TEXT NOT NULL,
  accepted_by_commit_id TEXT NOT NULL,
  project_revision INTEGER NOT NULL,
  reverted_by_commit_id TEXT
);
CREATE TABLE IF NOT EXISTS story_review_receipts_v2 (
  project_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  review_id TEXT NOT NULL,
  PRIMARY KEY(project_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_acceptances_v2 (
  commit_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  review_id TEXT,
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL,
  accepted_revision_id TEXT NOT NULL,
  previous_accepted_revision_id TEXT,
  base_revision INTEGER NOT NULL,
  result_revision INTEGER NOT NULL,
  base_snapshot_id TEXT NOT NULL,
  result_snapshot_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('accept', 'undo')),
  target_commit_id TEXT,
  undone_by_commit_id TEXT
);
CREATE TABLE IF NOT EXISTS story_acceptance_receipts_v2 (
  project_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  commit_id TEXT NOT NULL,
  result_revision INTEGER NOT NULL,
  PRIMARY KEY(project_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_tasks_v2 (
  task_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  thread_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  task_kind TEXT NOT NULL,
  purpose TEXT NOT NULL,
  required_output_kind TEXT,
  target_json TEXT NOT NULL,
  input_manifest_json TEXT NOT NULL,
  input_revision INTEGER NOT NULL,
  model TEXT,
  status TEXT NOT NULL CHECK(status IN ('queued','running','waiting','completed','failed','cancelled')),
  stage TEXT NOT NULL,
  delivery_status TEXT NOT NULL CHECK(delivery_status IN ('pending','ready_for_review','incomplete')),
  output_refs_json TEXT NOT NULL DEFAULT '[]',
  result_summary TEXT NOT NULL DEFAULT '',
  error TEXT,
  capability_hash TEXT NOT NULL,
  capability_expires_at INTEGER NOT NULL,
  capability_revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS story_task_outputs_v2 (
  task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  output_kind TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  completeness TEXT,
  PRIMARY KEY(task_id, ordinal)
);
CREATE TABLE IF NOT EXISTS story_working_baselines_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  object_kind TEXT NOT NULL,
  object_id TEXT NOT NULL,
  revision_id TEXT,
  selection_revision INTEGER NOT NULL,
  selected_at INTEGER NOT NULL,
  PRIMARY KEY(project_id, object_kind, object_id)
);
CREATE TABLE IF NOT EXISTS story_continuation_intents_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  working_revision_id TEXT NOT NULL,
  author_intent TEXT NOT NULL,
  status TEXT NOT NULL,
  task_id TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(project_id, target_kind, target_id)
);
CREATE TABLE IF NOT EXISTS story_chapter_plan_bindings_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  plan_revision_id TEXT NOT NULL,
  chapter_plan_id TEXT NOT NULL,
  chapter_id TEXT NOT NULL REFERENCES story_chapters_v2(chapter_id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(project_id, chapter_plan_id),
  UNIQUE(project_id, chapter_id)
);
CREATE TABLE IF NOT EXISTS story_domain_receipts_v2 (
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  operation TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  result_json TEXT NOT NULL,
  PRIMARY KEY(project_id, operation, idempotency_key)
);
CREATE TABLE IF NOT EXISTS story_planning_impacts_v2 (
  impact_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  source_fact_id TEXT NOT NULL,
  source_commit_id TEXT NOT NULL,
  target_artifact_kind TEXT NOT NULL CHECK(target_artifact_kind IN ('outline','plan')),
  target_revision_id TEXT NOT NULL,
  target_node_id TEXT NOT NULL,
  target_label TEXT NOT NULL,
  dependency_key TEXT NOT NULL,
  predicate TEXT NOT NULL,
  evidence TEXT NOT NULL,
  severity TEXT NOT NULL CHECK(severity IN ('affected','locked-conflict')),
  state TEXT NOT NULL CHECK(state IN ('open','resolved-kept','resolved-false-positive','resolved-updated','superseded')),
  resolution_note TEXT,
  created_revision INTEGER NOT NULL,
  resolved_revision INTEGER,
  reverted_by_commit_id TEXT,
  UNIQUE(project_id, source_fact_id, target_revision_id, target_node_id, dependency_key)
);
CREATE INDEX IF NOT EXISTS story_planning_impacts_project_v2
  ON story_planning_impacts_v2(project_id, state, severity, created_revision DESC);
CREATE TABLE IF NOT EXISTS story_planning_impact_links_v2 (
  impact_id TEXT PRIMARY KEY REFERENCES story_planning_impacts_v2(impact_id) ON DELETE CASCADE,
  candidate_revision_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_planning_review_runs_v2 (
  run_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
  artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('outline','plan')),
  revision_id TEXT NOT NULL,
  target_node_id TEXT,
  author_intent TEXT NOT NULL,
  input_revision INTEGER NOT NULL,
  roles_json TEXT NOT NULL,
  source_packet_json TEXT NOT NULL,
  source_packet_hash TEXT NOT NULL,
  checks_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('open','complete','partial','cancelled')),
  synthesis_json TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS story_planning_review_seats_v2 (
  run_id TEXT NOT NULL REFERENCES story_planning_review_runs_v2(run_id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  task_id TEXT UNIQUE REFERENCES story_tasks_v2(task_id),
  PRIMARY KEY(run_id, role)
);
CREATE TABLE IF NOT EXISTS story_planning_review_reports_v2 (
  report_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES story_planning_review_runs_v2(run_id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  task_id TEXT NOT NULL UNIQUE REFERENCES story_tasks_v2(task_id),
  report_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('complete','partial')),
  created_at INTEGER NOT NULL,
  UNIQUE(run_id, role)
);
CREATE TABLE IF NOT EXISTS story_local_metrics_v2 (
  metric TEXT PRIMARY KEY,
  count INTEGER NOT NULL DEFAULT 0,
  total_seconds INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS story_local_metric_receipts_v2 (
  idempotency_key TEXT PRIMARY KEY,
  metric TEXT NOT NULL
);
"#;

/// Durable Story Core repository. Every state transition is committed in one
/// SQLite transaction; callers never need to mirror an in-memory authority.
pub struct SqliteStoryRepository {
    pub(crate) connection: Connection,
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
        Self::from_connection(connection)
    }

    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, RepositoryError> {
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
            .map_err(storage_error)?;
        connection.execute_batch(SCHEMA).map_err(storage_error)?;
        Ok(Self { connection })
    }

    pub fn create_project(
        &mut self,
        project: ProjectRecord,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<ProjectRecord, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
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
        transaction.commit().map_err(storage_error)?;
        Ok(project)
    }

    pub fn project_revision(&self, project_id: &ProjectId) -> Result<i64, RepositoryError> {
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

    pub fn object_heads(&self, project_id: &ProjectId) -> Result<Vec<ObjectHead>, RepositoryError> {
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

    pub fn set_object_lock(
        &mut self,
        project_id: &ProjectId,
        object_id: &ObjectId,
        locked: bool,
        expected_project_revision: i64,
    ) -> Result<i64, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
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

    pub fn save_candidate(
        &mut self,
        request: SaveCandidateRequest,
    ) -> Result<CandidateReceipt, RepositoryError> {
        serde_json::from_str::<serde_json::Value>(&request.content_json)
            .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
        if request.object_kind.trim().is_empty() {
            return Err(RepositoryError::InvalidObjectKind);
        }
        let transaction = self.connection.transaction().map_err(storage_error)?;
        if let Some(receipt) = replay_candidate_receipt(
            &transaction,
            &request.project_id,
            &request.branch_id,
            &request.idempotency_key,
            &request.request_digest,
        )? {
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
        if let Some(binding) = &request.task_output {
            crate::tasks::validate_task_write(
                &transaction,
                &request.project_id,
                binding,
                project_revision,
                &request.object_id,
            )?;
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
            if chapter.number < 1 || chapter.title.chars().count() > 160 {
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
                Some(_) => {}
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
        let parent_revision_id =
            current_candidate.or_else(|| accepted.and_then(|value| RevisionId::parse(value).ok()));
        let revision = RevisionRecord {
            project_id: request.project_id.clone(),
            object_id: request.object_id.clone(),
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
        if let Some(binding) = &request.task_output {
            crate::tasks::record_task_output(
                &transaction,
                binding,
                &request.revision_id,
                &request.object_id,
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
        transaction.commit().map_err(storage_error)?;
        Ok(CandidateReceipt {
            revision_id: request.revision_id,
            result_snapshot_id: request.next_snapshot_id,
            project_revision,
        })
    }

    pub fn chapters(&self, project_id: &ProjectId) -> Result<Vec<ChapterRecord>, RepositoryError> {
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

    pub fn project(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<ProjectRecord>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT project_id, title, logline, created_at, updated_at FROM story_projects_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                project_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, RepositoryError> {
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

    pub fn create_branch(
        &mut self,
        project_id: ProjectId,
        branch_id: BranchId,
        snapshot_id: SnapshotId,
    ) -> Result<(), RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
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

    pub fn branch_head(
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

    pub fn revision_content(
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

    pub fn apply_commit(
        &mut self,
        request: CommitRequest,
    ) -> Result<CommitReceipt, RepositoryError> {
        if request.revisions.is_empty() {
            return Err(RepositoryError::EmptyCommit);
        }
        let transaction = self.connection.transaction().map_err(storage_error)?;
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

    pub fn undo_latest(&mut self, request: UndoRequest) -> Result<CommitReceipt, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
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
    serde_json::from_str::<serde_json::Value>(&revision.content_json)
        .map_err(|error| RepositoryError::Storage(format!("invalid revision JSON: {error}")))?;
    connection
        .execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, parent_revision_id, content_digest, content_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![revision.revision_id.as_str(), revision.project_id.as_str(), revision.object_id.as_str(), revision.parent_revision_id.as_ref().map(RevisionId::as_str), revision.content_digest, revision.content_json],
        )
        .map_err(storage_error)?;
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
