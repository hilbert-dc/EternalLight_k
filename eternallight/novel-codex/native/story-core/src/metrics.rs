use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::Value;

use crate::RepositoryError;
use crate::SqliteStoryRepository;

impl SqliteStoryRepository {
    pub fn local_metrics(&self) -> Result<Vec<Value>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT metric, count, total_seconds FROM story_local_metrics_v2 ORDER BY metric",
            )
            .map_err(storage_error)?;
        statement
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "metric": row.get::<_, String>(0)?, "count": row.get::<_, i64>(1)?,
                    "totalSeconds": row.get::<_, i64>(2)?
                }))
            })
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect()
    }

    pub fn record_local_metric(
        &mut self,
        metric: &str,
        idempotency_key: &str,
    ) -> Result<Vec<Value>, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        let previous = transaction
            .query_row(
                "SELECT metric FROM story_local_metric_receipts_v2 WHERE idempotency_key = ?1",
                [idempotency_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?;
        match previous {
            Some(previous) if previous != metric => {
                return Err(RepositoryError::IdempotencyConflict);
            }
            Some(_) => {}
            None => {
                transaction.execute("INSERT INTO story_local_metric_receipts_v2(idempotency_key, metric) VALUES (?1, ?2)", params![idempotency_key, metric]).map_err(storage_error)?;
                transaction.execute("INSERT INTO story_local_metrics_v2(metric, count, total_seconds) VALUES (?1, 1, 0) ON CONFLICT(metric) DO UPDATE SET count = count + 1", [metric]).map_err(storage_error)?;
            }
        }
        transaction.commit().map_err(storage_error)?;
        self.local_metrics()
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
