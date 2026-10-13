use rusqlite::{Connection, OptionalExtension, params};

use crate::authoring_runs::storage_error;
use crate::{
    ProjectId, ProviderBudget, RepositoryError, SqliteStoryRepository, UsageChargeOutcome,
    WorkBudgetAuthorization,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PendingProviderUsage {
    pub task_id: String,
    pub status: String,
}



impl crate::SqliteStoryRepository {
    /// Replaces an existing charge only through the Engine's reconciliation proof.
    pub fn reconcile_charged_provider_usage(&mut self, project: &ProjectId, work_id: &str, task_id: &str, receipt: &serde_json::Value) -> Result<ProviderBudget, RepositoryError> {
        let tx = self.connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(storage_error)?;
        let mut budget = crate::work_budget_storage::load(&tx, project, work_id)?;
        if !budget.has_charge(task_id) { return Err(RepositoryError::TaskBindingInvalid); }
        if budget.charge_receipt(task_id, receipt)? == UsageChargeOutcome::AlreadyCharged { return Ok(budget); }
        let encoded = serde_json::to_string(&budget).map_err(|_| RepositoryError::CorruptState)?;
        tx.execute("UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",params![encoded,work_id]).map_err(storage_error)?;
        tx.execute("UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",[work_id]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(budget)
    }
}

/// Resolves the frozen authorization for an ordinary attempt after its allowance cutoff.
/// An absent allowance is not permission to allocate a shared budget.
pub(crate) fn authorization(
    db: &Connection,
    project: &ProjectId,
    work_id: &str,
    task_id: &str,
) -> Result<Option<WorkBudgetAuthorization>, RepositoryError> {
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
    Ok(Some(authorization))
}

/// Public ordinary work budget capability; SQLite retains transaction ownership.
pub trait OrdinaryWorkBudgetRepository {
    /// A bound message must commit `sending` before Host can open its run.
    /// This stronger durable fence also covers a first run whose Engine file
    /// was never created. Missing storage alone is not evidence of zero usage.
    fn undispatched_message_usage_receipt(
        &self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
    ) -> Result<serde_json::Value, RepositoryError>;
    /// Trusted Runtime may use this only after the configured engine ledger
    /// proves that no budget exists. Network requests require that ledger first.
    fn unstarted_work_usage_receipt(
        &self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
    ) -> Result<serde_json::Value, RepositoryError>;
    /// Returns the single unsettled ordinary attempt, including active attempts that cannot settle yet.
    fn pending_work_provider_usage(
        &self,
        project: &ProjectId,
        work_id: &str,
    ) -> Result<Option<PendingProviderUsage>, RepositoryError>;
    /// Records terminal usage supplied by the trusted engine receipt reader, never model input.
    fn charge_work_provider_usage(
        &mut self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
        receipt: &serde_json::Value,
    ) -> Result<ProviderBudget, RepositoryError>;
}

impl OrdinaryWorkBudgetRepository for SqliteStoryRepository {
    /// A bound message must commit `sending` before Host can open its run.
    /// This stronger durable fence also covers a first run whose Engine file
    /// was never created. Missing storage alone is not evidence of zero usage.
    fn undispatched_message_usage_receipt(
        &self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
    ) -> Result<serde_json::Value, RepositoryError> {
        let fenced: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_conversation_messages_v1 m JOIN story_tasks_v2 t ON t.task_id=m.task_id
             JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id
             WHERE m.task_id=?1 AND m.project_id=?2 AND t.project_id=m.project_id AND t.thread_id=m.thread_id
               AND s.work_task_id=?3 AND m.status='claimed' AND m.turn_id IS NULL
               AND t.status IN ('failed','cancelled') AND t.capability_revoked_at IS NOT NULL)",
            params![task_id,project.as_str(),work_id], |r|r.get(0),
        ).map_err(storage_error)?;
        if !fenced {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.unstarted_work_usage_receipt(project, work_id, task_id)
    }

    /// Trusted Runtime may use this only after the configured engine ledger
    /// proves that no budget exists. Network requests require that ledger first.
    fn unstarted_work_usage_receipt(
        &self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
    ) -> Result<serde_json::Value, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let generation: Option<String> = read.query_row(
            "SELECT g.state_json FROM story_generation_runs_v1 g JOIN story_work_tasks_v1 w ON w.id='work-'||g.run_id WHERE w.id=?1 AND w.project_id=?2 AND g.project_id=?2 AND w.kind='generation'",
            params![work_id,project.as_str()], |row| row.get(0),
        ).optional().map_err(storage_error)?;
        let deadline = if let Some(state) = generation {
            let state: serde_json::Value =
                serde_json::from_str(&state).map_err(|_| RepositoryError::CorruptState)?;
            if state["deadlineAt"].is_null() {
                None
            } else {
                Some(
                    state["deadlineAt"]
                        .as_i64()
                        .ok_or(RepositoryError::CorruptState)?,
                )
            }
        } else if crate::work_interludes::is_attempt(&read, task_id)? {
            crate::work_interludes::deadline(&read, project, work_id, task_id)?
        } else {
            authorization(&read, project, work_id, task_id)?
                .ok_or(RepositoryError::TaskBindingInvalid)?
                .deadline_at
        };
        let eligible:bool=read.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_tasks_v2 t JOIN story_task_provider_budgets_v1 b ON b.task_id=t.task_id
             WHERE t.task_id=?1 AND t.project_id=?2 AND b.work_task_id=?3
               AND t.status IN ('failed','cancelled') AND t.turn_id IS NULL AND t.output_refs_json='[]'
               AND NOT EXISTS(SELECT 1 FROM story_task_outputs_v2 WHERE task_id=t.task_id)
               AND NOT EXISTS(SELECT 1 FROM story_tool_operations_v1 WHERE task_id=t.task_id))",
            params![task_id,project.as_str(),work_id], |r|r.get(0),
        ).map_err(storage_error)?;
        if !eligible {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let stored: String = read
            .query_row(
                "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
                [task_id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let grant: crate::TaskProviderBudget =
            serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)?;
        let budget = crate::work_budget_storage::load(&read, project, work_id)?;
        if grant.task_id != task_id
            || grant.work_task_id != work_id
            || grant.budget_id != format!("budget-{task_id}")
            || grant.deadline_at != deadline
            || (!budget.has_charge(task_id)
                && (grant.max_requests != budget.remaining_requests()
                    || grant.max_tokens != budget.remaining_tokens()))
            || grant.max_requests == Some(0)
            || grant.max_tokens == Some(0)
        {
            return Err(RepositoryError::CorruptState);
        }
        let task = crate::tasks::load_task(&read, task_id)?;
        Ok(
            serde_json::json!({"runId":task_id,"budgetId":grant.budget_id,"status":task.status,
            "authority":"nativeUnstartedAttempt","limits":{"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"maxDurationMs":null},
            "usage":{"requests":0,"tokens":0,"unknownTokens":0,"overrun":false}}),
        )
    }

    /// Returns the single unsettled ordinary attempt, including active attempts that cannot settle yet.
    fn pending_work_provider_usage(
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
        let mut pending: Option<(String,String)> = read.query_row(
            "SELECT g.task_id,t.status FROM story_task_provider_budgets_v1 g JOIN story_tasks_v2 t ON t.task_id=g.task_id WHERE g.work_task_id=?1 AND g.settled=0 AND t.project_id=?2",
            params![work_id,project.as_str()],|row|Ok((row.get(0)?,row.get(1)?)),
        ).optional().map_err(storage_error)?;
        if pending.is_none() {
            let budget = crate::work_budget_storage::load(&read, project, work_id)?;
            if let Some(charge) = budget
                .charges
                .iter()
                .find(|charge| charge.unknown_tokens > 0)
            {
                pending = Some((
                    charge.task_id.clone(),
                    crate::tasks::load_task(&read, &charge.task_id)?.status,
                ));
            }
        }
        pending
            .map(|(task, status)| {
                authorization(&read, project, work_id, &task)?
                    .ok_or(RepositoryError::CorruptState)?;
                Ok(PendingProviderUsage {
                    task_id: task,
                    status,
                })
            })
            .transpose()
    }

    /// Records terminal usage supplied by the trusted engine receipt reader, never model input.
    fn charge_work_provider_usage(
        &mut self,
        project: &ProjectId,
        work_id: &str,
        task_id: &str,
        receipt: &serde_json::Value,
    ) -> Result<ProviderBudget, RepositoryError> {
        if crate::work_budget_storage::load(&self.connection, project, work_id)?.has_charge(task_id)
        {
            return self.reconcile_charged_provider_usage(project, work_id, task_id, receipt);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if crate::work_interludes::is_attempt(&tx, task_id)? {
            crate::work_interludes::deadline(&tx, project, work_id, task_id)?;
        } else {
            authorization(&tx, project, work_id, task_id)?
                .ok_or(RepositoryError::TaskBindingInvalid)?;
        }
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
