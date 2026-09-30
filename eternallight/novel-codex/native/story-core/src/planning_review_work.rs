use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::authoring_runs::storage_error;
use crate::{CreatePlanningReview, CreateTaskRequest, RepositoryError};

pub(crate) fn bind_claim(
    db: &Connection,
    request: &CreateTaskRequest,
) -> Result<(), RepositoryError> {
    let mut target: Value = serde_json::from_str(&request.target_json)
        .map_err(|_| RepositoryError::TaskBindingInvalid)?;
    let run = target["reviewRunId"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .to_owned();
    let role = target["role"]
        .as_str()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .to_owned();
    let step = format!("review-seat-{run}-{role}");
    if target["resumeStepId"] != step {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    target
        .as_object_mut()
        .ok_or(RepositoryError::TaskBindingInvalid)?
        .remove("resumeStepId");
    let expected: Option<(String, Option<String>)> = db.query_row(
        "SELECT s.target_json,seat.task_id FROM story_planning_review_runs_v2 r
         JOIN story_planning_review_seats_v2 seat ON seat.run_id=r.run_id AND seat.role=?3
         JOIN story_work_steps_v1 s ON s.id=?4 AND s.work_task_id='work-'||r.run_id AND s.current_attempt_id=?5
         JOIN story_work_attempts_v1 a ON a.step_id=s.id AND a.attempt_id=?5
         JOIN story_task_provider_budgets_v1 g ON g.task_id=a.attempt_id AND g.work_task_id=s.work_task_id AND g.settled=0
         WHERE r.run_id=?1 AND r.project_id=?2 AND r.status='open' AND r.input_revision=?6
         AND (seat.task_id IS NULL OR EXISTS(
           SELECT 1 FROM story_tasks_v2 prior JOIN story_task_provider_budgets_v1 old_grant ON old_grant.task_id=prior.task_id
           WHERE prior.task_id=seat.task_id AND prior.status='failed' AND prior.output_refs_json='[]'
           AND old_grant.work_task_id=s.work_task_id AND old_grant.settled=1
           AND NOT EXISTS(SELECT 1 FROM story_planning_review_reports_v2 report WHERE report.task_id=prior.task_id)))",
        params![run, request.project_id.as_str(), role, step, request.task_id, request.input_revision],
        |row|Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?;
    let (expected, previous_task) = expected.ok_or(RepositoryError::TaskBindingInvalid)?;
    let expected: Value =
        serde_json::from_str(&expected).map_err(|_| RepositoryError::CorruptState)?;
    if target != expected {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let changed = db.execute("UPDATE story_planning_review_seats_v2 SET task_id=?1 WHERE run_id=?2 AND role=?3 AND task_id IS ?4",
        params![request.task_id,run,role,previous_task]).map_err(storage_error)?;
    if changed != 1 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn target(run: &str, role: &str, kind: &str, revision: &str) -> Value {
    json!({"type":"artifact", "artifactKind":kind, "revisionId":revision,
        "reviewRunId":run, "role":role})
}

pub(crate) fn create(
    db: &Connection,
    request: &CreatePlanningReview<'_>,
    input_revision: i64,
) -> Result<(), RepositoryError> {
    let work_id = format!("work-{}", request.run_id);
    db.execute(
        "INSERT INTO story_work_tasks_v1(id,project_id,thread_id,goal,kind,status,input_revision)
         VALUES(?1,?2,'',?3,'direct','running',?4)",
        params![
            work_id,
            request.project_id.as_str(),
            request.author_intent,
            input_revision
        ],
    )
    .map_err(storage_error)?;
    for (ordinal, role) in request.roles.iter().enumerate() {
        let step_id = format!("review-seat-{}-{role}", request.run_id);
        let target = target(
            request.run_id,
            role,
            request.artifact_kind,
            request.revision_id,
        );
        db.execute(
            "INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,status)
             VALUES(?1,?2,?3,'reviewPlanningSeat',?4,?5,'queued')",
            params![step_id, work_id, ordinal as i64 + 1, target.to_string(), input_revision],
        ).map_err(storage_error)?;
    }
    Ok(())
}

pub(crate) enum ReviewEnd {
    Cancelled,
    Finished,
}

pub(crate) fn close(
    db: &Connection,
    run_id: &str,
    end: ReviewEnd,
    now: i64,
) -> Result<(), RepositoryError> {
    let work_id = format!("work-{run_id}");
    let (step_status, work_status) = match end {
        ReviewEnd::Cancelled => ("cancelled", "cancelled"),
        ReviewEnd::Finished => {
            let ready: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 w WHERE w.id=?1
                 AND (w.status IN ('running','completed') OR (w.status='blocked'
                   AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 s WHERE s.work_task_id=w.id AND s.status!='completed')))
                 AND NOT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 g WHERE g.work_task_id=w.id AND g.settled=0))",
                [&work_id], |row| row.get(0),
            ).map_err(storage_error)?;
            if !ready {
                return Err(RepositoryError::TaskNotWritable);
            }
            let active: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id
                 JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.status IN ('queued','running','waiting'))",
                [&work_id], |row| row.get(0),
            ).map_err(storage_error)?;
            if active {
                return Err(RepositoryError::TaskNotWritable);
            }
            // Unperformed seats are explicitly superseded, never marked successfully checked.
            ("superseded", "completed")
        }
    };
    db.execute("UPDATE story_work_steps_v1 SET status=?2 WHERE work_task_id=?1 AND status IN ('queued','running','blocked')",
        params![work_id, step_status]).map_err(storage_error)?;
    db.execute("UPDATE story_work_attempts_v1 SET completed_at=?2 WHERE step_id IN (SELECT id FROM story_work_steps_v1 WHERE work_task_id=?1) AND completed_at IS NULL",
        params![work_id, now]).map_err(storage_error)?;
    db.execute(
        "UPDATE story_work_tasks_v1 SET status=?2,version=version+1 WHERE id=?1",
        params![work_id, work_status],
    )
    .map_err(storage_error)?;
    Ok(())
}
