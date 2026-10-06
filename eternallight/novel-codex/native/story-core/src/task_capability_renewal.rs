use rusqlite::params;
use serde_json::Value;

use crate::tasks::{load_task, validate_active_task};
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

/// Host-issued renewal of a still-live capability; it cannot alter task authority.
pub struct RenewTaskCapability<'a> {
    pub project_id: &'a ProjectId,
    pub task_id: &'a str,
    pub thread_id: &'a str,
    pub capability_hash: &'a str,
    pub expected_expires_at: i64,
    pub operation: &'a str,
    pub target: &'a Value,
    pub model: Option<&'a str>,
    pub now: i64,
}

impl SqliteStoryRepository {
    pub fn renew_task_capability(
        &mut self,
        request: RenewTaskCapability<'_>,
    ) -> Result<TaskRecord, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let task = load_task(&tx, request.task_id)?;
        validate_active_task(
            &tx,
            &task,
            request.project_id,
            request.thread_id,
            task.input_revision,
        )?;
        // Re-read the persisted author/source/range grant, without replenishing it.
        crate::task_provider_budget::read_budget(
            &tx,
            request.project_id,
            request.task_id,
            request.now,
        )?;
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        if task.capability_revoked_at.is_some()
            || task.capability_expires_at <= request.now
            || task.capability_expires_at != request.expected_expires_at
            || task.capability_hash != request.capability_hash
            || task.operation != request.operation
            || target != *request.target
            || task.model.as_deref() != request.model
        {
            return Err(RepositoryError::TaskCapabilityInvalid);
        }
        let expires_at = request
            .now
            .checked_add(30 * 60)
            .filter(|expiry| *expiry > task.capability_expires_at)
            .ok_or(RepositoryError::TaskCapabilityInvalid)?;
        tx.execute(
            "UPDATE story_tasks_v2 SET capability_expires_at=?1 WHERE task_id=?2",
            params![expires_at, request.task_id],
        )
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let renewed = load_task(&tx, request.task_id)?;
        tx.commit()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(renewed)
    }
}
