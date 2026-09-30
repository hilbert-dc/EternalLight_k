use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskProviderBudget {
    pub task_id: String,
    pub work_task_id: String,
    pub budget_id: String,
    pub max_requests: u64,
    pub max_tokens: u64,
    pub deadline_at: i64,
}

pub(crate) fn reserve(
    db: &rusqlite::Connection,
    project: &ProjectId,
    work_id: &str,
    task_id: &str,
    deadline_at: i64,
) -> Result<(), RepositoryError> {
    let budget = crate::work_budget_storage::load(db, project, work_id)?;
    if budget.exhausted() || budget.has_charge(task_id) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let unsettled: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=0)",
        [work_id], |row| row.get(0),
    ).map_err(storage_error)?;
    if unsettled {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let grant = TaskProviderBudget {
        task_id: task_id.to_owned(),
        work_task_id: work_id.to_owned(),
        budget_id: format!("budget-{task_id}"),
        max_requests: budget.remaining_requests().min(64),
        max_tokens: budget.remaining_tokens().min(1_000_000),
        deadline_at,
    };
    db.execute("INSERT INTO story_task_provider_budgets_v1(task_id,work_task_id,grant_json) VALUES (?1,?2,?3)",
        params![task_id, work_id, serde_json::to_string(&grant).map_err(|_| RepositoryError::CorruptState)?],
    ).map_err(storage_error)?;
    Ok(())
}

impl SqliteStoryRepository {
    /// Claims an attempt against existing author-approved authority. The claim and
    /// full grant validation commit together; missing authority rolls back all
    /// task, step, action and generation-run mutations. New roots use
    /// `create_budgeted_root_task` instead.
    pub fn create_funded_task(
        &mut self,
        request: crate::CreateTaskRequest,
        now: i64,
    ) -> Result<crate::TaskRecord, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = crate::tasks::create_task_record(&transaction, &request)?;
        read_budget(&transaction, &request.project_id, &request.task_id, now)?;
        transaction.commit().map_err(storage_error)?;
        Ok(task)
    }

    /// Reads execution limits for an already claimed active, budgeted attempt.
    /// This does not approve a task, reserve another attempt, or replenish its budget.
    pub fn task_provider_budget(
        &self,
        project: &ProjectId,
        task_id: &str,
        now: i64,
    ) -> Result<TaskProviderBudget, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        read_budget(&read, project, task_id, now)
    }
}

pub(crate) fn read_budget(
    read: &rusqlite::Connection,
    project: &ProjectId,
    task_id: &str,
    now: i64,
) -> Result<TaskProviderBudget, RepositoryError> {
    let (work_id, kind, state_json, version): (String, String, Option<String>, Option<i64>) = read.query_row(
            "SELECT w.id,w.kind,r.state_json,r.version FROM story_tasks_v2 t
             JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id
             JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.current_attempt_id=t.task_id
             JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
             LEFT JOIN story_generation_runs_v1 r ON w.id='work-' || r.run_id AND r.project_id=w.project_id
             WHERE t.task_id=?1 AND t.project_id=?2 AND w.project_id=?2
               AND t.status IN ('queued','running','waiting') AND t.capability_revoked_at IS NULL
               AND t.capability_expires_at > ?3
               AND w.status='running' AND s.status='running'",
            params![task_id, project.as_str(), now], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let deadline = match kind.as_str() {
        "generation" => {
            let state: Value =
                serde_json::from_str(&state_json.ok_or(RepositoryError::CorruptState)?)
                    .map_err(|_| RepositoryError::CorruptState)?;
            let version = version.ok_or(RepositoryError::CorruptState)?;
            if state["projectId"] != project.as_str()
                || state["version"] != version
                || state["id"]
                    .as_str()
                    .is_none_or(|id| format!("work-{id}") != work_id)
                || state.get("providerBudget").is_some()
            {
                return Err(RepositoryError::CorruptState);
            }
            if state["status"] != "running" || state["activeTaskId"] != task_id {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            state["deadlineAt"]
                .as_i64()
                .ok_or(RepositoryError::CorruptState)?
        }
        "conversation" | "direct" => {
            if state_json.is_some() || version.is_some() {
                return Err(RepositoryError::CorruptState);
            }
            crate::ordinary_work_budget::deadline(read, project, &work_id, task_id)?
                .ok_or(RepositoryError::TaskBindingInvalid)?
        }
        _ => return Err(RepositoryError::CorruptState),
    };
    let budget = crate::work_budget_storage::load(read, project, &work_id)?;
    let task = crate::tasks::load_task(read, task_id)?;
    crate::task_input::assert_task_inputs_current(read, &task)?;
    if now >= deadline || budget.exhausted() || budget.has_charge(task_id) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let stored: String = read.query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1 AND work_task_id=?2 AND settled=0",
            params![task_id,work_id], |row| row.get(0),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::CorruptState)?;
    let grant: TaskProviderBudget =
        serde_json::from_str(&stored).map_err(|_| RepositoryError::CorruptState)?;
    if grant.task_id != task_id
        || grant.work_task_id != work_id
        || grant.budget_id != format!("budget-{task_id}")
        || grant.deadline_at != deadline
        || grant.max_requests == 0
        || grant.max_requests > budget.remaining_requests().min(64)
        || grant.max_tokens == 0
        || grant.max_tokens > budget.remaining_tokens().min(1_000_000)
    {
        return Err(RepositoryError::CorruptState);
    }
    Ok(grant)
}
