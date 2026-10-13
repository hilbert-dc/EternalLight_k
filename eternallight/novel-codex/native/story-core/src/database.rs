//! Bounded ownership: only connections, never call identities, are reused.
use crate::{RepositoryError, SqliteStoryRepository, authoring_runs::storage_error};
use rusqlite::{Connection, OpenFlags};
use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

pub(crate) fn configure(db: &Connection) -> Result<(), RepositoryError> {
    db.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000; PRAGMA synchronous = FULL;",
    )
    .map_err(storage_error)
}

#[derive(Clone)]
pub struct StoryDatabase {
    pool: Arc<Pool>,
}
struct Pool {
    path: PathBuf,
    capacity: usize,
    state: Mutex<PoolState>,
    available: Condvar,
}
struct PoolState {
    idle: Vec<Connection>,
    opened: usize,
}

impl StoryDatabase {
    /// Initialize and validate before serving requests or starting workers.
    /// Capacity controls resources; it never limits the author's work or transactions.
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self, RepositoryError> {
        if capacity == 0 {
            return Err(RepositoryError::Storage(
                "database capacity must be positive".into(),
            ));
        }
        let mut repository = SqliteStoryRepository::open(path.as_ref())?;
        let connection = repository
            .connection
            .inner
            .take()
            .expect("new repository owns a connection");
        Ok(Self {
            pool: Arc::new(Pool {
                path: path.as_ref().to_owned(),
                capacity,
                state: Mutex::new(PoolState {
                    idle: vec![connection],
                    opened: 1,
                }),
                available: Condvar::new(),
            }),
        })
    }

    pub fn capacity(&self) -> usize {
        self.pool.capacity
    }

    /// Blocking checkout: async callers must use a blocking execution boundary.
    pub fn repository(&self) -> Result<SqliteStoryRepository, RepositoryError> {
        let mut state = self.pool.state.lock().map_err(|_| poisoned())?;
        let connection = loop {
            if let Some(connection) = state.idle.pop() {
                break connection;
            }
            if state.opened < self.pool.capacity {
                state.opened += 1;
                drop(state);
                let connection = Connection::open_with_flags(
                    &self.pool.path,
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )
                .map_err(storage_error)
                .and_then(|db| {
                    configure(&db)?;
                    Ok(db)
                });
                match connection {
                    Ok(db) => break db,
                    Err(error) => {
                        let mut state = self.pool.state.lock().map_err(|_| poisoned())?;
                        state.opened -= 1;
                        self.pool.available.notify_one();
                        return Err(error);
                    }
                }
            }
            state = self.pool.available.wait(state).map_err(|_| poisoned())?;
        };
        Ok(SqliteStoryRepository::with_connection(ConnectionLease {
            inner: Some(connection),
            pool: Some(self.pool.clone()),
        }))
    }
}

fn poisoned() -> RepositoryError {
    RepositoryError::Storage("database pool is unavailable".into())
}

pub(crate) struct ConnectionLease {
    inner: Option<Connection>,
    pool: Option<Arc<Pool>>,
}
impl From<Connection> for ConnectionLease {
    fn from(connection: Connection) -> Self {
        Self {
            inner: Some(connection),
            pool: None,
        }
    }
}
impl Deref for ConnectionLease {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.inner.as_ref().expect("connection exists until drop")
    }
}
impl DerefMut for ConnectionLease {
    fn deref_mut(&mut self) -> &mut Connection {
        self.inner.as_mut().expect("connection exists until drop")
    }
}
impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let (Some(connection), Some(pool)) = (self.inner.take(), self.pool.as_ref()) else {
            return;
        };
        let reusable = connection.is_autocommit() || connection.execute_batch("ROLLBACK").is_ok();
        if let Ok(mut state) = pool.state.lock() {
            if reusable {
                state.idle.push(connection);
            } else {
                state.opened -= 1;
            }
            pool.available.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_reuses_connection_but_not_call_context() {
        let dir = tempfile::tempdir().unwrap();
        let database = StoryDatabase::open(dir.path().join("story.sqlite3"), 1).unwrap();
        {
            let mut repo = database.repository().unwrap();
            repo.connection.execute_batch("CREATE TEMP TABLE connection_marker(value TEXT); INSERT INTO connection_marker VALUES ('same connection')").unwrap();
            repo.generation_work_control_guard = Some(("previous-work".into(), 7));
            repo.recovery_operation = Some(crate::RecoveryOperationBinding {
                project_id: crate::ProjectId::parse("previous-project").unwrap(),
                task_id: "previous-task".into(),
                thread_id: "previous-thread".into(),
                turn_id: "previous-turn".into(),
                call_id: "previous-call".into(),
                tool_name: "save_draft_checkpoint".into(),
                request_hash: "previous-hash".into(),
                idempotency_ref: Some("previous-key".into()),
                now: 1,
            });
        }
        let repo = database.repository().unwrap();
        assert_eq!(
            repo.connection
                .query_row::<String, _, _>("SELECT value FROM connection_marker", [], |r| r.get(0))
                .unwrap(),
            "same connection"
        );
        assert!(repo.generation_work_control_guard.is_none());
        assert!(repo.recovery_operation.is_none());
        assert_eq!(
            repo.connection
                .query_row::<i64, _, _>("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn uncommitted_changes_are_rolled_back_before_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let database = StoryDatabase::open(dir.path().join("story.sqlite3"), 1).unwrap();
        {
            let repo = database.repository().unwrap();
            repo.connection
                .execute_batch(
                    "BEGIN; INSERT INTO story_projects_v2 VALUES('uncommitted','title','',0,0)",
                )
                .unwrap();
        }
        let repo = database.repository().unwrap();
        assert!(repo.connection.is_autocommit());
        assert_eq!(
            repo.connection
                .query_row::<i64, _, _>("SELECT COUNT(*) FROM story_projects_v2", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn checkout_does_not_reinitialize_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("story.sqlite3");
        let database = StoryDatabase::open(&path, 2).unwrap();
        let first = database.repository().unwrap();
        // Remove an index only in this isolated fixture, then force a fresh connection.
        first
            .connection
            .execute_batch("DROP INDEX story_model_dependencies_source_v1")
            .unwrap();
        let second = database.repository().unwrap();
        assert_eq!(second.connection.query_row::<i64, _, _>("SELECT COUNT(*) FROM sqlite_schema WHERE name='story_model_dependencies_source_v1'", [], |r| r.get(0)).unwrap(), 0);
    }
}
