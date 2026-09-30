use rusqlite::{Connection, OptionalExtension, params};

use crate::authoring_runs::storage_error;
use crate::{
    ProjectId, ProviderBudget, RepositoryError, SqliteStoryRepository, UsageChargeOutcome,
    WorkBudgetAuthorization,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProviderUsage {
    pub task_id: String,
    pub status: String,
}

impl SqliteStoryRepository {
    /// Returns the single unsettled ordinary attempt, including active attempts that cannot settle yet.
    pub fn pending_work_provider_usage(
        &self,
        project: &ProjectId,
        work_id: &str,
    ) -> Result<Option<PendingProviderUsage>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let kind: String = read
            .query_row(
                "SELECT kind FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2",
                params![work_id, project.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::TaskNotFound)?;
        if kind == "generation" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let pending: Option<(String,String)> = read.query_row(
            "SELECT g.task_id,t.status FROM story_task_provider_budgets_v1 g JOIN story_tasks_v2 t ON t.task_id=g.task_id WHERE g.work_task_id=?1 AND g.settled=0 AND t.project_id=?2",
            params![work_id,project.as_str()],|row|Ok((row.get(0)?,row.get(1)?)),
        ).optional().map_err(storage_error)?;
        pending
            .map(|(task, status)| {
                deadline(&read, project, work_id, &task)?.ok_or(RepositoryError::CorruptState)?;
                Ok(PendingProviderUsage {
                    task_id: task,
                    status,
                })
            })
            .transpose()
    }

    /// Records terminal usage supplied by the trusted engine receipt reader, never model input.
    pub fn charge_work_provider_usage(
        &mut self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
        receipt: &serde_json::Value,
    ) -> Result<ProviderBudget, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        deadline(&tx, project, work_id, task_id)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        if !matches!(task.status.as_str(), "completed" | "failed" | "cancelled") {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let mut budget = crate::work_budget_storage::load(&tx, project, work_id)?;
        if budget.charge_receipt(task_id, receipt)? == UsageChargeOutcome::AlreadyCharged {
            return Ok(budget);
        }
        crate::work_budget_storage::update(&tx, project, work_id, &budget)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [work_id],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(budget)
    }
}

/// Resolves the frozen authorization for an ordinary attempt after its allowance cutoff.
/// An absent allowance is not permission to allocate a shared budget.
pub(crate) fn deadline(
    db: &Connection,
    project: &ProjectId,
    work_id: &str,
    task_id: &str,
) -> Result<Option<i64>, RepositoryError> {
    let authorization: Option<(String, i64)> = db.query_row(
        "SELECT authorization_json,after_attempt_sequence FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",
        [work_id], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?;
    let Some((stored, cutoff)) = authorization else {
        let ledger_exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM story_work_budgets_v1 WHERE work_task_id=?1)",
                [work_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        return if ledger_exists {
            Err(RepositoryError::CorruptState)
        } else {
            Ok(None)
        };
    };
    let authorization: WorkBudgetAuthorization =
        serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)?;
    let eligible: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE w.id=?1 AND w.project_id=?2 AND t.project_id=?2 AND w.kind IN ('conversation','direct') AND a.attempt_id=?3 AND a.sequence>?4)",
        params![work_id,project.as_str(),task_id,cutoff], |row|row.get(0),
    ).map_err(storage_error)?;
    if !eligible {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let ledger = crate::work_budget_storage::load(db, project, work_id)?;
    if ledger.max_requests != authorization.max_requests
        || ledger.max_tokens != authorization.max_tokens
    {
        return Err(RepositoryError::CorruptState);
    }
    Ok(Some(authorization.deadline_at))
}
