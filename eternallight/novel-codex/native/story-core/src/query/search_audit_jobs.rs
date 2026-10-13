use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use crate::search_rebuild::{audit_orphans_on, audit_revision_on, index_generation_on};
use crate::{ProjectId, RepositoryError, SearchIndexAudit, SqliteStoryRepository};

const AUDIT_CHUNK_REVISIONS: i64 = 100;
const AUDIT_CHUNK_BYTES: usize = 1_000_000;
const MAX_ACTIVE_AUDITS: i64 = 16;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS, serde::Deserialize)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchAuditJob {
    pub id: String,
    pub project_id: String,
    pub status: String,
    pub version: i64,
    pub source_count: i64,
    pub source_max_rowid: i64,
    pub index_generation: i64,
    pub next_rowid: i64,
    pub audit: SearchIndexAudit,
    /// None while incomplete; false when a completed audit found an inconsistency.
    pub verified_consistent: Option<bool>,
    pub error: Option<String>,
}



fn source_signature(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(i64, i64), RepositoryError> {
    connection.query_row(
        "SELECT COUNT(*), COALESCE(MAX(rowid), 0) FROM story_revisions_v2 WHERE project_id = ?1",
        [project_id.as_str()], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(storage_error)
}

fn source_matches(
    connection: &Connection,
    project_id: &ProjectId,
    job: &SearchAuditJob,
) -> Result<bool, RepositoryError> {
    Ok(
        source_signature(connection, project_id)? == (job.source_count, job.source_max_rowid)
            && index_generation_on(connection, project_id)? == job.index_generation,
    )
}

fn mark_outdated_for_read(
    connection: &Connection,
    project_id: &ProjectId,
    job: &mut SearchAuditJob,
) -> Result<(), RepositoryError> {
    if matches!(
        job.status.as_str(),
        "queued" | "running" | "paused" | "completed"
    ) && !source_matches(connection, project_id, job)?
    {
        job.status = "stale".into();
        job.verified_consistent = None;
    }
    Ok(())
}

fn load_job(
    connection: &Connection,
    project_id: &ProjectId,
    id: &str,
) -> Result<SearchAuditJob, RepositoryError> {
    connection.query_row(
        "SELECT status, version, source_count, source_max_rowid, index_generation, next_rowid, audit_json, error FROM story_search_audit_jobs_v1 WHERE project_id = ?1 AND job_id = ?2",
        params![project_id.as_str(), id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?)),
    ).optional().map_err(storage_error)?
        .map(|(status, version, source_count, source_max_rowid, index_generation, next_rowid, audit_json, error)| {
            Ok(SearchAuditJob {
                id: id.into(), project_id: project_id.to_string(), status, version,
                source_count, source_max_rowid, index_generation, next_rowid,
                audit: serde_json::from_str(&audit_json).map_err(|_| RepositoryError::CorruptState)?,
                verified_consistent: None,
                error,
            })
        }).transpose()?
            .map(|mut job| {
                if job.status == "completed" {
                    job.verified_consistent = Some(job.audit.complete());
                }
                job
            })
            .ok_or(RepositoryError::TaskNotFound)
}

fn persist_job(connection: &Connection, job: &mut SearchAuditJob) -> Result<(), RepositoryError> {
    let changed = connection.execute(
        "UPDATE story_search_audit_jobs_v1 SET status = ?1, version = version + 1, next_rowid = ?2, audit_json = ?3, error = ?4 WHERE project_id = ?5 AND job_id = ?6 AND version = ?7",
        params![job.status, job.next_rowid,
            serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?,
            job.error, job.project_id, job.id, job.version],
    ).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::InvalidContent(
            "search audit version changed".into(),
        ));
    }
    job.version += 1;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "search_audit_jobs_tests.rs"]
mod tests;

/// Public search audit jobs capability; SQLite retains transaction ownership.
pub trait SearchAuditJobsRepository {
    fn start_search_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<SearchAuditJob, RepositoryError>;
    fn search_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<SearchAuditJob, RepositoryError>;
    fn latest_search_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<SearchAuditJob>, RepositoryError>;
    fn next_search_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError>;
    fn control_search_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<SearchAuditJob, RepositoryError>;
    /// Processes at most 100 immutable revisions and normally 1 MB of source in one transaction.
    /// One revision may exceed the byte limit so that the checkpoint can always advance. A different source
    /// manifest or index generation makes the checkpoint stale, never globally complete.
    fn advance_search_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<SearchAuditJob, RepositoryError>;
}

