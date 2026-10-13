use crate::{RepositoryError, SqliteStoryRepository};



#[cfg(test)]
#[path = "read_snapshot_tests.rs"]
mod tests;

/// Shares one snapshot across related reads, retaining an existing transaction.
pub(crate) fn with_connection_snapshot<T>(
    connection: &rusqlite::Connection,
    read: impl FnOnce() -> T,
) -> Result<T, RepositoryError> {
    if !connection.is_autocommit() {
        return Ok(read());
    }
    let snapshot = connection
        .unchecked_transaction()
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let result = read();
    snapshot
        .commit()
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    Ok(result)
}

/// Public read snapshot capability; SQLite retains transaction ownership.
pub trait ReadSnapshotRepository {
    /// Executes related authorization and retrieval reads against one SQLite snapshot.
    /// The closure must be read-only; writes belong to explicit domain transactions.
    fn with_read_snapshot<T>(&self, read: impl FnOnce() -> T) -> Result<T, RepositoryError>;
}

impl ReadSnapshotRepository for SqliteStoryRepository {
    /// Executes related authorization and retrieval reads against one SQLite snapshot.
    /// The closure must be read-only; writes belong to explicit domain transactions.
    fn with_read_snapshot<T>(&self, read: impl FnOnce() -> T) -> Result<T, RepositoryError> {
        with_connection_snapshot(&self.connection, read)
    }
}
