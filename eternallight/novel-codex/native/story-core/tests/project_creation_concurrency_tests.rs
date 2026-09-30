use novel_codex_story_core::{
    BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn project_creation_waits_for_an_existing_writer_before_reading() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let expected = ProjectRecord {
        project_id: ProjectId::parse("project-concurrent").unwrap(),
        title: "Concurrent creation".into(),
        logline: String::new(),
        created_at: 1,
        updated_at: 1,
    };
    let input = expected.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        result_tx
            .send(repository.create_project(
                input,
                BranchId::parse("main").unwrap(),
                SnapshotId::parse("snapshot-initial").unwrap(),
            ))
            .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let early = result_rx.recv_timeout(Duration::from_millis(150));
    // Release the competing writer even when the regression returns an early error.
    writer.execute_batch("COMMIT").unwrap();
    worker.join().unwrap();
    assert!(
        matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
        "{early:?}"
    );
    assert_eq!(result_rx.recv().unwrap().unwrap(), expected);
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository.project_revision(&expected.project_id).unwrap(),
        0
    );
}
