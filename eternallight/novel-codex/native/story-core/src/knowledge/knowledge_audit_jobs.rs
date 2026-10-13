use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::assertion_index_integrity::{audit_assertion_batch_on, audit_assertion_orphans_on};
use crate::entity_index_integrity::{
    audit_entity_batch_on, audit_entity_orphans_on, entity_generation_on,
};
use crate::epistemic_index_integrity::{audit_epistemic_batch_on, audit_epistemic_orphans_on};
use crate::knowledge_head_integrity::{audit_head_batch_on, audit_head_orphans_on};
use crate::{
    AssertionIndexAudit, EntityIndexAudit, EpistemicIndexAudit, KnowledgeHeadAudit, ProjectId,
    RepositoryError, SqliteStoryRepository,
};

const CHUNK_BATCHES: i64 = 25;
const CHUNK_BYTES: usize = 250_000;
const MAX_ACTIVE_JOBS: i64 = 16;
const VALIDATION_VERSION: &str = "source-and-index-v1";

#[path = "knowledge_audit_scheduling.rs"]
mod scheduling;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAudit {
    pub entities: EntityIndexAudit,
    pub assertions: AssertionIndexAudit,
    pub epistemic: EpistemicIndexAudit,
    pub heads: KnowledgeHeadAudit,
}

impl KnowledgeAudit {
    fn empty() -> Self {
        Self {
            entities: EntityIndexAudit::empty(),
            assertions: AssertionIndexAudit::empty(),
            epistemic: EpistemicIndexAudit::empty(),
            heads: KnowledgeHeadAudit::empty(),
        }
    }

    pub fn complete(&self) -> bool {
        self.entities.complete()
            && self.assertions.complete()
            && self.epistemic.complete()
            && self.heads.complete()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS, serde::Deserialize)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAuditJob {
    pub id: String,
    pub project_id: String,
    pub status: String,
    pub version: i64,
    pub source_count: i64,
    pub source_max_rowid: i64,
    pub index_generation: i64,
    pub entity_generation: i64,
    pub next_rowid: i64,
    pub audit: KnowledgeAudit,
    pub verified_consistent: Option<bool>,
    pub error: Option<String>,
}



pub(crate) fn bump_knowledge_generation(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(), RepositoryError> {
    let next = knowledge_generation_on(connection, project_id)?
        .checked_add(1)
        .ok_or_else(|| {
            RepositoryError::InvalidContent("knowledge index generation exhausted".into())
        })?;
    connection.execute(
        "INSERT INTO story_knowledge_index_state_v1(project_id, generation) VALUES (?1, ?2) ON CONFLICT(project_id) DO UPDATE SET generation = excluded.generation",
        params![project_id.as_str(), next],
    ).map_err(storage_error)?;
    Ok(())
}

fn knowledge_generation_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT generation FROM story_knowledge_index_state_v1 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)
        .map(|value| value.unwrap_or(0))
}

/// The caller must keep this proof check and every indexed read in one transaction.
pub(crate) fn current_source_proof(
    connection: &Connection,
    project: &ProjectId,
) -> Result<Option<String>, RepositoryError> {
    if connection.is_autocommit() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let id: Option<String> = connection.query_row(
        "SELECT job_id FROM story_knowledge_audit_jobs_v1 WHERE project_id=?1 AND status='completed' AND validation_version=?2 AND index_generation=?3 AND entity_generation=?4 ORDER BY rowid DESC LIMIT 1",
        params![project.as_str(),VALIDATION_VERSION,knowledge_generation_on(connection,project)?,entity_generation_on(connection,project)?],
        |row|row.get(0),
    ).optional().map_err(storage_error)?;
    let Some(id) = id else {
        return Ok(None);
    };
    let job = load_job(connection, project, &id)?;
    let counts = [
        job.audit.entities.published_batches,
        job.audit.assertions.published_batches,
        job.audit.epistemic.published_batches,
        job.audit.heads.published_batches,
    ];
    Ok((job.audit.complete()
        && counts.iter().all(|count| *count as i64 == job.source_count)
        && source_matches(connection, project, &job)?)
    .then_some(id))
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
    job: &KnowledgeAuditJob,
) -> Result<bool, RepositoryError> {
    Ok(
        source_signature(connection, project_id)? == (job.source_count, job.source_max_rowid)
            && knowledge_generation_on(connection, project_id)? == job.index_generation
            && entity_generation_on(connection, project_id)? == job.entity_generation
            && connection.query_row("SELECT validation_version=?1 FROM story_knowledge_audit_jobs_v1 WHERE project_id=?2 AND job_id=?3",params![VALIDATION_VERSION,project_id.as_str(),job.id],|row|row.get::<_,bool>(0)).map_err(storage_error)?,
    )
}

