//! All synchronous domain work runs outside Tokio workers, with admission before spawn.
use crate::server::{RuntimeState, error};
use axum::{http::StatusCode, response::Response};

pub(crate) async fn with_repo(
    state: RuntimeState,
    action: impl FnOnce(novel_codex_story_core::SqliteStoryRepository) -> Response + Send + 'static,
) -> Response {
    let database = state.database.clone();
    run(state, move || match database.repository() {
        Ok(repository) => action(repository),
        Err(source) => crate::server::repository_error(source),
    })
    .await
}

pub(crate) async fn run(
    state: RuntimeState,
    action: impl FnOnce() -> Response + Send + 'static,
) -> Response {
    match execute(state, action).await {
        Ok(response) => response,
        Err(response) => response,
    }
}

pub(crate) async fn execute<T: Send + 'static>(
    state: RuntimeState,
    action: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Response> {
    let permit = match state.database_slots.clone().acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_unavailable",
                "Database executor is shutting down",
            ));
        }
    };
    // Keep admission until the actual operation ends, even if the HTTP future
    // is dropped. A client timeout is not evidence that a transaction rolled back.
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        action()
    })
    .await
    {
        Ok(response) => Ok(response),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database_execution_failed",
            "Database operation did not return a result; reconcile using the original request identity",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novel_codex_story_core::{BranchId, ProjectId, ProjectRecord, SnapshotId};
    use std::time::{Duration, Instant};

    fn state() -> (tempfile::TempDir, RuntimeState) {
        let dir = tempfile::tempdir().unwrap();
        let state = RuntimeState::new(
            dir.path().join("story.sqlite3"),
            "author-token-12345678901234567890".into(),
            "writer-token-12345678901234567890".into(),
        )
        .unwrap();
        (dir, state)
    }
    fn project() -> ProjectRecord {
        ProjectRecord {
            project_id: ProjectId::parse("project").unwrap(),
            title: "title".into(),
            logline: "".into(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sqlite_lock_wait_does_not_block_health_or_async_timers() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let (dir, state) = state();
        let lock = rusqlite::Connection::open(dir.path().join("story.sqlite3")).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let writer = state.clone();
        let (started, waiting) = tokio::sync::oneshot::channel();
        let start = Instant::now();
        let operation = tokio::spawn(execute(state.clone(), move || {
            let mut repository = writer.database.repository().unwrap();
            started.send(()).unwrap();
            repository.create_project(
                project(),
                BranchId::main(),
                SnapshotId::parse("snapshot").unwrap(),
            )
        }));
        waiting.await.unwrap();
        let health = crate::router(state)
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(health.status().is_success());
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "SQLite blocked the async executor"
        );
        lock.execute_batch("ROLLBACK").unwrap();
        operation.await.unwrap().unwrap().unwrap();
    }

    #[tokio::test]
    async fn cancelled_request_keeps_admission_until_write_finishes() {
        let (_dir, state) = state();
        let writer = state.clone();
        let (started, waiting) = tokio::sync::oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let operation = tokio::spawn(execute(state.clone(), move || {
            started.send(()).unwrap();
            blocked.recv().unwrap();
            let result = writer.database.repository().unwrap().create_project(
                project(),
                BranchId::main(),
                SnapshotId::parse("snapshot").unwrap(),
            );
            finished.send(()).unwrap();
            result
        }));
        waiting.await.unwrap();
        operation.abort();
        assert!(operation.await.unwrap_err().is_cancelled());
        assert_eq!(
            state.database_slots.available_permits(),
            state.database.capacity() - 1
        );
        release.send(()).unwrap();
        done.await.unwrap();
        let reader = state.clone();
        let projects = execute(state, move || {
            reader.database.repository().unwrap().list_projects()
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            projects.len(),
            1,
            "cancellation must not pretend the write was rolled back"
        );
    }
    #[tokio::test]
    async fn cancelled_request_waiting_for_admission_does_not_execute() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (_dir, mut state) = state();
        state.database_slots = Arc::new(tokio::sync::Semaphore::new(1));
        let held = state.database_slots.clone().acquire_owned().await.unwrap();
        let executed = Arc::new(AtomicBool::new(false));
        let observed = executed.clone();
        let operation = tokio::spawn(execute(state, move || {
            observed.store(true, Ordering::SeqCst)
        }));
        tokio::task::yield_now().await;
        operation.abort();
        assert!(operation.await.unwrap_err().is_cancelled());
        drop(held);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!executed.load(Ordering::SeqCst));
    }

use novel_codex_story_core::{ProjectRevisionRepository};
}
