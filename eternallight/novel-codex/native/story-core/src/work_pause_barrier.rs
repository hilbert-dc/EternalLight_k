//! Author-owned scheduling fence. Existing attempts retain only their original grants.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, WorkTask};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_work_pause_barriers_v1 (
 work_task_id TEXT PRIMARY KEY REFERENCES story_work_tasks_v1(id),
 control_revision INTEGER NOT NULL,
 attempt_ids_json TEXT NOT NULL
);
"#;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkPauseAfterStep {
    pub control_revision: i64,
    pub remaining_attempts: i64,
}

pub(crate) fn read(
    db: &Connection,
    work: &str,
) -> Result<Option<WorkPauseAfterStep>, RepositoryError> {
    db.query_row(
        "SELECT b.control_revision,(SELECT COUNT(*) FROM json_each(b.attempt_ids_json) ids JOIN story_tasks_v2 t ON t.task_id=ids.value WHERE t.status IN ('queued','running','waiting')) FROM story_work_pause_barriers_v1 b WHERE b.work_task_id=?1",
        [work], |r| Ok(WorkPauseAfterStep { control_revision: r.get(0)?, remaining_attempts: r.get(1)? }),
    ).optional().map_err(storage_error)
}

pub(crate) fn assert_can_schedule(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    if read(db, work)?.is_some() {
        return Err(RepositoryError::TaskNotWritable);
    }
    crate::work_interludes::assert_schedule(db,work,None)
}

pub(crate) fn clear(db: &Connection, work: &str) -> Result<(), RepositoryError> {
    db.execute(
        "DELETE FROM story_work_pause_barriers_v1 WHERE work_task_id=?1",
        [work],
    )
    .map_err(storage_error)?;
    Ok(())
}

/// Reconcile only a frozen set. It cannot grant another attempt, alter a receipt,
/// settle usage, clear the active task identity or certify delivery.
pub(crate) fn reconcile(db: &Connection, work: &str) -> Result<bool, RepositoryError> {
    let Some(barrier) = read(db, work)? else {
        return Ok(false);
    };
    if barrier.remaining_attempts == 0 {
        db.execute("UPDATE story_work_tasks_v1 SET status='paused',version=version+1 WHERE id=?1 AND status IN ('running','blocked','completed')", [work]).map_err(storage_error)?;
        db.execute("UPDATE story_generation_runs_v1 SET version=version+1,state_json=json_set(state_json,'$.status','paused','$.version',version+1) WHERE 'work-'||run_id=?1 AND json_extract(state_json,'$.status') IN ('running','blocked','completed')", [work]).map_err(storage_error)?;
    }
    Ok(true)
}

impl SqliteStoryRepository {
    pub fn can_resume_failed_after_step(
        &self,
        project: &ProjectId,
        run: &str,
        task: &str,
    ) -> Result<bool, RepositoryError> {
        let work = format!("work-{run}");
        let bound: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id AND s.current_attempt_id=?3 WHERE w.id=?1 AND w.project_id=?2 AND w.status='paused')", params![work,project.as_str(),task], |r| r.get(0)).map_err(storage_error)?;
        if !bound || !read(&self.connection, &work)?.is_some_and(|b| b.remaining_attempts == 0) {
            return Ok(false);
        }
        // Explicit author recovery is distinct from automatic retry eligibility.
        // A provider may have completed (and charged) while domain completion
        // failed; keep that receipt rather than requiring a fictional failed bill.
        let unsettled: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE task_id=?1 AND settled=0) OR EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=?1 AND state='accepted')", [task], |r| r.get(0)).map_err(storage_error)?;
        let budget = crate::work_budget_storage::load(&self.connection, project, &work)?;
        Ok(!unsettled && budget.has_charge(task) && budget.continuation_block_reason().is_none())
    }

    pub fn pause_work_after_step(
        &mut self,
        project: &ProjectId,
        work: &str,
        expected: i64,
    ) -> Result<WorkTask, RepositoryError> {
        let tx = crate::work_control_commands::ControlTransaction::new(&self.connection)?;
        let prior: Option<(String, i64)> = tx.query_row("SELECT status,control_revision FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2", params![work,project.as_str()], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
        let (status, revision) = prior.ok_or(RepositoryError::TaskNotFound)?;
        if revision != expected {
            return Err(RepositoryError::WorkTaskStale);
        }
        if status != "running" {
            return Err(RepositoryError::TaskNotWritable);
        }
        assert_can_schedule(&tx, work)?;
        tx.execute("INSERT INTO story_work_pause_barriers_v1(work_task_id,control_revision,attempt_ids_json) SELECT ?1,?2,COALESCE(json_group_array(t.task_id),'[]') FROM story_work_steps_v1 s JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.status IN ('queued','running','waiting')", params![work,expected+1]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_tasks_v1 SET control_revision=control_revision+1,version=version+1 WHERE id=?1", [work]).map_err(storage_error)?;
        reconcile(&tx, work)?;
        let result = crate::work_tasks::load_work_task(self, &tx, project, work)?;
        tx.commit()?;
        Ok(result)
    }
}
