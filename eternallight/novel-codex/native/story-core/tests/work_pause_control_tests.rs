use novel_codex_story_core::{
    BranchId, CreateTaskRequest, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository, TaskUpdate,
};

fn fixture() -> (SqliteStoryRepository, ProjectId) {
    seed(SqliteStoryRepository::in_memory().unwrap())
}

fn seed(mut repo: SqliteStoryRepository) -> (SqliteStoryRepository, ProjectId) {
    let project = ProjectId::parse("pause-project").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "Pause".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    for id in ["first", "other"] {
        repo.create_task(CreateTaskRequest {
            task_id: id.into(),
            project_id: project.clone(),
            thread_id: format!("thread-{id}"),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"解释设定"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    }
    (repo, project)
}

#[test]
fn pause_revokes_only_original_attempt_and_resume_cannot_reanimate_it() {
    let (mut repo, project) = fixture();
    let work = repo.work_task(&project, "work-first").unwrap();
    let paused = repo
        .control_work_task(&project, &work.id, "pause", work.control_revision, 5)
        .unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused.steps[0].status, "blocked");
    let old = repo.task(&project, "first").unwrap().unwrap();
    assert!(old.was_author_paused());
    assert_eq!(old.capability_revoked_at, Some(5));
    assert_eq!(paused.steps[0].attempts[0].completed_at.is_some(), true);
    assert_eq!(
        repo.task(&project, "other")
            .unwrap()
            .unwrap()
            .capability_revoked_at,
        None
    );
    let resumed = repo
        .control_work_task(&project, &work.id, "resume", paused.control_revision, 6)
        .unwrap();
    assert_eq!(resumed.id, work.id);
    assert_eq!(resumed.status, "running");
    assert_eq!(resumed.steps[0].status, "queued");
    assert_eq!(resumed.steps[0].current_attempt_id, None);
    assert!(matches!(
        repo.update_task(
            &project,
            "first",
            "thread-first",
            TaskUpdate {
                status: "running".into(),
                error: None,
                result_summary: None,
                now: 7,
            }
        ),
        Err(RepositoryError::TaskNotWritable)
    ));
    assert!(repo
        .task(&project, "first")
        .unwrap()
        .unwrap()
        .was_author_paused());
}

#[test]
fn stale_pause_and_late_failure_do_not_change_original_or_other_work() {
    let (mut repo, project) = fixture();
    let work = repo.work_task(&project, "work-first").unwrap();
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "pause", work.control_revision + 1, 4),
        Err(RepositoryError::WorkTaskStale)
    ));
    assert_eq!(
        repo.task(&project, "first")
            .unwrap()
            .unwrap()
            .capability_revoked_at,
        None
    );
    let paused = repo
        .control_work_task(&project, &work.id, "pause", work.control_revision, 5)
        .unwrap();
    assert!(matches!(
        repo.update_task(
            &project,
            "first",
            "thread-first",
            TaskUpdate {
                status: "failed".into(),
                error: Some("late network failure".into()),
                result_summary: None,
                now: 6,
            }
        ),
        Err(RepositoryError::TaskNotWritable)
    ));
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), paused);
    assert_eq!(
        repo.work_task(&project, "work-other").unwrap().status,
        "running"
    );
}

#[test]
fn progress_does_not_invalidate_control_but_author_control_does() {
    let (mut repo, project) = fixture();
    let before = repo.work_task(&project, "work-first").unwrap();
    repo.update_task(&project, "first", "thread-first", TaskUpdate {
        status: "failed".into(), error: Some("transport failure".into()),
        result_summary: None, now: 3,
    }).unwrap();
    let progressed = repo.work_task(&project, &before.id).unwrap();
    assert!(progressed.version > before.version);
    assert_eq!(progressed.control_revision, before.control_revision);
    let paused = repo.control_work_task(&project, &before.id, "pause", before.control_revision, 4).unwrap();
    assert_eq!(paused.control_revision, before.control_revision + 1);
    assert!(matches!(repo.control_work_task(&project, &before.id, "cancel", before.control_revision, 5), Err(RepositoryError::WorkTaskStale)));
    assert_eq!(repo.work_task(&project, &before.id).unwrap(), paused);
    assert_eq!(repo.work_task(&project, "work-other").unwrap().control_revision, 0);
}