impl SearchAuditJobsRepository for SqliteStoryRepository {
    fn start_search_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<SearchAuditJob, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let existing = tx.query_row(
            "SELECT job_id FROM story_search_audit_jobs_v1 WHERE project_id = ?1 AND status IN ('queued','running','paused')",
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
            "SELECT COUNT(*) FROM story_search_audit_jobs_v1 WHERE status IN ('queued','running','paused')",
            [], |row| row.get(0),
        ).map_err(storage_error)?;
        if active >= MAX_ACTIVE_AUDITS {
            return Err(RepositoryError::InvalidContent(
                "search audit queue is full".into(),
            ));
        }
        let (source_count, source_max_rowid) = source_signature(&tx, project_id)?;
        let index_generation = index_generation_on(&tx, project_id)?;
        let job = SearchAuditJob {
            id: format!("search-audit-{}", uuid::Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            status: "queued".into(),
            version: 0,
            source_count,
            source_max_rowid,
            index_generation,
            next_rowid: 0,
            audit: SearchIndexAudit {
                revision_count: 0,
                document_count: 0,
                gram_count: 0,
                mismatch_count: 0,
                mismatched_revision_ids: Vec::new(),
                orphaned_documents: 0,
                orphaned_grams: 0,
                index_generation,
            },
            verified_consistent: None,
            error: None,
        };
        tx.execute(
            "INSERT INTO story_search_audit_jobs_v1(job_id, project_id, status, source_count, source_max_rowid, index_generation, audit_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![job.id, job.project_id, job.status, job.source_count, job.source_max_rowid,
                job.index_generation, serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    fn search_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<SearchAuditJob, RepositoryError> {
        let mut job = load_job(&self.connection, project_id, job_id)?;
        mark_outdated_for_read(&self.connection, project_id, &mut job)?;
        Ok(job)
    }

    fn latest_search_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<SearchAuditJob>, RepositoryError> {
        self.project_revision(project_id)?;
        let id = self.connection.query_row(
            "SELECT job_id FROM story_search_audit_jobs_v1 WHERE project_id = ?1 ORDER BY CASE WHEN status IN ('queued','running','paused') THEN 0 ELSE 1 END, rowid DESC LIMIT 1",
            [project_id.as_str()], |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| self.search_audit_job(project_id, &id))
            .transpose()
    }

    fn next_search_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError> {
        self.connection.query_row(
            "SELECT project_id, job_id FROM story_search_audit_jobs_v1 WHERE status IN ('queued','running') ORDER BY version, rowid LIMIT 1",
            [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?
            .map(|(project, id)| Ok((ProjectId::parse(project).map_err(|_| RepositoryError::CorruptState)?, id)))
            .transpose()
    }

    fn control_search_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<SearchAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if job.version != expected_version {
            return Err(RepositoryError::InvalidContent(
                "search audit version changed".into(),
            ));
        }
        job.status = match (command, job.status.as_str()) {
            ("pause", "queued" | "running") => "paused",
            ("resume", "paused") => {
                if !source_matches(&tx, project_id, &job)? {
                    "stale"
                } else {
                    "queued"
                }
            }
            ("cancel", "queued" | "running" | "paused") => "cancelled",
            _ => {
                return Err(RepositoryError::InvalidContent(
                    "invalid search audit control".into(),
                ));
            }
        }
        .into();
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    /// Processes at most 100 immutable revisions and normally 1 MB of source in one transaction.
    /// One revision may exceed the byte limit so that the checkpoint can always advance. A different source
    /// manifest or index generation makes the checkpoint stale, never globally complete.
    fn advance_search_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<SearchAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if !matches!(job.status.as_str(), "queued" | "running") {
            return Err(RepositoryError::InvalidContent(
                "search audit is not runnable".into(),
            ));
        }
        if !source_matches(&tx, project_id, &job)? {
            job.status = "stale".into();
            persist_job(&tx, &mut job)?;
            tx.commit().map_err(storage_error)?;
            return Ok(job);
        }
        let mut statement = tx.prepare(
            "SELECT rowid, revision_id, object_id, content_json FROM story_revisions_v2 WHERE project_id = ?1 AND rowid > ?2 AND rowid <= ?3 ORDER BY rowid LIMIT ?4",
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id.as_str(),
                    job.next_rowid,
                    job.source_max_rowid,
                    AUDIT_CHUNK_REVISIONS
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
        let mut revisions = Vec::new();
        let mut source_bytes = 0usize;
        for row in rows {
            let revision = row.map_err(storage_error)?;
            let next_bytes = source_bytes.saturating_add(revision.3.len());
            if !revisions.is_empty() && next_bytes > AUDIT_CHUNK_BYTES {
                break;
            }
            source_bytes = next_bytes;
            revisions.push(revision);
        }
        drop(statement);
        job.status = "running".into();
        for (rowid, revision_id, object_id, content_json) in revisions {
            match audit_revision_on(
                &tx,
                project_id,
                &revision_id,
                &object_id,
                &content_json,
                &mut job.audit,
            ) {
                Ok(()) => job.next_rowid = rowid,
                Err(RepositoryError::CorruptState | RepositoryError::InvalidContent(_)) => {
                    job.status = "failed".into();
                    job.error = Some(format!("source revision {revision_id} cannot be audited"));
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if job.status == "running" && job.audit.revision_count as i64 == job.source_count {
            audit_orphans_on(&tx, project_id, &mut job.audit)?;
            job.status = "completed".into();
            job.verified_consistent = Some(job.audit.complete());
        }
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }
}

use crate::ProjectRevisionRepository;
