use crate::{RepositoryError, SqliteStoryRepository};

impl SqliteStoryRepository {
    /// Executes related authorization and retrieval reads against one SQLite snapshot.
    /// The closure must be read-only; writes belong to explicit domain transactions.
    pub fn with_read_snapshot<T>(&self, read: impl FnOnce() -> T) -> Result<T, RepositoryError> {
        if !self.connection.is_autocommit() { return Ok(read()); }
        let snapshot = self
            .connection
            .unchecked_transaction()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let result = read();
        snapshot
            .commit()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "read_snapshot_tests.rs"]
mod tests;
