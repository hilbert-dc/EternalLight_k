use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn project() -> ProjectId {
    ProjectId::parse("reading-project").unwrap()
}

fn setup(path: &std::path::Path) -> SqliteStoryRepository {
    let mut repo = SqliteStoryRepository::open(path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "阅读恢复".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    save_source(&mut repo, 0);
    repo
}

fn source(generation: usize) -> String {
    format!("{}🙂\0渡口封闭。{generation}", "潮声".repeat(160))
}

fn save_source(repo: &mut SqliteStoryRepository, generation: usize) {
    let branch = BranchId::parse("main").unwrap();
    let head = repo
        .branch_head(&project(), &branch)
        .unwrap()
        .unwrap()
        .snapshot_id;
    let content = json!({"chapterId":"chapter-1","body":source(generation)}).to_string();
    let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
    repo.save_candidate(SaveCandidateRequest {
        project_id: project(),
        branch_id: branch,
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(format!("draft-{generation}")).unwrap(),
        commit_id: CommitId::parse(format!("commit-{generation}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{generation}")).unwrap(),
        request_digest: digest.clone(),
        content_digest: digest,
        content_json: content,
        expected_project_revision: 0,
        expected_snapshot_id: head,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{generation}")).unwrap(),
        expected_candidate_revision_id: generation
            .checked_sub(1)
            .map(|previous| RevisionId::parse(format!("draft-{previous}")).unwrap()),
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: 1,
            title: "渡口".into(),
        }),
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
}

fn task(repo: &mut SqliteStoryRepository, id: &str, target: Value) -> TaskRecord {
    repo.create_task(CreateTaskRequest {
        task_id: id.into(),
        project_id: project(),
        thread_id: format!("thread-{id}"),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":"分析行程"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 1000,
    })
    .unwrap()
}

fn plan() -> StoryQueryPlan {
    serde_json::from_value(
        json!({"question":"为什么折返？","byteBudget":9000,"ranking":{"type":"literal"},
    "steps":[{"id":"read","operation":{"type":"passages","cursor":null,"maxChars":80,"limit":2}}]}),
    )
    .unwrap()
}

fn request(
    repo: &SqliteStoryRepository,
    task: &TaskRecord,
    plan: StoryQueryPlan,
    version: i64,
) -> CheckpointReadingRequest {
    let report = repo.query_task_plan(task, &plan, 10).unwrap();
    CheckpointReadingRequest {
        plan,
        report_hash: report["contentHash"].as_str().unwrap().into(),
        expected_version: version,
        summary: "页内原文已读，解释仍是候选".into(),
    }
}

fn resume(repo: &mut SqliteStoryRepository, old: &TaskRecord, id: &str) -> TaskRecord {
    let work = repo
        .work_task_for_attempt(&project(), &old.task_id)
        .unwrap();
    repo.control_work_task(&project(), &work.id, "pause", work.version, 12)
        .unwrap();
    assert!(repo.reading_progress(old, 13).is_err());
    repo.update_task(
        &project(),
        &old.task_id,
        &old.thread_id,
        TaskUpdate {
            status: "failed".into(),
            error: Some("执行已中断，未产生小说产物".into()),
            result_summary: None,
            now: 13,
        },
    )
    .unwrap();
    let paused = repo.work_task(&project(), &work.id).unwrap();
    repo.control_work_task(&project(), &work.id, "resume", paused.version, 14)
        .unwrap();
    task(
        repo,
        id,
        json!({"type":"project","resumeStepId":work.steps[0].id}),
    )
}

#[test]
fn progress_survives_reopen_and_resume_without_old_credentials_or_novel_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reading.sqlite3");
    let mut repo = setup(&path);
    let first = task(&mut repo, "first", json!({"type":"project"}));
    let formal = repo
        .branch_head(&project(), &BranchId::parse("main").unwrap())
        .unwrap();
    assert_eq!(
        repo.reading_progress(&first, 10).unwrap(),
        json!({"checkpoint":null,"semanticCompleteness":"unknown"})
    );
    let request = request(&repo, &first, plan(), 0);
    let saved = repo.checkpoint_reading(&first, &request, 10).unwrap();
    assert_eq!(
        repo.checkpoint_reading(&first, &request, 11).unwrap(),
        saved
    );
    assert_eq!(
        repo.branch_head(&project(), &BranchId::parse("main").unwrap())
            .unwrap(),
        formal
    );
    assert_eq!(
        repo.task(&project(), "first")
            .unwrap()
            .unwrap()
            .output_refs_json,
        "[]"
    );
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(repo.reading_progress(&first, 11).unwrap(), saved);
    let second = resume(&mut repo, &first, "second");
    let recovered = repo.reading_progress(&second, 15).unwrap();
    assert_ne!(
        recovered["checkpoint"]["resumePlan"],
        saved["checkpoint"]["resumePlan"]
    );
    let mut normalized = recovered.clone();
    normalized["checkpoint"]["resumePlan"] = saved["checkpoint"]["resumePlan"].clone();
    assert_eq!(normalized, saved);
    assert!(repo.checkpoint_reading(&first, &request, 15).is_err());
    let mut progress = recovered;
    for _ in 0..30 {
        let Some(next) = progress["checkpoint"]["resumePlan"].as_object() else {
            break;
        };
        let next: StoryQueryPlan = serde_json::from_value(json!(next)).unwrap();
        let request = crate::request(
            &repo,
            &second,
            next,
            progress["checkpoint"]["version"].as_i64().unwrap(),
        );
        progress = repo.checkpoint_reading(&second, &request, 16).unwrap();
    }
    assert_eq!(
        progress["checkpoint"]["acknowledgedTraversalComplete"],
        true
    );
    assert_eq!(progress["checkpoint"]["resumePlan"], Value::Null);
    assert_eq!(
        progress["checkpoint"]["acknowledgedCharacters"],
        source(0).chars().count()
    );
    assert_eq!(progress["checkpoint"]["modelJudgmentVerified"], false);
    assert_eq!(progress["semanticCompleteness"], "unknown");
    let completed = repo
        .complete_task(
            &project(),
            "second",
            "thread-second",
            "仅完成本轮讨论".into(), None,
            20,
        )
        .unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(
        repo.work_task_for_attempt(&project(), "second")
            .unwrap()
            .steps[0]
            .reading_progress,
        Some(AuthorReadingProgress::Current {
            version: progress["checkpoint"]["version"].as_i64().unwrap(),
            pages: progress["checkpoint"]["pages"].as_i64().unwrap(),
            acknowledged_characters: source(0).chars().count(),
            traversal: ReadingTraversal::AcknowledgedEnd
        })
    );
    assert_eq!(
        repo.task(&project(), "second")
            .unwrap()
            .unwrap()
            .output_refs_json,
        "[]"
    );
    assert_eq!(
        repo.work_task_for_attempt(&project(), "second")
            .unwrap()
            .steps[0]
            .attempts
            .len(),
        2
    );
}

