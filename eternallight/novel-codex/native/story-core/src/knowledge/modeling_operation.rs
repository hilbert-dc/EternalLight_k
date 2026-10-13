use rusqlite::{OptionalExtension, params};

use crate::{ModelingBatch, PublishModelingBatch, RepositoryError, SqliteStoryRepository};



fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public modeling operation capability; SQLite retains transaction ownership.
pub trait ModelingOperationRepository {
    /// Close proven publication rejections after the publication transaction has
    /// rolled back. Storage failures remain uncertain; committed receipts never
    /// become rejections. Definite domain rejections do not remain accepted.
    fn publish_modeling_batch(
        &mut self,
        request: PublishModelingBatch<'_>,
    ) -> Result<ModelingBatch, RepositoryError>;
}

impl ModelingOperationRepository for SqliteStoryRepository {
    /// Close proven publication rejections after the publication transaction has
    /// rolled back. Storage failures remain uncertain; committed receipts never
    /// become rejections. Definite domain rejections do not remain accepted.
    fn publish_modeling_batch(
        &mut self,
        request: PublishModelingBatch<'_>,
    ) -> Result<ModelingBatch, RepositoryError> {
        let project = request.project_id.clone();
        let task = request.task_id.to_owned();
        let outcome = self.publish_modeling_batch_transaction(request);
        if matches!(
            &outcome,
            Err(RepositoryError::InvalidContent(_)
                | RepositoryError::IdempotencyConflict
                | RepositoryError::CandidateNotFound
                | RepositoryError::TaskBindingInvalid
                | RepositoryError::TaskInputStale
                | RepositoryError::ReviewChecksFailed
                | RepositoryError::TaskNotWritable)
        ) && let Some(binding) = self.recovery_operation.as_ref()
        {
            let transaction = self
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(storage_error)?;
            let state: Option<String> = transaction.query_row(
                "SELECT state FROM story_tool_operations_v1 WHERE project_id=?1 AND task_id=?2 AND call_id=?3",
                params![binding.project_id.as_str(), binding.task_id, binding.call_id],
                |row| row.get(0),
            ).optional().map_err(storage_error)?;
            if state.as_deref() == Some("accepted") {
                crate::recovery_operations::reject_operation(
                    &transaction,
                    Some(binding),
                    &project,
                    &task,
                    "publish_knowledge_batch",
                )?;
            }
            transaction.commit().map_err(storage_error)?;
        }
        outcome
    }
}
