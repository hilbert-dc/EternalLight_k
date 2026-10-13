//! Durable, source-bound requests for deterministic condition evaluation.
//! Execution and result publication are separate from accepting a request.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[path = "condition_evaluation_binding.rs"]
mod binding;
#[path = "condition_evaluation_execution.rs"]
mod execution;
#[path = "condition_explanations.rs"]
mod explanations;
#[path = "condition_evaluation_worker.rs"]
mod worker;
pub use explanations::ConditionExplanationPage;
#[path = "condition_evaluation_query.rs"]
mod query;
pub(crate) use query::resolve as resolve_assessment;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS story_condition_evaluations_v1 (
 id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES story_projects_v2(project_id) ON DELETE CASCADE,
 target_json TEXT NOT NULL,
 binding_digest TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('queued','running','completed','paused','cancelled','stale','failed')),
 dispatch_sequence INTEGER NOT NULL DEFAULT 0 CHECK(dispatch_sequence>=0),
 version INTEGER NOT NULL DEFAULT 0 CHECK(version>=0),
 progress_json TEXT NOT NULL,
 progress_digest TEXT NOT NULL,
 last_batch_id TEXT,
 scanned_sources INTEGER NOT NULL DEFAULT 0 CHECK(scanned_sources>=0),
 source_bytes INTEGER NOT NULL DEFAULT 0 CHECK(source_bytes>=0),
 result_json TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS story_condition_evaluation_active_v1
 ON story_condition_evaluations_v1(project_id,binding_digest) WHERE status IN ('queued','running','paused');
CREATE INDEX IF NOT EXISTS story_condition_evaluation_dispatch_v1
 ON story_condition_evaluations_v1(dispatch_sequence);
CREATE INDEX IF NOT EXISTS story_condition_evaluation_target_v1
 ON story_condition_evaluations_v1(project_id,target_json,binding_digest);
CREATE INDEX IF NOT EXISTS story_condition_evaluation_runnable_v1
 ON story_condition_evaluations_v1(dispatch_sequence) WHERE status IN ('queued','running');
";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConditionEvaluationTarget {
    pub snapshot_id: Option<String>,
    pub source_batch_id: String,
    pub assertion_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ConditionEvaluationStatus {
    Queued,
    Running,
    Completed,
    Paused,
    Cancelled,
    Stale,
    Failed,
}