#[test]
fn durable_command_replays_without_repeating_control_or_reopening_old_authority() {
    use novel_codex_story_core::WorkControlCommand;
    let (mut repo, project) = fixture();
    let request = WorkControlCommand { command_id: uuid::Uuid::new_v4().to_string(), thread_id: "thread-first".into(),
        expected_control_revision: 0, command: "pause".into() };
    let first = repo.apply_work_control_command(&project, "work-first", &request, 4,
        |r| r.control_work_task(&project,"work-first","pause",0,4).map(|_|())).unwrap();
    assert_eq!(first.receipt.status, "applied");
    assert!(!first.replayed);
    let resumed = repo.control_work_task(&project,"work-first","resume",1,5).unwrap();
    let replay = repo.apply_work_control_command(&project,"work-first",&request,6, |_| panic!("duplicate executed")).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.receipt, first.receipt);
    assert_eq!(replay.work, resumed);
    let mut different = request.clone(); different.command = "cancel".into();
    assert!(repo.apply_work_control_command(&project,"work-first",&different,7, |_| panic!("different executed")).is_err());
    assert!(repo.apply_work_control_command(&project,"work-other",&request,7, |_| panic!("foreign executed")).is_err());
    assert!(repo.task(&project,"first").unwrap().unwrap().was_author_paused());
}

#[test]
fn rejected_command_and_intermediate_effects_are_atomic() {
    use novel_codex_story_core::WorkControlCommand;
    let (mut repo, project) = fixture();
    let before = repo.work_task(&project,"work-first").unwrap();
    let request = WorkControlCommand { command_id: uuid::Uuid::new_v4().to_string(), thread_id: "thread-first".into(),
        expected_control_revision: 0, command: "pause".into() };
    let rejected = repo.apply_work_control_command(&project,"work-first",&request,4, |r| {
        r.control_work_task(&project,"work-first","pause",0,4)?;
        Err(RepositoryError::TaskInputStale)
    }).unwrap();
    assert_eq!(rejected.receipt.status,"rejected");
    assert_eq!(rejected.work,before);
    assert_eq!(repo.task(&project,"first").unwrap().unwrap().capability_revoked_at,None);
    let replay = repo.apply_work_control_command(&project,"work-first",&request,5, |_| panic!("rejected replay executed")).unwrap();
    assert_eq!(replay.receipt,rejected.receipt);
    let panic_request = WorkControlCommand { command_id: uuid::Uuid::new_v4().to_string(), ..request };
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = repo.apply_work_control_command(&project,"work-first",&panic_request,6, |r| {
            r.control_work_task(&project,"work-first","pause",0,6)?;
            panic!("interrupted command");
        });
    })).is_err());
    assert_eq!(repo.work_task(&project,"work-first").unwrap(), before);
    assert!(repo.work_control_receipt(&project,"work-first",&panic_request.command_id).unwrap().is_none());
}

#[test]
fn committed_control_receipt_survives_database_reopen_without_executing_again() {
    use novel_codex_story_core::WorkControlCommand;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.sqlite3");
    let (mut repo, project) = seed(SqliteStoryRepository::open(&path).unwrap());
    let before = repo.work_task(&project, "work-first").unwrap();
    let command = WorkControlCommand { command_id: uuid::Uuid::new_v4().to_string(),
        thread_id: before.thread_id.clone(), command: "pause".into(), expected_control_revision: before.control_revision };
    let applied = repo.apply_work_control_command(&project, &before.id, &command, 5,
        |repo| repo.control_work_task(&project, &before.id, "pause", before.control_revision, 5).map(|_| ())).unwrap();
    drop(repo);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(reopened.work_control_receipt(&project, &before.id, &command.command_id).unwrap(), Some(applied.receipt.clone()));
    let replay = reopened.apply_work_control_command(&project, &before.id, &command, 9,
        |_| panic!("committed command must never execute twice")).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.receipt, applied.receipt);
    assert_eq!(replay.work.control_revision, 1);
    assert_eq!(reopened.task(&project, "first").unwrap().unwrap().capability_revoked_at, Some(5));
}