#[test]
fn skips_forged_reports_conflicting_retries_and_other_steps_cannot_advance_progress() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = setup(&dir.path().join("guard.sqlite"));
    let task = task(&mut repo, "root", json!({"type":"project"}));
    let initial = request(&repo, &task, plan(), 0);
    let report = repo.query_task_plan(&task, &initial.plan, 10).unwrap();
    let mut skipped = plan();
    let QueryOperation::Passages { cursor, .. } = &mut skipped.steps[0].operation else {
        panic!()
    };
    *cursor = report["steps"][0]["nextCursor"].as_str().map(str::to_owned);
    let skipped = request(&repo, &task, skipped, 0);
    assert!(repo.checkpoint_reading(&task, &skipped, 10).is_err());
    let mut forged = request(&repo, &task, plan(), 0);
    forged.report_hash = "b".repeat(64);
    assert!(repo.checkpoint_reading(&task, &forged, 10).is_err());
    let saved = repo.checkpoint_reading(&task, &initial, 10).unwrap();
    let mut conflict = request(&repo, &task, plan(), 0);
    conflict.summary = "另一条摘要".into();
    assert!(repo.checkpoint_reading(&task, &conflict, 10).is_err());
    let mut changed: StoryQueryPlan =
        serde_json::from_value(saved["checkpoint"]["resumePlan"].clone()).unwrap();
    changed.question = "另一个问题".into();
    let mut conflict = skipped;
    conflict.plan = changed;
    conflict.expected_version = 1;
    assert!(repo.checkpoint_reading(&task, &conflict, 10).is_err());
    let other = crate::task(&mut repo, "other", json!({"type":"project"}));
    assert_eq!(
        repo.reading_progress(&other, 10).unwrap()["checkpoint"],
        Value::Null
    );
    assert!(repo.checkpoint_reading(&other, &initial, 10).is_err());
    assert_eq!(repo.reading_progress(&task, 10).unwrap(), saved);
}