fn load_job(
    connection: &Connection,
    project_id: &ProjectId,
    job_id: &str,
) -> Result<KnowledgeAuditJob, RepositoryError> {
    connection.query_row(
        "SELECT status, version, source_count, source_max_rowid, index_generation, next_rowid, audit_json, error, entity_generation FROM story_knowledge_audit_jobs_v1 WHERE project_id = ?1 AND job_id = ?2",
        params![project_id.as_str(), job_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?,row.get::<_,i64>(8)?)),
    ).optional().map_err(storage_error)?
        .map(|(status, version, source_count, source_max_rowid, index_generation, next_rowid, audit_json, error, entity_generation)| {
            let audit: KnowledgeAudit = serde_json::from_str(&audit_json).map_err(|_| RepositoryError::CorruptState)?;
            let verified_consistent = (status == "completed").then(|| audit.complete());
            Ok(KnowledgeAuditJob { id: job_id.into(), project_id: project_id.to_string(), status,
                version, source_count, source_max_rowid, index_generation, entity_generation, next_rowid, audit,
                verified_consistent, error })
        }).transpose()?.ok_or(RepositoryError::TaskNotFound)
}

fn persist_job(
    connection: &Connection,
    job: &mut KnowledgeAuditJob,
) -> Result<(), RepositoryError> {
    let changed = connection.execute(
        "UPDATE story_knowledge_audit_jobs_v1 SET status = ?1, version = version + 1, next_rowid = ?2, audit_json = ?3, error = ?4 WHERE project_id = ?5 AND job_id = ?6 AND version = ?7",
        params![job.status, job.next_rowid, serde_json::to_string(&job.audit).map_err(|_| RepositoryError::CorruptState)?,
            job.error, job.project_id, job.id, job.version],
    ).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::InvalidContent(
            "knowledge audit version changed".into(),
        ));
    }
    job.version += 1;
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "knowledge_audit_jobs_tests.rs"]
mod tests;

/// Public knowledge audit jobs capability; SQLite retains transaction ownership.
pub trait KnowledgeAuditJobsRepository {
    fn start_knowledge_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<KnowledgeAuditJob, RepositoryError>;
    fn knowledge_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<KnowledgeAuditJob, RepositoryError>;
    fn latest_knowledge_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<KnowledgeAuditJob>, RepositoryError>;
    fn next_knowledge_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError>;
    fn control_knowledge_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<KnowledgeAuditJob, RepositoryError>;
    /// Checks at most 25 published batches and normally 250 KB including bound drafts.
    /// One larger source is processed on its own so the checkpoint can advance.
    fn advance_knowledge_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<KnowledgeAuditJob, RepositoryError>;
}