#[test]
fn scheduled_pause_keeps_the_original_grant_then_pauses_on_terminal_failure() {
    let (mut repo, project) = fixture();
    let prior = repo.task(&project, "first").unwrap().unwrap();
    let scheduled = repo.pause_work_after_step(&project, "work-first", 0).unwrap();
    assert_eq!(scheduled.status, "running");
    assert_eq!(scheduled.pause_after_step.as_ref().unwrap().remaining_attempts, 1);
    assert_eq!(repo.task(&project, "first").unwrap().unwrap(), prior);
    assert!(matches!(repo.pause_work_after_step(&project, "work-first", 1), Err(RepositoryError::TaskNotWritable)));
    repo.update_task(&project, "first", "thread-first", TaskUpdate { status: "failed".into(), error: Some("original failure".into()), result_summary: None, now: 6 }).unwrap();
    let paused = repo.work_task(&project, "work-first").unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused.control_revision, 1);
    assert_eq!(paused.pause_after_step.unwrap().remaining_attempts, 0);
    assert_eq!(repo.task(&project, "first").unwrap().unwrap().error.as_deref(), Some("original failure"));
    assert_eq!(repo.work_task(&project, "work-other").unwrap().status, "running");
}

#[test]
fn scheduled_pause_survives_reopen_and_immediate_stop_revokes_the_frozen_attempt() {
    use novel_codex_story_core::WorkControlCommand;
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("barrier.sqlite");
    let (mut repo, project) = seed(SqliteStoryRepository::open(&path).unwrap());
    let request = WorkControlCommand { command_id: uuid::Uuid::new_v4().to_string(), thread_id: "thread-first".into(), command: "pauseAfterStep".into(), expected_control_revision: 0 };
    let outcome = repo.apply_work_control_command(&project, "work-first", &request, 5, |r| r.pause_work_after_step(&project, "work-first", 0).map(|_| ())).unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let replay = repo.apply_work_control_command(&project, "work-first", &request, 6, |_| panic!("replayed scheduled pause executed")).unwrap();
    assert!(replay.replayed); assert_eq!(replay.receipt, outcome.receipt);
    assert_eq!(replay.work.pause_after_step.unwrap().remaining_attempts, 1);
    let paused = repo.control_work_task(&project, "work-first", "pause", 1, 7).unwrap();
    assert_eq!(paused.status, "paused"); assert!(paused.pause_after_step.is_none());
    assert!(repo.task(&project, "first").unwrap().unwrap().was_author_paused());
    assert_eq!(repo.task(&project, "first").unwrap().unwrap().capability_revoked_at, Some(7));
    assert!(repo.control_work_task(&project, "work-first", "resume", 2, 8).unwrap().pause_after_step.is_none());
}

#[test]
fn scheduled_pause_with_no_active_attempt_is_immediate_and_resume_clears_it() {
    let (mut repo, project) = fixture();
    repo.control_work_task(&project, "work-first", "pause", 0, 4).unwrap();
    let queued = repo.control_work_task(&project, "work-first", "resume", 1, 5).unwrap();
    assert_eq!(queued.steps[0].status, "queued");
    let paused = repo.pause_work_after_step(&project, "work-first", 2).unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused.pause_after_step.unwrap().remaining_attempts, 0);
    assert!(repo.control_work_task(&project, "work-first", "resume", 3, 7).unwrap().pause_after_step.is_none());
}

