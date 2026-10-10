use rusqlite::{Connection, OptionalExtension, params};

use crate::authoring_runs::storage_error;
use crate::task_action_permit::PermissionBasis;
use crate::{ProjectId, RepositoryError, TaskActionRequest};

pub(crate) fn queue(
    db: &Connection,
    project: &ProjectId,
    parent: &str,
    action: &TaskActionRequest,
    basis: PermissionBasis<'_>,
    now: i64,
) -> Result<(), RepositoryError> {
    crate::work_pause_barrier::assert_can_schedule(db, parent)?;
    let prior: Option<String> = db.query_row(
        "SELECT id FROM story_work_steps_v1 WHERE work_task_id=?1 AND status!='superseded' ORDER BY ordinal DESC LIMIT 1",
        [parent], |row|row.get(0),
    ).optional().map_err(storage_error)?;
    crate::task_action_permit::grant(db, project, parent, action, basis, now)?;
    db.execute(
        "INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status) VALUES(?1,?2,(SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?2),?3,?4,?5,?6,'queued')",
        params![action.id,parent,action.operation,action.target.to_string(),action.input_revision,prior],
    ).map_err(storage_error)?;
    if db.execute("UPDATE story_task_action_requests_v1 SET status='queued' WHERE action_id=?1 AND status='proposed'",[&action.id]).map_err(storage_error)? != 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::work_tasks::refresh_work_task(db, parent)
}