impl KnowledgeAuditJobsRepository for SqliteStoryRepository {
    fn start_knowledge_audit(
        &mut self,
        project_id: &ProjectId,
    ) -> Result<KnowledgeAuditJob, RepositoryError> {
        self.project_revision(project_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let job = scheduling::start_on(&tx, project_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    fn knowledge_audit_job(
        &self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<KnowledgeAuditJob, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut job = load_job(&read, project_id, job_id)?;
        if matches!(
            job.status.as_str(),
            "queued" | "running" | "paused" | "completed"
        ) && !source_matches(&read, project_id, &job)?
        {
            job.status = "stale".into();
            job.verified_consistent = None;
        }
        read.commit().map_err(storage_error)?;
        Ok(job)
    }

    fn latest_knowledge_audit_job(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<KnowledgeAuditJob>, RepositoryError> {
        self.project_revision(project_id)?;
        let id = self.connection.query_row(
            "SELECT job_id FROM story_knowledge_audit_jobs_v1 WHERE project_id = ?1 ORDER BY CASE WHEN status IN ('queued','running','paused') THEN 0 ELSE 1 END, rowid DESC LIMIT 1",
            [project_id.as_str()], |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| self.knowledge_audit_job(project_id, &id))
            .transpose()
    }

    fn next_knowledge_audit_job(&self) -> Result<Option<(ProjectId, String)>, RepositoryError> {
        self.connection.query_row(
            "SELECT project_id, job_id FROM story_knowledge_audit_jobs_v1 WHERE status IN ('queued','running') ORDER BY version, rowid LIMIT 1",
            [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?
            .map(|(project, id)| Ok((ProjectId::parse(project).map_err(|_| RepositoryError::CorruptState)?, id)))
            .transpose()
    }

    fn control_knowledge_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
        command: &str,
        expected_version: i64,
    ) -> Result<KnowledgeAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if job.version != expected_version {
            return Err(RepositoryError::InvalidContent(
                "knowledge audit version changed".into(),
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
                    "invalid knowledge audit control".into(),
                ));
            }
        }
        .into();
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    /// Checks at most 25 published batches and normally 250 KB including bound drafts.
    /// One larger source is processed on its own so the checkpoint can advance.
    fn advance_knowledge_audit(
        &mut self,
        project_id: &ProjectId,
        job_id: &str,
    ) -> Result<KnowledgeAuditJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut job = load_job(&tx, project_id, job_id)?;
        if !matches!(job.status.as_str(), "queued" | "running") {
            return Err(RepositoryError::InvalidContent(
                "knowledge audit is not runnable".into(),
            ));
        }
        if !source_matches(&tx, project_id, &job)? {
            job.status = "stale".into();
            persist_job(&tx, &mut job)?;
            tx.commit().map_err(storage_error)?;
            return Ok(job);
        }
        let mut statement = tx.prepare(
            "SELECT b.rowid, b.batch_id, b.assertions_json, b.entities_json, b.object_id, b.source_revision_id, b.status, length(CAST(r.content_json AS BLOB)) FROM story_modeling_batches_v2 b LEFT JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.object_id=b.object_id AND r.revision_id=b.source_revision_id AND r.object_kind='draft' WHERE b.project_id = ?1 AND b.published_at IS NOT NULL AND b.rowid > ?2 AND b.rowid <= ?3 ORDER BY b.rowid LIMIT ?4",
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
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<usize>>(7)?,
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
                .saturating_add(batch.3.len())
                .saturating_add(batch.7.unwrap_or(0));
            if !batches.is_empty() && next > CHUNK_BYTES {
                break;
            }
            bytes = next;
            batches.push(batch);
        }
        drop(statement);
        job.status = "running".into();
        for (
            rowid,
            batch_id,
            assertions_json,
            entities_json,
            object_id,
            source_revision_id,
            status,
            _source_bytes,
        ) in batches
        {
            let result =
                audit_assertion_batch_on(&tx, project_id, &batch_id, &mut job.audit.assertions)
                    .and_then(|()| {
                        audit_entity_batch_on(
                            &tx,
                            project_id,
                            &batch_id,
                            &entities_json,
                            &mut job.audit.entities,
                        )
                    })
                    .and_then(|()| {
                        audit_epistemic_batch_on(
                            &tx,
                            project_id,
                            &batch_id,
                            &assertions_json,
                            &entities_json,
                            &mut job.audit.epistemic,
                        )
                    })
                    .and_then(|()| {
                        audit_head_batch_on(
                            &tx,
                            project_id,
                            &batch_id,
                            &object_id,
                            &source_revision_id,
                            &status,
                            &mut job.audit.heads,
                        )
                    });
            match result {
                Ok(()) => job.next_rowid = rowid,
                Err(RepositoryError::CorruptState | RepositoryError::InvalidContent(_)) => {
                    job.status = "failed".into();
                    job.error = Some(format!("published batch {batch_id} cannot be audited"));
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if job.status == "running"
            && job.audit.assertions.published_batches as i64 == job.source_count
        {
            audit_assertion_orphans_on(&tx, project_id, &mut job.audit.assertions)?;
            audit_entity_orphans_on(&tx, project_id, &mut job.audit.entities)?;
            audit_epistemic_orphans_on(&tx, project_id, &mut job.audit.epistemic)?;
            audit_head_orphans_on(&tx, project_id, &mut job.audit.heads)?;
            job.status = "completed".into();
            job.verified_consistent = Some(job.audit.complete());
        }
        persist_job(&tx, &mut job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }
}

pub use scheduling::KnowledgeAuditSchedulingRepository;

use crate::ProjectRevisionRepository;