pub enum ConditionEvaluationControl {
    Pause,
    Resume,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ConditionEvaluationJob {
    pub id: String,
    pub project_id: String,
    pub target: ConditionEvaluationTarget,
    pub binding_digest: String,
    pub status: ConditionEvaluationStatus,
    pub version: i64,
    pub scanned_sources: i64,
    pub source_bytes: i64,
    pub result: Option<serde_json::Value>,
}



struct StoredJob {
    target_json: String,
    binding_digest: String,
    status: String,
    version: i64,
    scanned_sources: i64,
    source_bytes: i64,
    result_json: Option<String>,
}

fn load(
    db: &Connection,
    project: &ProjectId,
    id: &str,
) -> Result<ConditionEvaluationJob, RepositoryError> {
    let row = db.query_row("SELECT target_json,binding_digest,status,version,scanned_sources,source_bytes,result_json FROM story_condition_evaluations_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|row|Ok(StoredJob { target_json:row.get(0)?,binding_digest:row.get(1)?,status:row.get(2)?,version:row.get(3)?,scanned_sources:row.get(4)?,source_bytes:row.get(5)?,result_json:row.get(6)? })).optional().map_err(storage)?;
    let StoredJob {
        target_json: target,
        binding_digest: digest,
        status,
        version,
        scanned_sources,
        source_bytes,
        result_json: result,
    } = row.ok_or(RepositoryError::TaskNotFound)?;
    let target: ConditionEvaluationTarget =
        serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?;
    let mut status: ConditionEvaluationStatus = serde_json::from_value(serde_json::json!(status))
        .map_err(|_| RepositoryError::CorruptState)?;
    if matches!(
        status,
        ConditionEvaluationStatus::Queued
            | ConditionEvaluationStatus::Running
            | ConditionEvaluationStatus::Completed
            | ConditionEvaluationStatus::Paused
    ) && binding::capture(db, project, &target)?.as_ref() != Some(&digest)
    {
        status = ConditionEvaluationStatus::Stale;
    }
    if status == ConditionEvaluationStatus::Completed {
        explanations::checkpoint(db, project, id)?;
    }
    Ok(ConditionEvaluationJob {
        id: id.into(),
        project_id: project.to_string(),
        target,
        binding_digest: digest,
        status,
        version,
        scanned_sources,
        source_bytes,
        result: if status == ConditionEvaluationStatus::Completed {
            Some(
                serde_json::from_str(
                    result
                        .as_deref()
                        .filter(|value| value.len() <= 6144)
                        .ok_or(RepositoryError::CorruptState)?,
                )
                .map_err(|_| RepositoryError::CorruptState)?,
            )
        } else {
            None
        },
    })
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public condition evaluation jobs capability; SQLite retains transaction ownership.
pub trait ConditionEvaluationJobsRepository {
    /// Accept a bound request, not a claim that evaluation has run or completed.
    fn start_condition_evaluation(
        &mut self,
        project: &ProjectId,
        target: ConditionEvaluationTarget,
    ) -> Result<ConditionEvaluationJob, RepositoryError>;
    fn condition_evaluation(
        &self,
        project: &ProjectId,
        id: &str,
    ) -> Result<ConditionEvaluationJob, RepositoryError>;
    fn control_condition_evaluation(
        &mut self,
        project: &ProjectId,
        id: &str,
        expected_version: i64,
        command: ConditionEvaluationControl,
    ) -> Result<ConditionEvaluationJob, RepositoryError>;
}

impl ConditionEvaluationJobsRepository for SqliteStoryRepository {
    /// Accept a bound request, not a claim that evaluation has run or completed.
    fn start_condition_evaluation(
        &mut self,
        project: &ProjectId,
        target: ConditionEvaluationTarget,
    ) -> Result<ConditionEvaluationJob, RepositoryError> {
        self.project_revision(project)?;
        if target.source_batch_id.is_empty()
            || target.source_batch_id.len() > 160
            || target.assertion_index >= 100
        {
            return Err(RepositoryError::InvalidContent(
                "invalid condition event target".into(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let digest =
            binding::capture(&tx, project, &target)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        // Retire invalidated reservations before enforcing the shared capacity.
        // There are at most sixteen active records, so reconciliation is bounded.
        let active = {
            let mut statement=tx.prepare("SELECT id,project_id FROM story_condition_evaluations_v1 WHERE status IN ('queued','running','paused') LIMIT 17").map_err(storage)?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?
        };
        if active.len() > 16 {
            return Err(RepositoryError::CorruptState);
        }
        for (id, owner) in active {
            let owner = ProjectId::parse(owner).map_err(|_| RepositoryError::CorruptState)?;
            let job = load(&tx, &owner, &id)?;
            if job.status == ConditionEvaluationStatus::Stale {
                let version = job
                    .version
                    .checked_add(1)
                    .ok_or(RepositoryError::CorruptState)?;
                tx.execute("UPDATE story_condition_evaluations_v1 SET status='stale',version=?1 WHERE id=?2",params![version,id]).map_err(storage)?;
            }
        }
        let existing:Option<String> = tx.query_row("SELECT id FROM story_condition_evaluations_v1 WHERE project_id=?1 AND binding_digest=?2 AND status IN ('queued','running','paused','completed') ORDER BY rowid DESC LIMIT 1",params![project.as_str(),digest],|row|row.get(0)).optional().map_err(storage)?;
        if let Some(id) = existing {
            let job = load(&tx, project, &id)?;
            tx.commit().map_err(storage)?;
            return Ok(job);
        }
        let active:i64 = tx.query_row("SELECT COUNT(*) FROM story_condition_evaluations_v1 WHERE status IN ('queued','running','paused')",[],|row|row.get(0)).map_err(storage)?;
        if active >= 16 {
            return Err(RepositoryError::InvalidContent(
                "condition evaluation queue is full".into(),
            ));
        }
        let id = format!("condition-{}", uuid::Uuid::new_v4().simple());
        let progress =
            serde_json::to_string(&crate::event_rule_evaluation::ConditionProgress::default())
                .map_err(|_| RepositoryError::CorruptState)?;
        use sha2::{Digest, Sha256};
        let progress_digest = format!("{:x}", Sha256::digest(progress.as_bytes()));
        tx.execute("INSERT INTO story_condition_evaluations_v1(id,project_id,target_json,binding_digest,status,progress_json,progress_digest,dispatch_sequence) VALUES(?1,?2,?3,?4,'queued',?5,?6,(SELECT COALESCE(MAX(dispatch_sequence),0) FROM story_condition_evaluations_v1))",params![id,project.as_str(),serde_json::to_string(&target).map_err(|_|RepositoryError::CorruptState)?,digest,progress,progress_digest]).map_err(storage)?;
        let job = load(&tx, project, &id)?;
        tx.commit().map_err(storage)?;
        Ok(job)
    }

    fn condition_evaluation(
        &self,
        project: &ProjectId,
        id: &str,
    ) -> Result<ConditionEvaluationJob, RepositoryError> {
        let read = self.connection.unchecked_transaction().map_err(storage)?;
        let job = load(&read, project, id)?;
        read.commit().map_err(storage)?;
        Ok(job)
    }

    fn control_condition_evaluation(
        &mut self,
        project: &ProjectId,
        id: &str,
        expected_version: i64,
        command: ConditionEvaluationControl,
    ) -> Result<ConditionEvaluationJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let mut job = load(&tx, project, id)?;
        if job.version != expected_version {
            return Err(RepositoryError::InvalidContent(
                "condition evaluation version changed".into(),
            ));
        }
        use ConditionEvaluationControl as Control;
        use ConditionEvaluationStatus as Status;
        job.status = match (command, job.status) {
            (Control::Pause, Status::Queued | Status::Running) => Status::Paused,
            (Control::Resume, Status::Paused) => Status::Queued,
            (Control::Resume, Status::Stale) => Status::Stale,
            (
                Control::Cancel,
                Status::Queued | Status::Running | Status::Paused | Status::Stale | Status::Failed,
            ) => Status::Cancelled,
            (
                Control::Pause,
                Status::Paused
                | Status::Completed
                | Status::Cancelled
                | Status::Stale
                | Status::Failed,
            )
            | (
                Control::Resume,
                Status::Queued
                | Status::Running
                | Status::Completed
                | Status::Cancelled
                | Status::Failed,
            )
            | (Control::Cancel, Status::Completed | Status::Cancelled) => {
                return Err(RepositoryError::InvalidContent(
                    "invalid condition evaluation control".into(),
                ));
            }
        };
        job.version = job
            .version
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        let status = serde_json::to_value(job.status).map_err(|_| RepositoryError::CorruptState)?;
        tx.execute("UPDATE story_condition_evaluations_v1 SET status=?1,version=?2 WHERE id=?3 AND project_id=?4",params![status.as_str().ok_or(RepositoryError::CorruptState)?,job.version,id,project.as_str()]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(job)
    }
}

pub use explanations::ConditionExplanationsRepository;

pub use worker::ConditionEvaluationWorkerRepository;

pub use execution::ConditionEvaluationExecutionRepository;

use crate::ProjectRevisionRepository;