#[test]
fn changing_source_after_pause_hides_the_old_note_and_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = setup(&dir.path().join("changed.sqlite"));
    let first = task(&mut repo, "first", json!({"type":"project"}));
    let page = request(&repo, &first, plan(), 0);
    repo.checkpoint_reading(&first, &page, 10).unwrap();
    let second = resume(&mut repo, &first, "second");
    save_source(&mut repo, 1);
    assert!(matches!(
        repo.reading_progress(&second, 15),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project(), "second")
            .unwrap()
            .steps[0]
            .reading_progress,
        Some(AuthorReadingProgress::Stale)
    );
    let work = repo.work_task_for_attempt(&project(), "second").unwrap();
    repo.control_work_task(&project(), &work.id, "pause", work.version, 16)
        .unwrap();
    repo.update_task(
        &project(),
        "second",
        &second.thread_id,
        TaskUpdate {
            status: "failed".into(),
            error: Some("来源已改变".into()),
            result_summary: None,
            now: 17,
        },
    )
    .unwrap();
    let paused = repo.work_task(&project(), &work.id).unwrap();
    assert!(matches!(
        repo.control_work_task(&project(), &work.id, "resume", paused.version, 18),
        Err(RepositoryError::TaskInputStale)
    ));
}

#[test]
fn receipt_failure_rolls_back_checkpoint_and_success_records_no_formal_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atomic.sqlite");
    let mut repo = setup(&path);
    let task = task(&mut repo, "root", json!({"type":"project"}));
    repo.bind_task_turn(&project(), "root", "thread-root", "turn")
        .unwrap();
    let page = request(&repo, &task, plan(), 0);
    let binding = RecoveryOperationBinding {
        project_id: project(),
        task_id: "root".into(),
        thread_id: "thread-root".into(),
        turn_id: "turn".into(),
        call_id: "call".into(),
        tool_name: "checkpoint_reading".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 10,
    };
    repo.bind_recovery_operation(binding).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_receipt BEFORE UPDATE ON story_tool_operations_v1 WHEN NEW.state='committed' BEGIN SELECT RAISE(ABORT,'test receipt failure'); END;").unwrap();
    assert!(repo.checkpoint_reading(&task, &page, 10).is_err());
    assert_eq!(
        repo.reading_progress(&task, 10).unwrap()["checkpoint"],
        Value::Null
    );
    db.execute_batch("DROP TRIGGER fail_receipt;").unwrap();
    let saved = repo.checkpoint_reading(&task, &page, 10).unwrap();
    let snapshot = repo
        .recovery_snapshot(
            &project(),
            &["root".into()],
            /*fence*/ None,
            /*cursor*/ 0,
            /*limit*/ 500,
            /*now*/ 11,
        )
        .unwrap();
    assert!(
        snapshot
            .to_string()
            .contains(saved["checkpoint"]["receiptRef"].as_str().unwrap())
    );
    assert!(!snapshot.to_string().contains("页内原文已读"));
    assert_eq!(
        repo.task(&project(), "root")
            .unwrap()
            .unwrap()
            .output_refs_json,
        "[]"
    );
}

#[test]
fn stale_compare_and_swap_and_corrupt_storage_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cas.sqlite");
    let mut repo = setup(&path);
    let task = task(&mut repo, "root", json!({"type":"project"}));
    let mut rival = SqliteStoryRepository::open(&path).unwrap();
    let first = request(&repo, &task, plan(), 0);
    let mut conflict = request(&rival, &task, plan(), 0);
    conflict.summary = "竞争者".into();
    let saved = repo.checkpoint_reading(&task, &first, 10).unwrap();
    assert!(rival.checkpoint_reading(&task, &conflict, 10).is_err());
    assert_eq!(rival.reading_progress(&task, 10).unwrap(), saved);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_reading_checkpoints_v1 SET state_hash='broken'",
        [],
    )
    .unwrap();
    assert!(matches!(
        repo.reading_progress(&task, 10),
        Err(RepositoryError::CorruptState)
    ));
}

