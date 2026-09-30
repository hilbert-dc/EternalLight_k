use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

use novel_codex_story_core::{
    BranchId, CandidateReceipt, CommitId, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;

fn project() -> ProjectId {
    ProjectId::parse("concurrent-project").unwrap()
}

fn setup(path: &std::path::Path) -> SqliteStoryRepository {
    let mut repository = SqliteStoryRepository::open(path).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project(),
                title: "并发候选".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    repository
}

fn candidate(suffix: &str) -> SaveCandidateRequest {
    SaveCandidateRequest {
        project_id: project(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("artifact-brief").unwrap(),
        object_kind: "brief".into(),
        revision_id: RevisionId::parse(format!("revision-{suffix}")).unwrap(),
        commit_id: CommitId::parse(format!("commit-{suffix}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{suffix}")).unwrap(),
        request_digest: format!("request-{suffix}"),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}")).unwrap(),
        expected_candidate_revision_id: None,
        content_digest: format!("content-{suffix}"),
        content_json: serde_json::json!({"premise":suffix}).to_string(),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: Vec::new(),
    }
}

#[test]
fn snapshot_reader_does_not_block_candidate_commit_and_reopen_sees_the_committed_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.sqlite3");
    let mut writer = setup(&path);
    let reader = SqliteStoryRepository::open(&path).unwrap();
    let before = reader
        .branch_head(&project(), &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap();
    let receipt = reader
        .with_read_snapshot(|| {
            assert_eq!(
                reader
                    .branch_head(&project(), &BranchId::parse("main").unwrap())
                    .unwrap()
                    .unwrap(),
                before
            );
            let receipt = writer.save_candidate(candidate("new"));
            assert_eq!(
                reader
                    .branch_head(&project(), &BranchId::parse("main").unwrap())
                    .unwrap()
                    .unwrap(),
                before
            );
            assert_eq!(reader.object_heads(&project()).unwrap(), Vec::new());
            receipt
        })
        .unwrap()
        .unwrap();
    drop(writer);
    let reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened
            .branch_head(&project(), &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        receipt.result_snapshot_id
    );
    assert_eq!(
        reader.object_heads(&project()).unwrap(),
        reopened.object_heads(&project()).unwrap()
    );
    assert_eq!(
        reopened.revision_content(&receipt.revision_id).unwrap(),
        Some(candidate("new").content_json)
    );
    assert_eq!(reopened.project_revision(&project()).unwrap(), 0);
}

#[test]
fn candidate_waits_for_the_writer_before_reading_and_retains_normal_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reserved.sqlite3");
    let mut repository = setup(&path);
    let blocker = rusqlite::Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        result_tx
            .send(repository.save_candidate(candidate("reserved")))
            .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let early = result_rx.recv_timeout(Duration::from_millis(150));
    blocker.execute_batch("COMMIT").unwrap();
    worker.join().unwrap();
    assert!(
        matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
        "{early:?}"
    );
    let receipt = result_rx.recv().unwrap().unwrap();
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened.save_candidate(candidate("reserved")).unwrap(),
        receipt
    );
    assert_eq!(reopened.object_heads(&project()).unwrap().len(), 1);
}

#[test]
fn competing_candidates_revalidate_source_after_writer_admission_and_only_one_can_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("competing.sqlite3");
    let first = setup(&path);
    let second = SqliteStoryRepository::open(&path).unwrap();
    let start = Arc::new(Barrier::new(2));
    let workers = [(first, "first"), (second, "second")]
        .into_iter()
        .map(|(mut repository, suffix)| {
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                repository.save_candidate(candidate(suffix))
            })
        })
        .collect::<Vec<_>>();
    let mut results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    let receipt: CandidateReceipt = results
        .remove(results.iter().position(Result::is_ok).unwrap())
        .unwrap();
    assert!(
        matches!(&results[0], Err(RepositoryError::StaleSnapshot { .. })),
        "{results:?}"
    );
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository
            .branch_head(&project(), &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        receipt.result_snapshot_id
    );
    assert_eq!(
        repository.object_heads(&project()).unwrap()[0]
            .candidate_revision_id
            .as_ref(),
        Some(&receipt.revision_id)
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_revisions_v2),(SELECT COUNT(*) FROM story_candidate_receipts_v2)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).unwrap(),(1,1));
}
