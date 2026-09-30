use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use crate::fact_integrity::{FACT_LEDGER_SELECT, FactLedgerRow};
use crate::{FactLedgerAudit, ProjectId, RepositoryError, SqliteStoryRepository};

const CHUNK_FACTS: i64 = 100;
const MAX_ACTIVE_JOBS: i64 = 16;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactAuditJob {
    pub id: String,
    pub project_id: String,
    pub status: String,
    pub version: i64,
    pub source_count: i64,
    pub source_max_rowid: i64,
    pub source_generation: i64,
    pub next_rowid: i64,
    pub audit: FactLedgerAudit,
    pub verified_consistent: Option<bool>,
    pub error: Option<String>,
}

impl SqliteStoryRepository {
    pub fn start_fact_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<FactAuditJob, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let existing: Option<String> = tx.query_row(
            "SELECT job_id FROM story_fact_audit_jobs_v1 WHERE project_id = ?1 AND status IN ('queued','running','paused')",
            [project_id.as_str()], |row| row.get(0),
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
            "SELECT COUNT(*) FROM story_fact_audit_jobs_v1 WHERE status IN ('queued','running','paused')",
            [], |row| row.get(0),
        ).map_err(storage_error)?;
        if active >= MAX_ACTIVE_JOBS {
            return Err(RepositoryError::InvalidContent(
                "fact audit queue is full".into(),
            ));
        }
        let (source_count, source_max_rowid) = source_signature(&tx, project_id)?;
        let job = FactAuditJob {
            id: format!("fact-audit-{}", uuid::Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            status: "queued".into(),
            version: 0,
            source_count,
            source_max_rowid,
            source_generation: generation_on(&tx, project_id)?,
            next_rowid: 0,
            audit: FactLedgerAudit::empty(),
            verified_consistent: None,
            error: None,
        };
        tx.execute(
            "INSERT INTO story_fact_audit_jobs_v1(job_id, project_id, status, source_count, source_max_rowid, source_generation, audit_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![job.id, job.project_id, job.status, job.source_count, job.source_max_rowid,
                job.source_generation, serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    pub fn fact_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<FactAuditJob, RepositoryError> {
        self.with_read_snapshot(|| {
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
        })?
    }

    pub fn latest_fact_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<FactAuditJob>, RepositoryError> {
        self.project_revision(project_id)?;
        let id: Option<String> = self.connection.query_row(
            "SELECT job_id FROM story_fact_audit_jobs_v1 WHERE project_id = ?1 ORDER BY CASE WHEN status IN ('queued','running','paused') THEN 0 ELSE 1 END, rowid DESC LIMIT 1",
            [project_id.as_str()], |row| row.get(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| self.fact_audit_job(project_id, &id))
            .transpose()
    }

    pub fn next_fact_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError> {
        self.connection.query_row(
            "SELECT project_id, job_id FROM story_fact_audit_jobs_v1 WHERE status IN ('queued','running') ORDER BY version, rowid LIMIT 1",
            [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?
            .map(|(project, id)| Ok((ProjectId::parse(project).map_err(|_| RepositoryError::CorruptState)?, id)))
            .transpose()
    }

    pub fn control_fact_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<FactAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if job.version != expected_version {
            return Err(RepositoryError::InvalidContent(
                "fact audit version changed".into(),
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
                    "invalid fact audit control".into(),
                ));
            }
        }
        .into();
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    /// Checks at most 100 fact rows in one durable transaction.
    pub fn advance_fact_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<FactAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if !matches!(job.status.as_str(), "queued" | "running") {
            return Err(RepositoryError::InvalidContent(
                "fact audit is not runnable".into(),
            ));
        }
        if !source_matches(&tx, project_id, &job)? {
            job.status = "stale".into();
            persist_job(&tx, &mut job)?;
            tx.commit().map_err(storage_error)?;
            return Ok(job);
        }
        let sql = format!(
            "{FACT_LEDGER_SELECT} WHERE f.project_id = ?1 AND f.rowid > ?2 AND f.rowid <= ?3 ORDER BY f.rowid LIMIT ?4"
        );
        let mut statement = tx.prepare(&sql).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id.as_str(),
                    job.next_rowid,
                    job.source_max_rowid,
                    CHUNK_FACTS
                ],
                FactLedgerRow::from_row,
            )
            .map_err(storage_error)?;
        let facts = rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
        drop(statement);
        let empty_chunk = facts.is_empty();
        job.status = "running".into();
        for fact in facts {
            job.next_rowid = fact.rowid;
            job.audit.record(&fact);
        }
        if job.audit.facts as i64 == job.source_count {
            if !source_matches(&tx, project_id, &job)?
                || source_signature(&tx, project_id)? != (job.source_count, job.source_max_rowid)
            {
                job.status = "stale".into();
            } else {
                job.status = "completed".into();
                job.verified_consistent = Some(job.audit.complete());
            }
        } else if empty_chunk {
            job.status = "stale".into();
        }
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }
}

fn source_signature(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(i64, i64), RepositoryError> {
    connection
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(rowid), 0) FROM story_facts_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(storage_error)
}

fn generation_on(connection: &Connection, project_id: &ProjectId) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT generation FROM story_fact_audit_state_v1 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)
        .map(|value| value.unwrap_or(0))
}

fn source_matches(
    connection: &Connection,
    project_id: &ProjectId,
    job: &FactAuditJob,
) -> Result<bool, RepositoryError> {
    Ok(generation_on(connection, project_id)? == job.source_generation)
}

fn load_job(
    connection: &Connection,
    project_id: &ProjectId,
    job_id: &str,
) -> Result<FactAuditJob, RepositoryError> {
    connection.query_row(
        "SELECT status, version, source_count, source_max_rowid, source_generation, next_rowid, audit_json FROM story_fact_audit_jobs_v1 WHERE project_id = ?1 AND job_id = ?2",
        params![project_id.as_str(), job_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, i64>(5)?, row.get::<_, String>(6)?)),
    ).optional().map_err(storage_error)?
        .map(|(status, version, source_count, source_max_rowid, source_generation, next_rowid, raw)| {
            let audit: FactLedgerAudit = serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)?;
            let verified_consistent = (status == "completed").then(|| audit.complete());
            Ok(FactAuditJob { id: job_id.into(), project_id: project_id.to_string(), status,
                version, source_count, source_max_rowid, source_generation, next_rowid,
                audit, verified_consistent, error: None })
        }).transpose()?.ok_or(RepositoryError::TaskNotFound)
}

fn persist_job(connection: &Connection, job: &mut FactAuditJob) -> Result<(), RepositoryError> {
    let changed = connection.execute(
        "UPDATE story_fact_audit_jobs_v1 SET status = ?1, version = version + 1, next_rowid = ?2, audit_json = ?3 WHERE project_id = ?4 AND job_id = ?5 AND version = ?6",
        params![job.status, job.next_rowid, serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?,
            job.project_id, job.id, job.version],
    ).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::InvalidContent(
            "fact audit version changed".into(),
        ));
    }
    job.version += 1;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "fact_audit_jobs_tests.rs"]
mod tests;
