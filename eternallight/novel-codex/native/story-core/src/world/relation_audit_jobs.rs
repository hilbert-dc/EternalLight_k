use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use crate::relation_index_integrity::{
    audit_relation_batch_on, audit_relation_orphans_on, relation_generation_on,
};
use crate::{ProjectId, RelationIndexAudit, RepositoryError, SqliteStoryRepository};

const CHUNK_BATCHES: i64 = 25;
const CHUNK_BYTES: usize = 250_000;
const MAX_ACTIVE_JOBS: i64 = 16;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS, serde::Deserialize)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RelationAuditJob {
    pub id: String,
    pub project_id: String,
    pub status: String,
    pub version: i64,
    pub source_count: i64,
    pub source_max_rowid: i64,
    pub index_generation: i64,
    pub next_rowid: i64,
    pub audit: RelationIndexAudit,
    pub verified_consistent: Option<bool>,
    pub error: Option<String>,
}



fn source_signature(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(i64, i64), RepositoryError> {
    connection.query_row(
        "SELECT COUNT(*), COALESCE(MAX(rowid), 0) FROM story_modeling_batches_v2 WHERE project_id = ?1 AND published_at IS NOT NULL",
        [project_id.as_str()], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(storage_error)
}

fn source_matches(
    connection: &Connection,
    project_id: &ProjectId,
    job: &RelationAuditJob,
) -> Result<bool, RepositoryError> {
    Ok(
        source_signature(connection, project_id)? == (job.source_count, job.source_max_rowid)
            && relation_generation_on(connection, project_id)? == job.index_generation,
    )
}

fn load_job(
    connection: &Connection,
    project_id: &ProjectId,
    job_id: &str,
) -> Result<RelationAuditJob, RepositoryError> {
    connection.query_row(
        "SELECT status, version, source_count, source_max_rowid, index_generation, next_rowid, audit_json, error FROM story_relation_audit_jobs_v1 WHERE project_id = ?1 AND job_id = ?2",
        params![project_id.as_str(), job_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?)),
    ).optional().map_err(storage_error)?
        .map(|(status, version, source_count, source_max_rowid, index_generation, next_rowid, audit_json, error)| {
            let audit: RelationIndexAudit = serde_json::from_str(&audit_json).map_err(|_| RepositoryError::CorruptState)?;
            let verified_consistent = (status == "completed").then(|| audit.complete());
            Ok(RelationAuditJob { id: job_id.into(), project_id: project_id.to_string(), status,
                version, source_count, source_max_rowid, index_generation, next_rowid, audit,
                verified_consistent, error })
        }).transpose()?.ok_or(RepositoryError::TaskNotFound)
}

fn persist_job(connection: &Connection, job: &mut RelationAuditJob) -> Result<(), RepositoryError> {
    let changed = connection.execute(
        "UPDATE story_relation_audit_jobs_v1 SET status = ?1, version = version + 1, next_rowid = ?2, audit_json = ?3, error = ?4 WHERE project_id = ?5 AND job_id = ?6 AND version = ?7",
        params![job.status, job.next_rowid, serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?,
            job.error, job.project_id, job.id, job.version],
    ).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::InvalidContent(
            "relation audit version changed".into(),
        ));
    }
    job.version += 1;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "relation_audit_jobs_tests.rs"]
mod tests;

/// Public relation audit jobs capability; SQLite retains transaction ownership.
pub trait RelationAuditJobsRepository {
    fn start_relation_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<RelationAuditJob, RepositoryError>;
    fn relation_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<RelationAuditJob, RepositoryError>;
    fn latest_relation_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<RelationAuditJob>, RepositoryError>;
    fn next_relation_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError>;
    fn control_relation_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<RelationAuditJob, RepositoryError>;
    /// Audits at most 25 published batches and normally 250 KB of source payload per tick.
    fn advance_relation_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<RelationAuditJob, RepositoryError>;
}

