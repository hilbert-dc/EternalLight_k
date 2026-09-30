use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

impl SqliteStoryRepository {
    /// Reserves at most one due source as an ordinary WorkTask. It does not call a model.
    pub fn dispatch_modeling_intent(
        &mut self,
        project: &ProjectId,
        now_ms: i64,
    ) -> Result<Option<Value>, RepositoryError> {
        if now_ms < 0 {
            return Err(RepositoryError::InvalidContent(
                "invalid dispatch time".into(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let revision: i64 = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
                [project.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::ProjectNotFound)?;
        // Only unclaimed, still-running reservations may be superseded automatically.
        tx.execute("UPDATE story_modeling_dispatch_v1 SET superseded=1 WHERE project_id=?1 AND step_id IN (SELECT s.id FROM story_work_steps_v1 s JOIN story_work_tasks_v1 t ON t.id=s.work_task_id JOIN story_modeling_dispatch_v1 d ON d.step_id=s.id JOIN story_modeling_outbox_v1 q ON q.project_id=d.project_id AND q.object_id=d.object_id WHERE t.status='running' AND s.status='queued' AND NOT EXISTS(SELECT 1 FROM story_work_attempts_v1 a WHERE a.step_id=s.id) AND (d.intent_generation!=q.generation OR q.status!='pending' OR s.input_revision!=?2))",params![project.as_str(),revision]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_steps_v1 SET status='cancelled' WHERE id IN (SELECT s.id FROM story_work_steps_v1 s JOIN story_work_tasks_v1 t ON t.id=s.work_task_id LEFT JOIN story_modeling_dispatch_v1 d ON d.step_id=s.id LEFT JOIN story_modeling_outbox_v1 q ON q.project_id=d.project_id AND q.object_id=d.object_id WHERE t.project_id=?1 AND json_extract(s.target_json,'$.modelingIntent')=1 AND t.status='running' AND s.status='queued' AND NOT EXISTS(SELECT 1 FROM story_work_attempts_v1 a WHERE a.step_id=s.id) AND (q.status IS NULL OR d.intent_generation!=q.generation OR q.status!='pending' OR s.input_revision!=?2))", params![project.as_str(),revision]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_tasks_v1 SET status='cancelled',version=version+1 WHERE project_id=?1 AND status='running' AND id IN (SELECT s.work_task_id FROM story_work_steps_v1 s WHERE json_extract(s.target_json,'$.modelingIntent')=1 AND s.status='cancelled')", [project.as_str()]).map_err(storage_error)?;
        let existing: Option<String> = tx.query_row("SELECT s.work_task_id FROM story_modeling_dispatch_v1 d JOIN story_modeling_outbox_v1 q ON q.project_id=d.project_id AND q.object_id=d.object_id JOIN story_work_steps_v1 s ON s.id=d.step_id JOIN story_work_tasks_v1 t ON t.id=s.work_task_id WHERE d.project_id=?1 AND q.status='pending' AND d.intent_generation=q.generation AND t.status='running' AND s.status='queued' AND s.input_revision=?2 ORDER BY q.rowid LIMIT 1", params![project.as_str(),revision], |row| row.get(0)).optional().map_err(storage_error)?;
        if let Some(id) = existing {
            tx.commit().map_err(storage_error)?;
            return Ok(Some(json!({"workTaskId":id})));
        }
        let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_tasks_v1 t ON t.id=s.work_task_id WHERE t.project_id=?1 AND json_extract(s.target_json,'$.modelingIntent')=1 AND t.status='running' AND s.status IN ('queued','running'))",[project.as_str()],|row|row.get(0)).map_err(storage_error)?;
        if active {
            tx.commit().map_err(storage_error)?;
            return Ok(None);
        }
        // A paused/blocked/active modeling step owns this object, even across a new source.
        let source: Option<(String,String,i64,String)> = tx.query_row(
            "SELECT q.object_id,q.source_revision_id,q.generation,c.chapter_id FROM story_modeling_outbox_v1 q JOIN story_object_heads_v2 h ON h.project_id=q.project_id AND h.object_id=q.object_id JOIN story_chapters_v2 c ON c.project_id=q.project_id AND 'draft-'||c.chapter_id=q.object_id LEFT JOIN story_modeling_dispatch_v1 d ON d.project_id=q.project_id AND d.object_id=q.object_id WHERE q.project_id=?1 AND q.status='pending' AND q.source_revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id) AND MIN(q.last_dirty_ms+1000,q.first_dirty_ms+5000)<=?2 AND (d.step_id IS NULL OR d.intent_generation!=q.generation OR d.superseded=1) AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_work_tasks_v1 t ON t.id=s.work_task_id LEFT JOIN story_modeling_batches_v2 b ON b.batch_id=json_extract(s.target_json,'$.batchId') AND b.project_id=t.project_id WHERE t.project_id=q.project_id AND t.status IN ('running','paused','blocked') AND s.operation='modelKnowledge' AND s.status IN ('queued','running','blocked') AND (b.object_id=q.object_id OR json_extract(s.target_json,'$.chapterId')=c.chapter_id)) ORDER BY q.rowid LIMIT 1",
            params![project.as_str(),now_ms], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).optional().map_err(storage_error)?;
        let Some((object, source, generation, chapter)) = source else {
            tx.commit().map_err(storage_error)?;
            return Ok(None);
        };
        let work = format!("work-{}", uuid::Uuid::new_v4().simple());
        let step = format!("step-{}", uuid::Uuid::new_v4().simple());
        let batch = format!("modeling-{}", uuid::Uuid::new_v4().simple());
        // Algorithm versions are stable; stale batches remain evidence, not live dedup keys.
        tx.execute("INSERT OR IGNORE INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,created_at) VALUES(?1,?2,?3,?4,?5,'novel-knowledge-v1','queued',?6)", params![batch,project.as_str(),object,source,revision,now_ms/1000]).map_err(storage_error)?;
        let batch: String = tx.query_row("SELECT batch_id FROM story_modeling_batches_v2 WHERE project_id=?1 AND object_id=?2 AND source_revision_id=?3 AND input_project_revision=?4 AND extractor_version='novel-knowledge-v1' AND status='queued'",params![project.as_str(),object,source,revision],|row|row.get(0)).map_err(storage_error)?;
        tx.execute("INSERT INTO story_work_tasks_v1(id,project_id,thread_id,goal,kind,status,input_revision) VALUES(?1,?2,'','自动分析章节知识','direct','running',?3)", params![work,project.as_str(),revision]).map_err(storage_error)?;
        tx.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,status) VALUES(?1,?2,1,'modelKnowledge',?3,?4,'queued')", params![step,work,json!({"batchId":batch,"chapterId":chapter,"modelingIntent":true}).to_string(),revision]).map_err(storage_error)?;
        let domain_source = crate::task_domain_source::DomainTaskSource {
            source_type: crate::task_domain_source::DomainSourceKind::ModelingOutbox,
            work_task_id: work.clone(),
            step_id: step.clone(),
            batch_id: batch.clone(),
            object_id: object.clone(),
            source_revision_id: source.clone(),
            chapter_id: chapter.clone(),
            intent_generation: generation,
        };
        tx.execute(
            "INSERT INTO story_domain_task_sources_v1(step_id,source_json) VALUES(?1,?2)",
            params![
                step,
                serde_json::to_string(&domain_source).map_err(|_| RepositoryError::CorruptState)?
            ],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO story_modeling_dispatch_v1(project_id,object_id,intent_generation,step_id) VALUES(?1,?2,?3,?4) ON CONFLICT(project_id,object_id) DO UPDATE SET intent_generation=excluded.intent_generation,step_id=excluded.step_id,superseded=0",params![project.as_str(),object,generation,step]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(Some(json!({"workTaskId":work})))
    }
}

pub(crate) fn validate_claim(
    db: &Connection,
    project: &ProjectId,
    step: &str,
    count: i64,
    target: &Value,
) -> Result<(), RepositoryError> {
    let dispatch: Option<bool> = db.query_row("SELECT q.status='pending' AND q.generation=d.intent_generation AND q.source_revision_id=b.source_revision_id AND q.source_revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id) FROM story_modeling_dispatch_v1 d JOIN story_modeling_outbox_v1 q ON q.project_id=d.project_id AND q.object_id=d.object_id JOIN story_work_steps_v1 s ON s.id=d.step_id JOIN story_modeling_batches_v2 b ON b.batch_id=json_extract(s.target_json,'$.batchId') AND b.project_id=d.project_id JOIN story_object_heads_v2 h ON h.project_id=d.project_id AND h.object_id=d.object_id WHERE d.project_id=?1 AND d.step_id=?2",params![project.as_str(),step],|row|row.get(0)).optional().map_err(storage_error)?;
    if target["modelingIntent"] == true && (dispatch != Some(true) || count >= 3) {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
#[path = "modeling_dispatch_tests.rs"]
mod tests;