#[test]
fn scheduled_pause_denies_new_attempt_without_leaving_a_task_or_changing_work() {
    let (mut repo, project) = fixture();
    repo.control_work_task(&project, "work-first", "pause", 0, 4).unwrap();
    repo.control_work_task(&project, "work-first", "resume", 1, 5).unwrap();
    let before = repo.pause_work_after_step(&project, "work-first", 2).unwrap();
    let result = repo.create_task(CreateTaskRequest { task_id: "replacement".into(), project_id: project.clone(), thread_id: "thread-first".into(), operation: "advisory".into(), input_revision: 0, model: None,
        target_json: r#"{"resumeStepId":"step-first"}"#.into(), input_manifest_json: r#"{"authorIntent":"解释设定"}"#.into(), capability_hash: "b".repeat(64), capability_expires_at: 100 });
    assert!(matches!(result, Err(RepositoryError::TaskNotWritable)));
    assert!(repo.task(&project, "replacement").unwrap().is_none());
    assert_eq!(repo.work_task(&project, "work-first").unwrap(), before);
}

#[test]
fn scheduled_pause_preserves_successful_completion_receipt() {
    let (mut repo, project) = fixture();
    repo.pause_work_after_step(&project, "work-first", 0).unwrap();
    let done = repo.complete_task(&project, "first", "thread-first", "已解释设定".into(), None, 8).unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.result_summary, "已解释设定");
    let paused = repo.work_task(&project, "work-first").unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused.steps[0].status, "completed");
    assert_eq!(paused.pause_after_step.unwrap().remaining_attempts, 0);
    assert_eq!(repo.complete_task(&project, "first", "thread-first", "duplicate".into(), None, 9).unwrap(), done);
}

#[test]
fn scheduled_pause_freezes_all_attempts_and_rejects_new_dispatch_while_still_running() {
    let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("parallel.sqlite");
    let (mut repo, project) = seed(SqliteStoryRepository::open(&path).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    // Isolated fixture: two already-admitted siblings.
    db.execute("UPDATE story_work_steps_v1 SET work_task_id='work-first',ordinal=2 WHERE id='step-other'", []).unwrap();
    let scheduled = repo.pause_work_after_step(&project, "work-first", 0).unwrap();
    assert_eq!(scheduled.pause_after_step.unwrap().remaining_attempts, 2);
    repo.update_task(&project, "first", "thread-first", TaskUpdate {status:"failed".into(),error:Some("failure preserved".into()),result_summary:None,now:6}).unwrap();
    // Simulate a queued retry left by another scheduling path. Its original
    // author source is valid, but the frozen barrier must still deny a claim.
    db.execute("UPDATE story_work_steps_v1 SET status='queued',current_attempt_id=NULL WHERE id='step-first'", []).unwrap();
    let draining = repo.work_task(&project, "work-first").unwrap();
    assert_eq!(draining.status, "running");
    assert_eq!(draining.pause_after_step.as_ref().unwrap().remaining_attempts, 1);
    assert!(repo.authorize_task_capability(&project, "other", "thread-other", 0, &"a".repeat(64), 7).is_ok());
    let result = repo.create_task(CreateTaskRequest { task_id:"forbidden-next".into(),project_id:project.clone(),thread_id:"thread-first".into(),operation:"advisory".into(),input_revision:0,model:None,
        target_json:r#"{"resumeStepId":"step-first"}"#.into(),input_manifest_json:r#"{"authorIntent":"解释设定"}"#.into(),capability_hash:"b".repeat(64),capability_expires_at:100 });
    assert!(matches!(result, Err(RepositoryError::TaskNotWritable)));
    assert!(repo.task(&project,"forbidden-next").unwrap().is_none());
    assert_eq!(repo.work_task(&project,"work-first").unwrap(),draining);
    repo.update_task(&project,"other","thread-other",TaskUpdate {status:"failed".into(),error:Some("second failure".into()),result_summary:None,now:8}).unwrap();
    let paused = repo.work_task(&project,"work-first").unwrap();
    assert_eq!(paused.status,"paused");
    assert_eq!(paused.steps[0].status,"queued");
    assert_eq!(paused.steps[0].attempts.len(),1);
}

use novel_codex_story_core::{ProjectRevisionRepository, TasksRepository, WorkControlCommandsRepository, WorkPauseBarrierRepository, WorkTasksRepository};