impl RelationAuditJobsRepository for SqliteStoryRepository {
    fn start_relation_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<RelationAuditJob, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let existing = tx.query_row(
            "SELECT job_id FROM story_relation_audit_jobs_v1 WHERE project_id = ?1 AND status IN ('queued','running','paused')",
            [project_id.as_str()], |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?;
        if let Some(id) = existing {
            let mut job = load_job(&tx, project_id, &id)?;
            if source_matches(&tx, project_id, &job)? {
                tx.commit().map_err(storage_error)?;
                return Ok(job);
            }
            job.status = "stale".into();
            persist_job(&tx, &mut job)?;
        }
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM story_relation_audit_jobs_v1 WHERE status IN ('queued','running','paused')",
            [], |row| row.get(0),
        ).map_err(storage_error)?;
        if active >= MAX_ACTIVE_JOBS {
            return Err(RepositoryError::InvalidContent(
                "relation audit queue is full".into(),
            ));
        }
        let (source_count, source_max_rowid) = source_signature(&tx, project_id)?;
        let job = RelationAuditJob {
            id: format!("relation-audit-{}", uuid::Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            status: "queued".into(),
            version: 0,
            source_count,
            source_max_rowid,
            index_generation: relation_generation_on(&tx, project_id)?,
            next_rowid: 0,
            audit: RelationIndexAudit::empty(),
            verified_consistent: None,
            error: None,
        };
        tx.execute(
            "INSERT INTO story_relation_audit_jobs_v1(job_id, project_id, status, source_count, source_max_rowid, index_generation, audit_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![job.id, job.project_id, job.status, job.source_count, job.source_max_rowid,
                job.index_generation, serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    fn relation_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<RelationAuditJob, RepositoryError> {
        let mut job = load_job(&self.connection, project_id, job_id)?;
        if matches!(
            job.status.as_str(),
            "queued" | "running" | "paused" | "completed"
        ) && !source_matches(&self.connection, project_id, &job)?
        {
            job.status = "stale".into();
            job.verified_consistent = None;
        }
        Ok(job)
    }

    fn latest_relation_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<RelationAuditJob>, RepositoryError> {
        self.project_revision(project_id)?;
        let id = self.connection.query_row(
            "SELECT job_id FROM story_relation_audit_jobs_v1 WHERE project_id = ?1 ORDER BY CASE WHEN status IN ('queued','running','paused') THEN 0 ELSE 1 END, rowid DESC LIMIT 1",
            [project_id.as_str()], |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| self.relation_audit_job(project_id, &id))
            .transpose()
    }

    fn next_relation_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError> {
        self.connection.query_row(
            "SELECT project_id, job_id FROM story_relation_audit_jobs_v1 WHERE status IN ('queued','running') ORDER BY version, rowid LIMIT 1",
            [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?
            .map(|(project, id)| Ok((ProjectId::parse(project).map_err(|_| RepositoryError::CorruptState)?, id)))
            .transpose()
    }

    fn control_relation_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<RelationAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if job.version != expected_version {
            return Err(RepositoryError::InvalidContent(
                "relation audit version changed".into(),
            ));
        }
        job.status = match (command, job.status.as_str()) {
            ("pause", "queued" | "running") => "paused",
            ("resume", "paused") => {
                if source_matches(&tx, project_id, &job)? {
                    "queued"
                } else {
                    "stale"
                }
            }
            ("cancel", "queued" | "running" | "paused") => "cancelled",
            _ => {
                return Err(RepositoryError::InvalidContent(
                    "invalid relation audit control".into(),
                ));
            }
        }
        .into();
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    /// Audits at most 25 published batches and normally 250 KB of source payload per tick.
    fn advance_relation_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<RelationAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if !matches!(job.status.as_str(), "queued" | "running") {
            return Err(RepositoryError::InvalidContent(
                "relation audit is not runnable".into(),
            ));
        }
        if !source_matches(&tx, project_id, &job)? {
            job.status = "stale".into();
            persist_job(&tx, &mut job)?;
            tx.commit().map_err(storage_error)?;
            return Ok(job);
        }
        let mut statement = tx.prepare(
            "SELECT rowid, batch_id, assertions_json, entities_json FROM story_modeling_batches_v2 WHERE project_id = ?1 AND published_at IS NOT NULL AND rowid > ?2 AND rowid <= ?3 ORDER BY rowid LIMIT ?4",
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id.as_str(),
                    job.next_rowid,
                    job.source_max_rowid,
                    CHUNK_BATCHES
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(storage_error)?;
        let mut batches = Vec::new();
        let mut bytes = 0usize;
        for row in rows {
            let batch = row.map_err(storage_error)?;
            let next = bytes
                .saturating_add(batch.2.len())
                .saturating_add(batch.3.len());
            if !batches.is_empty() && next > CHUNK_BYTES {
                break;
            }
            bytes = next;
            batches.push(batch);
        }
        drop(statement);
        job.status = "running".into();
        for (rowid, batch_id, assertions_json, entities_json) in batches {
            match audit_relation_batch_on(
                &tx,
                project_id,
                &batch_id,
                &assertions_json,
                &entities_json,
                &mut job.audit,
            ) {
                Ok(()) => job.next_rowid = rowid,
                Err(RepositoryError::CorruptState | RepositoryError::InvalidContent(_)) => {
                    job.status = "failed".into();
                    job.error = Some(format!("published batch {batch_id} cannot be audited"));
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if job.status == "running" && job.audit.published_batches as i64 == job.source_count {
            audit_relation_orphans_on(&tx, project_id, &mut job.audit)?;
            job.status = "completed".into();
            job.verified_consistent = Some(job.audit.complete());
        }
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }
}

use crate::ProjectRevisionRepository;
