use rusqlite::{Connection, params};

use crate::{ProjectId, RepositoryError};

/// The next author action, derived from current domain evidence rather than UI state.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkResumeMode {
    RecoverSavedResult,
    ContinueExecution,
}

pub(crate) fn resume_mode(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    kind: &str,
    status: &str,
    block_reason: Option<&str>,
) -> Result<Option<WorkResumeMode>, RepositoryError> {
    if !matches!(status, "paused" | "blocked")
        || block_reason.is_some_and(|reason| reason != "unknownUsage")
    {
        return Ok(None);
    }
    if kind != "generation"
        && (!eligible_attempts(db, project, work)?.is_empty()
            || !crate::planning_reviews::eligible_saved_reviews(db, project, work)?.is_empty())
    {
        // This exact operation verifies a durable saved domain result;
        // it neither resolves unknown usage nor admits another model attempt.
        return Ok(Some(WorkResumeMode::RecoverSavedResult));
    }
    Ok(block_reason
        .is_none()
        .then_some(WorkResumeMode::ContinueExecution))
}

pub(crate) fn pending_results(db: &Connection, project: &ProjectId, work: &str) -> Result<bool, RepositoryError> {
    Ok(pending_reason(db, project, work)?.is_some())
}

/// Keep write uncertainty separate from the provider grant settlement fence.
/// Both may hold at once; neither is inferred from a read failure.
pub(crate) fn pending_reason(db: &Connection, project: &ProjectId, work: &str) -> Result<Option<&'static str>, RepositoryError> {
    let (writes, usage): (bool, bool) = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_tool_operations_v1 o \
         JOIN story_work_attempts_v1 a ON a.attempt_id=o.task_id \
         JOIN story_work_steps_v1 s ON s.id=a.step_id \
         JOIN story_work_tasks_v1 w ON w.id=s.work_task_id \
         WHERE w.id=?1 AND w.project_id=?2 AND o.project_id=?2 AND o.state='accepted'), \
         EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 g \
         JOIN story_work_tasks_v1 w ON w.id=g.work_task_id \
         WHERE w.id=?1 AND w.project_id=?2 AND g.settled=0)",
        params![work, project.as_str()], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(storage_error)?;
    Ok(match (writes, usage) {
        (true, true) => Some("pendingWriteAndUsage"),
        (true, false) => Some("pendingRecovery"),
        (false, true) => Some("pendingUsage"),
        (false, false) => None,
    })
}

/// Only a durable, still-current publication owned by this failed attempt can
/// satisfy its step. Unknown writes require reconciliation, not inferred success.
pub(crate) fn eligible_attempts(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<Vec<String>, RepositoryError> {
    if pending_results(db, project, work)? {
        return Ok(Vec::new());
    }
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

/// Published analysis can be recovered only while its exact publication is current.
/// A missing/stale knowledge head is not an unpublished attempt to retry.
pub(crate) fn publication_changed(
    db: &Connection,
    project: &ProjectId,
    work: &str,
) -> Result<bool, RepositoryError> {
    let mut statement = db
        .prepare(
            "SELECT t.task_id,b.status,b.publisher_task_id FROM story_work_steps_v1 s \
         JOIN story_work_tasks_v1 w ON w.id=s.work_task_id \
         JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id \
         LEFT JOIN story_modeling_batches_v2 b ON b.project_id=t.project_id \
           AND b.batch_id=json_extract(t.target_json,'$.batchId') \
         WHERE w.project_id=?1 AND w.id=?2 AND s.status IN ('queued','blocked') \
           AND s.operation='modelKnowledge'",
        )
        .map_err(storage_error)?;
    let candidates = statement
        .query_map(params![project.as_str(), work], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    for (id, status, publisher) in candidates {
        if status.as_deref() == Some("queued") && publisher.is_none() {
            continue;
        }
        let task = crate::tasks::load_task(db, &id)?;
        if !crate::tasks::recoverable_output(db, &task)? {
            return Ok(true);
        }
    }
    Ok(false)
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

#[cfg(test)]
mod pending_state_tests {
    use super::*;
    #[test]
    fn write_and_usage_pending_are_independent_and_project_scoped() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT,project_id TEXT); CREATE TABLE story_work_steps_v1(id TEXT,work_task_id TEXT); CREATE TABLE story_work_attempts_v1(attempt_id TEXT,step_id TEXT); CREATE TABLE story_tool_operations_v1(task_id TEXT,project_id TEXT,state TEXT); CREATE TABLE story_task_provider_budgets_v1(task_id TEXT,work_task_id TEXT,settled INTEGER); INSERT INTO story_work_tasks_v1 VALUES('w','project'); INSERT INTO story_work_steps_v1 VALUES('s','w'); INSERT INTO story_work_attempts_v1 VALUES('t','s');").unwrap();
        let project=ProjectId::parse("project").unwrap();
        for (write,usage,expected) in [(false,false,None),(true,false,Some("pendingRecovery")),(false,true,Some("pendingUsage")),(true,true,Some("pendingWriteAndUsage"))] {
            db.execute("DELETE FROM story_tool_operations_v1",[]).unwrap();
            db.execute("DELETE FROM story_task_provider_budgets_v1",[]).unwrap();
            if write { db.execute("INSERT INTO story_tool_operations_v1 VALUES('t','project','accepted')",[]).unwrap(); }
            if usage { db.execute("INSERT INTO story_task_provider_budgets_v1 VALUES('t','w',0)",[]).unwrap(); }
            assert_eq!(pending_reason(&db,&project,"w").unwrap(),expected);
            assert_eq!(pending_reason(&db,&ProjectId::parse("other").unwrap(),"w").unwrap(),None);
        }
    }
}
