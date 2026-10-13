use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, ProviderBudget, RepositoryError};

pub(crate) fn take(state: &mut Value) -> Result<ProviderBudget, RepositoryError> {
    let value = state
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?
        .remove("providerBudget")
        .ok_or(RepositoryError::CorruptState)?;
    serde_json::from_value(value).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn load(
    db: &Connection,
    project: &ProjectId,
    work_id: &str,
) -> Result<ProviderBudget, RepositoryError> {
    let stored: String = db.query_row(
        "SELECT b.ledger_json FROM story_work_budgets_v1 b JOIN story_work_tasks_v1 t ON t.id=b.work_task_id WHERE t.id=?1 AND t.project_id=?2",
        params![work_id, project.as_str()], |row| row.get(0),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::CorruptState)?;
    serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn hydrate(
    db: &Connection,
    project: &ProjectId,
    run_id: &str,
    stored: &str,
) -> Result<String, RepositoryError> {
    let mut state: Value =
        serde_json::from_str(stored).map_err(|_| RepositoryError::CorruptState)?;
    if state.get("providerBudget").is_some() {
        return Err(RepositoryError::CorruptState);
    }
    state["providerBudget"] = serde_json::to_value(load(db, project, &format!("work-{run_id}"))?)
        .map_err(|_| RepositoryError::CorruptState)?;
    if state["status"] == "blocked" && matches!(state["blockedReason"].as_str(), Some("pendingRecovery" | "pendingUsage" | "pendingWriteAndUsage")) {
        state["blockedReason"] = crate::work_modeling_recovery::pending_reason(db, project, &format!("work-{run_id}"))?.unwrap_or("executionBlocked").into();
    }
    Ok(state.to_string())
}

/// Called inside the owning task/run write transaction, never as an independent ledger write.
pub(crate) fn update(
    db: &Connection,
    project: &ProjectId,
    work_id: &str,
    next: &ProviderBudget,
) -> Result<(), RepositoryError> {
    let previous = load(db, project, work_id)?;
    if previous.max_requests != next.max_requests
        || previous.max_tokens != next.max_tokens
        || !next.charges.starts_with(&previous.charges)
        || next.charges.len() > previous.charges.len() + 1
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    for charge in &next.charges[previous.charges.len()..] {
        let owned: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND a.attempt_id=?3 AND t.status IN ('completed','failed','cancelled'))",
            params![work_id, project.as_str(), charge.task_id], |row| row.get(0),
        ).map_err(storage_error)?;
        if !owned {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let grant_json: String = db.query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1 AND work_task_id=?2 AND settled=0",
            params![charge.task_id, work_id], |row| row.get(0),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::CorruptState)?;
        let grant: crate::TaskProviderBudget =
            serde_json::from_str(&grant_json).map_err(|_| RepositoryError::CorruptState)?;
        if grant.task_id != charge.task_id
            || grant.work_task_id != work_id
            || grant.budget_id != charge.budget_id
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let settled = db.execute(
            "UPDATE story_task_provider_budgets_v1 SET settled=1 WHERE task_id=?1 AND work_task_id=?2 AND settled=0",
            params![charge.task_id, work_id],
        ).map_err(storage_error)?;
        if settled != 1 {
            return Err(RepositoryError::CorruptState);
        }
    }
    let serialized = serde_json::to_string(next).map_err(|_| RepositoryError::CorruptState)?;
    db.execute(
        "UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",
        params![serialized, work_id],
    )
    .map_err(storage_error)?;
    Ok(())
}
