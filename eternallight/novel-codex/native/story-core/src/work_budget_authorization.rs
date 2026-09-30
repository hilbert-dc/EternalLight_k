use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, ProviderBudget, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkBudgetAuthorization {
    pub expected_version: i64,
    pub expected_project_revision: i64,
    pub expected_source_digest: String,
    pub max_requests: u64,
    pub max_tokens: u64,
    pub deadline_at: i64,
}

impl WorkBudgetAuthorization {
    pub(crate) fn validate_limits(&self, now: i64) -> Result<(), RepositoryError> {
        if self.max_requests == 0
            || self.max_requests > 1024
            || self.max_tokens == 0
            || self.max_tokens > 16_000_000
            || self.deadline_at <= now
            || self.deadline_at.saturating_sub(now) > 86_400
        {
            return Err(RepositoryError::InvalidContent(
                "invalid work budget limits".into(),
            ));
        }
        Ok(())
    }
}

impl SqliteStoryRepository {
    /// Reads an ordinary task's author confirmation without allocating or renewing it.
    pub fn work_budget_authorization(
        &self,
        project: &ProjectId,
        work_id: &str,
    ) -> Result<Option<WorkBudgetAuthorization>, RepositoryError> {
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
        crate::planning_review_author_source::validate_work(&read, project, work_id)?;
        let stored: Option<String> = read.query_row(
            "SELECT authorization_json FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",
            [work_id],|row|row.get(0),
        ).optional().map_err(storage_error)?;
        match stored {
            Some(stored) => {
                let authorization: WorkBudgetAuthorization =
                    serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)?;
                let ledger = crate::work_budget_storage::load(&read, project, work_id)?;
                if ledger.max_requests != authorization.max_requests
                    || ledger.max_tokens != authorization.max_tokens
                {
                    return Err(RepositoryError::CorruptState);
                }
                Ok(Some(authorization))
            }
            None => {
                let exists: bool = read
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM story_work_budgets_v1 WHERE work_task_id=?1)",
                        [work_id],
                        |row| row.get(0),
                    )
                    .map_err(storage_error)?;
                if exists {
                    Err(RepositoryError::CorruptState)
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Authorizes additional work after the existing attempts have terminated.
    /// Existing attempts are outside this new allowance; no execution is started here.
    pub fn authorize_work_budget(
        &mut self,
        project: &ProjectId,
        work_id: &str,
        authorization: &WorkBudgetAuthorization,
        now: i64,
    ) -> Result<ProviderBudget, RepositoryError> {
        authorization.validate_limits(now)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (kind, status, version): (String, String, i64) = tx
            .query_row(
                "SELECT kind,status,version FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2",
                params![work_id, project.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or(RepositoryError::TaskNotFound)?;
        if kind == "generation" || status == "cancelled" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::planning_review_author_source::validate_work(&tx, project, work_id)?;
        let existing: Option<String> = tx.query_row(
            "SELECT authorization_json FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",
            [work_id], |row|row.get(0),
        ).optional().map_err(storage_error)?;
        if let Some(existing) = existing {
            let prior: WorkBudgetAuthorization =
                serde_json::from_str(&existing).map_err(|_| RepositoryError::CorruptState)?;
            if prior != *authorization {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return crate::work_budget_storage::load(&tx, project, work_id);
        }
        if version != authorization.expected_version {
            return Err(RepositoryError::WorkTaskStale);
        }
        let revision: i64 = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
                [project.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if revision != authorization.expected_project_revision
            || crate::task_input::project_source_digest(&tx, project)?
                != authorization.expected_source_digest
        {
            return Err(RepositoryError::TaskInputStale);
        }
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE s.work_task_id=?1 AND t.status IN ('queued','running','waiting'))",
            [work_id], |row|row.get(0),
        ).map_err(storage_error)?;
        if active {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let cutoff: i64 = tx.query_row(
            "SELECT COALESCE(MAX(a.sequence),0) FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1",
            [work_id], |row|row.get(0),
        ).map_err(storage_error)?;
        let budget = ProviderBudget {
            max_requests: authorization.max_requests,
            max_tokens: authorization.max_tokens,
            used_requests: 0,
            used_tokens: 0,
            unknown_tokens: 0,
            overrun: false,
            charges: Vec::new(),
        };
        tx.execute(
            "INSERT INTO story_work_budgets_v1(work_task_id,ledger_json) VALUES (?1,?2)",
            params![
                work_id,
                serde_json::to_string(&budget).map_err(|_| RepositoryError::CorruptState)?
            ],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO story_work_budget_authorizations_v1(work_task_id,authorization_json,after_attempt_sequence,granted_at) VALUES (?1,?2,?3,?4)",
            params![work_id,serde_json::to_string(authorization).map_err(|_|RepositoryError::CorruptState)?,cutoff,now]).map_err(storage_error)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [work_id],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(budget)
    }
}
