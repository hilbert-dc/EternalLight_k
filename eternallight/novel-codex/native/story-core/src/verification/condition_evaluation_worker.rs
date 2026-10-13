use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::storage;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

/// Public condition evaluation worker capability; SQLite retains transaction ownership.
pub trait ConditionEvaluationWorkerRepository {
    /// Run one persisted round-robin turn. Failed pages keep their last committed
    /// checkpoint and are retired unless a concurrent control already won.
    fn condition_evaluation_tick(&mut self) -> Result<bool, RepositoryError>;
}

impl ConditionEvaluationWorkerRepository for SqliteStoryRepository {
    /// Run one persisted round-robin turn. Failed pages keep their last committed
    /// checkpoint and are retired unless a concurrent control already won.
    fn condition_evaluation_tick(&mut self) -> Result<bool, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let selected: Option<(String,String,i64)> = tx.query_row(
            "SELECT project_id,id,version FROM story_condition_evaluations_v1 WHERE status IN ('queued','running') ORDER BY dispatch_sequence,rowid LIMIT 1",
            [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional().map_err(storage)?;
        let Some((owner, id, version)) = selected else {
            tx.commit().map_err(storage)?;
            return Ok(false);
        };
        let sequence: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(dispatch_sequence),0) FROM story_condition_evaluations_v1",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        let sequence = sequence
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        tx.execute(
            "UPDATE story_condition_evaluations_v1 SET dispatch_sequence=?1 WHERE id=?2",
            params![sequence, id],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        let outcome = ProjectId::parse(owner.clone())
            .map_err(|_| RepositoryError::CorruptState)
            .and_then(|project| self.advance_condition_evaluation(&project, &id, version));
        if outcome.is_err() {
            // Do not overwrite a successful peer page, a pause, or cancellation.
            // The failed page transaction has already rolled back at this point.
            let next = version
                .checked_add(1)
                .ok_or(RepositoryError::CorruptState)?;
            self.connection.execute("UPDATE story_condition_evaluations_v1 SET status='failed',version=?1,result_json=NULL WHERE project_id=?2 AND id=?3 AND version=?4 AND status IN ('queued','running')",params![next,owner,id,version]).map_err(storage)?;
        }
        Ok(true)
    }
}

use crate::ConditionEvaluationExecutionRepository;
