use rusqlite::{Connection, params};

use crate::{ProjectId, RepositoryError};

/// Only a durable, still-current publication owned by this failed attempt can
/// satisfy its step. Unknown writes require reconciliation, not inferred success.
pub(crate) fn eligible_attempts(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<Vec<String>, RepositoryError> {
    let mut statement = db.prepare(
        "SELECT t.task_id FROM story_work_steps_v1 s JOIN story_work_tasks_v1 w ON w.id=s.work_task_id \
         JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id \
         WHERE w.project_id=?1 AND w.id=?2 AND s.status='blocked' AND s.operation='modelKnowledge' \
         AND t.status='failed' AND NOT EXISTS(SELECT 1 FROM story_tool_operations_v1 o \
           WHERE o.project_id=w.project_id AND o.task_id=t.task_id AND o.state='accepted')",
    ).map_err(storage_error)?;
    let candidates = statement
        .query_map(params![project.as_str(), work], |row| {
            row.get::<_, String>(0)
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    let mut eligible = Vec::new();
    for id in candidates {
        let task = crate::tasks::load_task(db, &id)?;
        if crate::tasks::recoverable_output(db, &task)? {
            eligible.push(id);
        }
    }
    Ok(eligible)
}

pub(crate) fn recover_published_steps(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    now: i64,
) -> Result<(), RepositoryError> {
    for id in eligible_attempts(db, project, work)? {
        // Retain the original failure in error for the execution audit. Domain
        // completion does not fabricate a model reply or an MCP completion call.
        db.execute(
            "UPDATE story_tasks_v2 SET status='completed',stage='ready_for_review',delivery_status='ready_for_review', \
             result_summary='作者恢复：已核验保存的知识产物，无需重复建模',capability_revoked_at=?2 \
             WHERE task_id=?1 AND status='failed'",
            params![id,now],
        ).map_err(storage_error)?;
        let recovered = crate::tasks::load_task(db, &id)?;
        crate::work_tasks::finish_attempt(db, &recovered)?;
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
