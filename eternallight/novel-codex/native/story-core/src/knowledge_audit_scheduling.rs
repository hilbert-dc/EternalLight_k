use super::{
    KnowledgeAudit, KnowledgeAuditJob, MAX_ACTIVE_JOBS, VALIDATION_VERSION, entity_generation_on,
    knowledge_generation_on, load_job, persist_job, source_matches, source_signature,
    storage_error,
};
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

impl SqliteStoryRepository {
    /// Schedule one long-work source audit without overriding author controls or retrying failures.
    /// Selection, queue capacity and job creation share one write transaction.
    pub fn schedule_knowledge_audit(
        &mut self,
    ) -> Result<Option<KnowledgeAuditJob>, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let full: bool = tx.query_row("SELECT COUNT(*)>=?1 FROM story_knowledge_audit_jobs_v1 WHERE status IN ('queued','running','paused')",[MAX_ACTIVE_JOBS],|row|row.get(0)).map_err(storage_error)?;
        if full {
            return Ok(None);
        }
        let project: Option<String> = tx.query_row(
            "SELECT p.project_id FROM story_projects_v2 p \
             LEFT JOIN story_knowledge_audit_jobs_v1 j ON j.rowid=(SELECT MAX(rowid) FROM story_knowledge_audit_jobs_v1 WHERE project_id=p.project_id) \
             WHERE EXISTS(SELECT 1 FROM story_modeling_batches_v2 b WHERE b.project_id=p.project_id AND b.published_at IS NOT NULL LIMIT 1 OFFSET 200) \
             AND NOT EXISTS(SELECT 1 FROM story_knowledge_audit_jobs_v1 a WHERE a.project_id=p.project_id AND a.status IN ('queued','running','paused')) \
             AND (j.job_id IS NULL OR j.status='stale' OR (j.status='completed' AND ( \
               j.validation_version!=?1 OR j.index_generation!=COALESCE((SELECT generation FROM story_knowledge_index_state_v1 WHERE project_id=p.project_id),0) \
               OR j.entity_generation!=COALESCE((SELECT generation FROM story_entity_index_state_v1 WHERE project_id=p.project_id),0) \
               OR j.source_count!=(SELECT COUNT(*) FROM story_modeling_batches_v2 WHERE project_id=p.project_id AND published_at IS NOT NULL) \
               OR j.source_max_rowid!=(SELECT COALESCE(MAX(rowid),0) FROM story_modeling_batches_v2 WHERE project_id=p.project_id AND published_at IS NOT NULL)))) \
             ORDER BY COALESCE(j.rowid,0),p.project_id LIMIT 1",
            [VALIDATION_VERSION],|row|row.get(0),
        ).optional().map_err(storage_error)?;
        let Some(project) = project else {
            return Ok(None);
        };
        let project = ProjectId::parse(project).map_err(|_| RepositoryError::CorruptState)?;
        let job = start_on(&tx, &project)?;
        tx.commit().map_err(storage_error)?;
        Ok(Some(job))
    }
}

pub(super) fn start_on(
    tx: &Connection,
    project_id: &ProjectId,
) -> Result<KnowledgeAuditJob, RepositoryError> {
    let existing = tx.query_row(
        "SELECT job_id FROM story_knowledge_audit_jobs_v1 WHERE project_id=?1 AND status IN ('queued','running','paused')",
        [project_id.as_str()],|row|row.get::<_,String>(0),
    ).optional().map_err(storage_error)?;
    if let Some(id) = existing {
        let mut job = load_job(tx, project_id, &id)?;
        if source_matches(tx, project_id, &job)? {
            return Ok(job);
        }
        job.status = "stale".into();
        persist_job(tx, &mut job)?;
    }
    let active: i64 = tx.query_row("SELECT COUNT(*) FROM story_knowledge_audit_jobs_v1 WHERE status IN ('queued','running','paused')",[],|row|row.get(0)).map_err(storage_error)?;
    if active >= MAX_ACTIVE_JOBS {
        return Err(RepositoryError::InvalidContent(
            "knowledge audit queue is full".into(),
        ));
    }
    let (source_count, source_max_rowid) = source_signature(tx, project_id)?;
    let job = KnowledgeAuditJob {
        id: format!("knowledge-audit-{}", uuid::Uuid::new_v4().simple()),
        project_id: project_id.to_string(),
        status: "queued".into(),
        version: 0,
        source_count,
        source_max_rowid,
        index_generation: knowledge_generation_on(tx, project_id)?,
        entity_generation: entity_generation_on(tx, project_id)?,
        next_rowid: 0,
        audit: KnowledgeAudit::empty(),
        verified_consistent: None,
        error: None,
    };
    tx.execute(
        "INSERT INTO story_knowledge_audit_jobs_v1(job_id,project_id,status,source_count,source_max_rowid,index_generation,audit_json,entity_generation,validation_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![job.id,job.project_id,job.status,job.source_count,job.source_max_rowid,job.index_generation,serde_json::to_string(&job.audit).map_err(|_|RepositoryError::CorruptState)?,job.entity_generation,VALIDATION_VERSION],
    ).map_err(storage_error)?;
    Ok(job)
}