#[test]
fn finite_page_budget_survives_new_attempt_and_notes_are_utf8_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget.sqlite");
    let mut repo = setup(&path);
    let first = task(&mut repo, "root", json!({"type":"project"}));
    let mut page = request(&repo, &first, plan(), 0);
    page.summary = "潮".repeat(171);
    assert!(repo.checkpoint_reading(&first, &page, 10).is_err());
    page.summary = "\0".repeat(512);
    let saved = repo.checkpoint_reading(&first, &page, 10).unwrap();
    assert_eq!(repo.reading_progress(&first, 10).unwrap(), saved);
    // Place this finite ledger near its boundary without pretending a model read 9999 pages.
    let db = rusqlite::Connection::open(&path).unwrap();
    let encoded: String = db
        .query_row(
            "SELECT state_json FROM story_reading_checkpoints_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut state: Value = serde_json::from_str(&encoded).unwrap();
    state["version"] = json!(9999);
    state["pages"] = json!(9999);
    state["receipt"] = json!(format!(
        "reading:step-root:9999:{}",
        state["requestHash"].as_str().unwrap()
    ));
    let encoded = state.to_string();
    db.execute(
        "UPDATE story_reading_checkpoints_v1 SET version=9999,state_json=?1,state_hash=?2",
        rusqlite::params![encoded, format!("{:x}", Sha256::digest(encoded.as_bytes()))],
    )
    .unwrap();
    let next = serde_json::from_value(saved["checkpoint"]["resumePlan"].clone()).unwrap();
    let page = request(&repo, &first, next, 9999);
    let exhausted = repo.checkpoint_reading(&first, &page, 10).unwrap();
    assert_eq!(exhausted["checkpoint"]["status"], "budgetExhausted");
    assert_eq!(exhausted["checkpoint"]["resumePlan"], Value::Null);
    assert_eq!(
        exhausted["checkpoint"]["acknowledgedTraversalComplete"],
        false
    );
    let second = resume(&mut repo, &first, "second");
    assert_eq!(repo.reading_progress(&second, 15).unwrap(), exhausted);
    assert_eq!(
        repo.work_task_for_attempt(&project(), "second")
            .unwrap()
            .steps[0]
            .reading_progress,
        Some(AuthorReadingProgress::Current {
            version: 10000,
            pages: 10000,
            acknowledged_characters: exhausted["checkpoint"]["acknowledgedCharacters"]
                .as_u64()
                .unwrap() as usize,
            traversal: ReadingTraversal::BudgetExhausted
        })
    );
    let reset = request(&repo, &second, plan(), 0);
    assert!(repo.checkpoint_reading(&second, &reset, 15).is_err());
}

#[test]
fn author_progress_is_read_only_survives_controls_and_hides_unverifiable_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("author-progress.sqlite");
    let mut repo = setup(&path);
    let first = task(&mut repo, "root", json!({"type":"project"}));
    let work = repo
        .work_task_for_attempt(&project(), &first.task_id)
        .unwrap();
    assert_eq!(work.steps[0].reading_progress, None);
    let saved = repo
        .checkpoint_reading(&first, &request(&repo, &first, plan(), 0), 10)
        .unwrap();
    let current = Some(AuthorReadingProgress::Current {
        version: 1,
        pages: 1,
        acknowledged_characters: saved["checkpoint"]["acknowledgedCharacters"]
            .as_u64()
            .unwrap() as usize,
        traversal: ReadingTraversal::Reading,
    });
    let page = repo.work_tasks_page(&project(), None).unwrap();
    assert_eq!(page.tasks[0].steps[0].reading_progress, current);
    assert_eq!(page.tasks[0].version, work.version);
    let paused = repo
        .control_work_task(&project(), &work.id, "pause", work.version, 11)
        .unwrap();
    assert_eq!(paused.steps[0].reading_progress, current);
    assert!(repo.reading_progress(&first, 11).is_err());
    // Author reading is not an active task capability check, even after expiry or cancellation.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET capability_expires_at=1 WHERE task_id='root'",
        [],
    )
    .unwrap();
    let cancelled = repo
        .control_work_task(&project(), &work.id, "cancel", paused.version, 12)
        .unwrap();
    assert_eq!(cancelled.steps[0].reading_progress, current);
    assert_eq!(cancelled.status, "cancelled");
    assert!(repo.reading_progress(&first, 12).is_err());
    drop(repo);
    let repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(repo.work_task(&project(), &work.id).unwrap(), cancelled);
    assert_eq!(
        repo.work_task(&ProjectId::parse("other-project").unwrap(), &work.id),
        Err(RepositoryError::TaskNotFound)
    );
    // A valid state digest must not allow a checkpoint from a different WorkStep.
    let encoded: String = db
        .query_row(
            "SELECT state_json FROM story_reading_checkpoints_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut corrupt: Value = serde_json::from_str(&encoded).unwrap();
    corrupt["taskId"] = json!("another-step-attempt");
    let encoded = corrupt.to_string();
    db.execute(
        "UPDATE story_reading_checkpoints_v1 SET state_json=?1,state_hash=?2",
        rusqlite::params![encoded, format!("{:x}", Sha256::digest(encoded.as_bytes()))],
    )
    .unwrap();
    assert_eq!(
        repo.work_task(&project(), &work.id).unwrap().steps[0].reading_progress,
        Some(AuthorReadingProgress::Unavailable)
    );
    db.execute(
        "UPDATE story_reading_checkpoints_v1 SET state_hash='broken'",
        [],
    )
    .unwrap();
    let unavailable = repo.work_tasks_page(&project(), None).unwrap();
    assert_eq!(
        unavailable.tasks[0].steps[0].reading_progress,
        Some(AuthorReadingProgress::Unavailable)
    );
    // Real database errors must not be disguised as a benign unavailable checkpoint.
    db.execute("DROP TABLE story_reading_checkpoints_v1", [])
        .unwrap();
    assert!(matches!(
        repo.work_task(&project(), &work.id),
        Err(RepositoryError::Storage(_))
    ));
}
