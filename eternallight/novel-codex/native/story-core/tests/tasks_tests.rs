use novel_codex_story_core::BranchId;
use novel_codex_story_core::ChapterProjection;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::CreateTaskRequest;
use novel_codex_story_core::IdError;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ObjectId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ProjectRecord;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SaveCandidateRequest;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::TaskActionRequest;
use novel_codex_story_core::TaskOutputBinding;
use novel_codex_story_core::TaskUpdate;
use serde_json::json;

fn parse<T>(value: &str, parser: impl FnOnce(String) -> Result<T, IdError>) -> T {
    parser(value.to_owned()).unwrap()
}

#[test]
fn task_bound_draft_output_and_completion_are_one_authority_chain() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-task", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Task".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    let task = repository
        .create_task(CreateTaskRequest {
            task_id: "task-1".into(),
            project_id: project_id.clone(),
            thread_id: "thread-1".into(),
            operation: "generateChapter".into(),
            input_revision: 0,
            model: Some("model".into()),
            target_json: r#"{"type":"chapter","chapterId":"chapter-1"}"#.into(),
            input_manifest_json:
                r#"{"expectedCandidateRevisionId":null,"authorIntent":"创作测试"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    assert_eq!(task.status, "queued");
    let bound = repository
        .bind_task_turn(&project_id, "task-1", "thread-1", "turn-1")
        .unwrap();
    assert_eq!(bound.turn_id.as_deref(), Some("turn-1"));
    assert_eq!(
        repository
            .bind_task_turn(&project_id, "task-1", "thread-1", "turn-1")
            .unwrap(),
        bound
    );
    assert!(matches!(
        repository.bind_task_turn(&project_id, "task-1", "thread-1", "turn-2"),
        Err(RepositoryError::TaskNotWritable)
    ));

    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project_id.clone(),
            branch_id: parse("main", BranchId::parse),
            object_id: parse("draft-chapter-1", ObjectId::parse),
            object_kind: "draft".into(),
            revision_id: parse("draft-1", RevisionId::parse),
            commit_id: parse("candidate-1", CommitId::parse),
            idempotency_key: parse("save-1", IdempotencyKey::parse),
            request_digest: "digest".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("snapshot-0", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-1", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "content".into(),
            content_json: r#"{"chapterId":"chapter-1","body":"chapter"}"#.into(),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "One".into(),
            }),
            task_output: Some(TaskOutputBinding {
                task_id: "task-1".into(),
                thread_id: "thread-1".into(),
                output_kind: "draft".into(),
                completeness: Some("chapterComplete".into()),
            }),
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    let completed = repository
        .complete_task(&project_id, "task-1", "thread-1", "done".into(), None, 10)
        .unwrap();

    assert_eq!(completed.status, "completed");
    assert_eq!(completed.delivery_status, "ready_for_review");
    assert!(completed.output_refs_json.contains("draft-1"));
    assert_eq!(completed.capability_revoked_at, Some(10));
    assert_eq!(
        repository
            .complete_task(&project_id, "task-1", "thread-1", "duplicate".into(), None, 20)
            .unwrap(),
        completed
    );
}

#[test]
fn blocked_task_reports_the_same_resume_barrier_as_the_domain_control() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-partial", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Partial".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "task-partial".into(),
            project_id: project_id.clone(),
            thread_id: "thread-partial".into(),
            operation: "generateChapter".into(),
            input_revision: 0,
            model: None,
            target_json: r#"{"type":"chapter","chapterId":"chapter-1"}"#.into(),
            input_manifest_json:
                r#"{"expectedCandidateRevisionId":null,"authorIntent":"创作测试"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "task-stale".into(),
            project_id: project_id.clone(),
            thread_id: "thread-stale".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: r#"{"type":"project"}"#.into(),
            input_manifest_json: r#"{"authorIntent":"分析作品"}"#.into(),
            capability_hash: "b".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project_id.clone(),
            branch_id: parse("main", BranchId::parse),
            object_id: parse("draft-chapter-1", ObjectId::parse),
            object_kind: "draft".into(),
            revision_id: parse("draft-partial", RevisionId::parse),
            commit_id: parse("candidate-partial", CommitId::parse),
            idempotency_key: parse("save-partial", IdempotencyKey::parse),
            request_digest: "digest".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("snapshot-0", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-1", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "content".into(),
            content_json: r#"{"chapterId":"chapter-1","body":"chapter"}"#.into(),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "One".into(),
            }),
            task_output: Some(TaskOutputBinding {
                task_id: "task-partial".into(),
                thread_id: "thread-partial".into(),
                output_kind: "draft".into(),
                completeness: Some("chapterComplete".into()),
            }),
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    repository
        .update_task(
            &project_id,
            "task-partial",
            "thread-partial",
            TaskUpdate {
                status: "failed".into(),
                error: Some("interrupted".into()),
                result_summary: None,
                now: 10,
            },
        )
        .unwrap();
    let task = repository
        .work_task(&project_id, "work-task-partial")
        .unwrap();
    assert_eq!(task.status, "blocked");
    assert_eq!(task.resume_block_reason.as_deref(), Some("partialOutput"));
    assert!(matches!(
        repository.control_work_task(&project_id, &task.id, "resume", task.version, 11),
        Err(RepositoryError::TaskInputStale)
    ));
    repository
        .update_task(
            &project_id,
            "task-stale",
            "thread-stale",
            TaskUpdate {
                status: "failed".into(),
                error: Some("interrupted".into()),
                result_summary: None,
                now: 12,
            },
        )
        .unwrap();
    repository
        .materialize_standalone_chapter(
            novel_codex_story_core::MaterializeStandaloneChapterRequest {
                project_id: &project_id,
                title: "Another chapter",
                volume_id: None,
                expected_revision: 0,
                idempotency_key: "new-chapter-key",
                request_digest: "author",
                now: 12,
            },
        )
        .unwrap();
    let stale = repository
        .work_task(&project_id, "work-task-stale")
        .unwrap();
    assert_eq!(stale.resume_block_reason.as_deref(), Some("inputChanged"));
    assert!(matches!(
        repository.control_work_task(&project_id, &stale.id, "resume", stale.version, 13),
        Err(RepositoryError::TaskInputStale)
    ));
}

#[test]
fn cancelling_an_attempt_cancels_its_parent_work_task() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-cancel-attempt", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Task".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "task-cancel-attempt".into(),
            project_id: project_id.clone(),
            thread_id: "thread-cancel-attempt".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"创作测试"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    repository
        .update_task(
            &project_id,
            "task-cancel-attempt",
            "thread-cancel-attempt",
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 5,
            },
        )
        .unwrap();
    let work = repository
        .work_task(&project_id, "work-task-cancel-attempt")
        .unwrap();
    assert_eq!(work.status, "cancelled");
    assert_eq!(work.delivery_status, "pending");
}

#[test]
fn task_input_rejects_formal_chapter_change_without_object_head_change() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-task-directory", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Directory".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-directory", SnapshotId::parse),
        )
        .unwrap();
    let task = repository
        .create_task(CreateTaskRequest {
            task_id: "task-directory".into(),
            project_id: project_id.clone(),
            thread_id: "thread-directory".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"创作测试"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    let before = repository.project_source_watermarks(&project_id).unwrap();
    repository
        .materialize_standalone_chapter(
            novel_codex_story_core::MaterializeStandaloneChapterRequest {
                project_id: &project_id,
                title: "新章节",
                volume_id: None,
                expected_revision: 0,
                idempotency_key: "directory-key",
                request_digest: "directory",
                now: 2,
            },
        )
        .unwrap();
    let after = repository.project_source_watermarks(&project_id).unwrap();
    assert_eq!(before["objectCount"], after["objectCount"]);
    assert_ne!(before["digest"], after["digest"]);
    assert_eq!(after["projectRevision"], 1);
    assert!(matches!(
        repository.assert_task_inputs_current(&task),
        Err(RepositoryError::TaskInputStale)
    ));
}

#[test]
fn cancelling_a_claimed_candidate_closes_its_action_request() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-cancel-candidate", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Candidate".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-candidate", SnapshotId::parse),
        )
        .unwrap();
    let create_task = |repository: &mut SqliteStoryRepository,
                       task_id: &str,
                       operation: &str,
                       target: serde_json::Value| {
        repository
            .create_task(CreateTaskRequest {
                task_id: task_id.into(),
                project_id: project_id.clone(),
                thread_id: "thread-candidate".into(),
                operation: operation.into(),
                input_revision: 0,
                model: None,
                target_json: target.to_string(),
                input_manifest_json: r#"{"authorIntent":"生成候选"}"#.into(),
                capability_hash: "a".repeat(64),
                capability_expires_at: 100,
            })
            .unwrap();
    };
    create_task(
        &mut repository,
        "task-source",
        "advisory",
        json!({"type":"project"}),
    );
    repository
        .record_task_action(
            &project_id,
            "task-source",
            "candidate-key",
            TaskActionRequest {
                id: "action-candidate".into(),
                source_task_id: "task-source".into(),
                operation: "generateBrief".into(),
                target: json!({"type":"artifact"}),
                instruction: "拟定简报".into(),
                input_revision: 0,
                intent_revision: 0,
                status: "proposed".into(),
                result_task_id: None,
                error: None,
            },
        )
        .unwrap();
    repository
        .complete_task(
            &project_id,
            "task-source",
            "thread-candidate",
            "done".into(), None,
            2,
        )
        .unwrap();
    repository
        .approve_task_action(&project_id, "task-source", "action-candidate", 0, 2)
        .unwrap();
    create_task(
        &mut repository,
        "task-candidate",
        "generateBrief",
        json!({"type":"artifact", "requestedByTaskId":"task-source", "requestedActionId":"action-candidate"}),
    );
    repository
        .update_task(
            &project_id,
            "task-candidate",
            "thread-candidate",
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 3,
            },
        )
        .unwrap();
    let action = &repository.task_actions(&project_id, "task-source").unwrap()[0];
    assert_eq!(action.status, "failed");
    assert_eq!(action.error.as_deref(), Some("创作任务已取消"));
    assert_eq!(action.result_task_id.as_deref(), Some("task-candidate"));
    assert_eq!(
        repository
            .work_task_for_attempt(&project_id, "task-candidate")
            .unwrap()
            .status,
        "cancelled"
    );
}

#[test]
fn older_paused_task_remains_visible_after_many_completed_tasks() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-task-list", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Task list".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-task-list", SnapshotId::parse),
        )
        .unwrap();
    let create_advice = |repository: &mut SqliteStoryRepository, task_id: &str| {
        repository
            .create_task(CreateTaskRequest {
                task_id: task_id.into(),
                project_id: project_id.clone(),
                thread_id: format!("thread-{task_id}"),
                operation: "advisory".into(),
                input_revision: 0,
                model: None,
                target_json: "{}".into(),
                input_manifest_json: r#"{"authorIntent":"创作测试"}"#.into(),
                capability_hash: "a".repeat(64),
                capability_expires_at: 100,
            })
            .unwrap();
    };
    create_advice(&mut repository, "task-old");
    let old = repository.work_task(&project_id, "work-task-old").unwrap();
    repository
        .control_work_task(&project_id, &old.id, "pause", old.version, 5)
        .unwrap();
    for index in 0..31 {
        let task_id = format!("task-new-{index}");
        create_advice(&mut repository, &task_id);
        repository
            .complete_task(
                &project_id,
                &task_id,
                &format!("thread-{task_id}"),
                "done".into(), None,
                10,
            )
            .unwrap();
    }
    let listed = repository.work_tasks_page(&project_id, None).unwrap();
    assert_eq!(listed.active_count, 1);
    assert_eq!(listed.tasks.len(), 30);
    assert_eq!(listed.tasks[0].id, "work-task-old");
    assert_eq!(listed.tasks[0].status, "paused");
    assert!(
        listed
            .tasks
            .iter()
            .skip(1)
            .all(|task| task.status == "completed")
    );
    let second = repository
        .work_tasks_page(&project_id, listed.next_cursor.as_deref())
        .unwrap();
    assert_eq!(second.tasks.len(), 2);
    assert_eq!(second.next_cursor, None);
    assert!(second.tasks.iter().all(|task| task.status == "completed"));
    create_advice(&mut repository, "task-after-page");
    assert_eq!(
        repository.work_tasks_page(&project_id, listed.next_cursor.as_deref()),
        Err(RepositoryError::WorkTaskStale)
    );
    let refreshed = repository.work_tasks_page(&project_id, None).unwrap();
    assert_eq!(refreshed.active_count, 2);
    assert_eq!(refreshed.tasks[0].id, "work-task-after-page");
    repository
        .control_work_task(
            &project_id,
            "work-task-after-page",
            "cancel",
            refreshed.tasks[0].version,
            20,
        )
        .unwrap();
    assert_eq!(
        repository.work_tasks_page(&project_id, refreshed.next_cursor.as_deref()),
        Err(RepositoryError::WorkTaskStale)
    );
    assert!(matches!(
        repository.work_tasks_page(&project_id, Some("bad-cursor")),
        Err(RepositoryError::InvalidContent(_))
    ));
}

#[test]
fn recovered_attempt_with_persisted_complete_output_is_not_replayed() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-receipt", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Receipt".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-receipt", SnapshotId::parse),
        )
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "task-receipt".into(),
            project_id: project_id.clone(),
            thread_id: "thread-receipt".into(),
            operation: "generateBrief".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"创作测试"}"#.into(),
            capability_hash: "d".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project_id.clone(),
            branch_id: parse("main", BranchId::parse),
            object_id: parse("artifact-brief", ObjectId::parse),
            object_kind: "brief".into(),
            revision_id: parse("revision-receipt", RevisionId::parse),
            commit_id: parse("candidate-receipt", CommitId::parse),
            idempotency_key: parse("save-receipt", IdempotencyKey::parse),
            request_digest: "digest".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("snapshot-receipt", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-receipt-next", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "content".into(),
            content_json: r#"{"premise":"a","promise":"b","stakes":"c","endingDirection":"d"}"#
                .into(),
            chapter_projection: None,
            task_output: Some(TaskOutputBinding {
                task_id: "task-receipt".into(),
                thread_id: "thread-receipt".into(),
                output_kind: "brief".into(),
                completeness: None,
            }),
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        repository.recover_orphaned_tasks(25).unwrap(),
        vec!["task-receipt"]
    );
    let recovered = repository
        .task(&project_id, "task-receipt")
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, "completed");
    assert_eq!(
        repository
            .work_task_for_attempt(&project_id, "task-receipt")
            .unwrap()
            .status,
        "completed"
    );
    assert_eq!(
        repository
            .complete_task(
                &project_id,
                "task-receipt",
                "thread-receipt",
                "duplicate".into(), None,
                26
            )
            .unwrap(),
        recovered
    );
}

#[test]
fn recovered_candidate_attempt_finishes_its_claimed_action() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-action-receipt", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Receipt".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-action", SnapshotId::parse),
        )
        .unwrap();
    let make_task = |id: &str, operation: &str, target: serde_json::Value| CreateTaskRequest {
        task_id: id.into(),
        project_id: project_id.clone(),
        thread_id: "thread-action".into(),
        operation: operation.into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: r#"{"authorIntent":"写简报"}"#.into(),
        capability_hash: "e".repeat(64),
        capability_expires_at: 100,
    };
    repository
        .create_task(make_task(
            "task-source",
            "advisory",
            json!({"type":"project"}),
        ))
        .unwrap();
    repository
        .record_task_action(
            &project_id,
            "task-source",
            "action-receipt-key",
            TaskActionRequest {
                id: "action-receipt".into(),
                source_task_id: "task-source".into(),
                operation: "generateBrief".into(),
                target: json!({"type":"artifact"}),
                instruction: "写简报".into(),
                input_revision: 0,
                intent_revision: 0,
                status: "proposed".into(),
                result_task_id: None,
                error: None,
            },
        )
        .unwrap();
    repository
        .complete_task(
            &project_id,
            "task-source",
            "thread-action",
            "讨论完毕".into(), None,
            10,
        )
        .unwrap();
    repository
        .approve_task_action(&project_id, "task-source", "action-receipt", 0, 10)
        .unwrap();
    repository.create_task(make_task("task-candidate", "generateBrief", json!({"type":"artifact","requestedByTaskId":"task-source","requestedActionId":"action-receipt"}))).unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project_id.clone(),
            branch_id: parse("main", BranchId::parse),
            object_id: parse("artifact-brief", ObjectId::parse),
            object_kind: "brief".into(),
            revision_id: parse("revision-action", RevisionId::parse),
            commit_id: parse("candidate-action", CommitId::parse),
            idempotency_key: parse("save-action", IdempotencyKey::parse),
            request_digest: "digest".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("snapshot-action", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-action-next", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "content".into(),
            content_json: r#"{"premise":"a"}"#.into(),
            chapter_projection: None,
            task_output: Some(TaskOutputBinding {
                task_id: "task-candidate".into(),
                thread_id: "thread-action".into(),
                output_kind: "brief".into(),
                completeness: None,
            }),
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        repository.recover_orphaned_tasks(20).unwrap(),
        vec!["task-candidate"]
    );
    let action = repository
        .task_actions(&project_id, "task-source")
        .unwrap()
        .remove(0);
    assert_eq!(action.status, "completed");
    assert_eq!(action.result_task_id.as_deref(), Some("task-candidate"));
    assert_eq!(
        repository
            .work_task_for_attempt(&project_id, "task-candidate")
            .unwrap()
            .status,
        "completed"
    );
    assert_eq!(
        repository
            .finish_task_action(
                &project_id,
                "task-source",
                "action-receipt",
                Some("task-candidate"),
                None
            )
            .unwrap(),
        action
    );
    let original_request = TaskActionRequest {
        id: "action-receipt".into(),
        source_task_id: "task-source".into(),
        operation: "generateBrief".into(),
        target: json!({"type":"artifact"}),
        instruction: "写简报".into(),
        input_revision: 0,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    assert_eq!(
        repository
            .record_task_action(
                &project_id,
                "task-source",
                "action-receipt-key",
                original_request.clone(),
            )
            .unwrap(),
        action
    );
    let mut altered_request = original_request.clone();
    altered_request.id = "different-action".into();
    assert_eq!(
        repository
            .record_task_action(
                &project_id,
                "task-source",
                "action-receipt-key",
                altered_request,
            )
            .unwrap(),
        action
    );
    let mut altered_request = original_request;
    altered_request.instruction = "写另一份简报".into();
    assert!(matches!(
        repository.record_task_action(
            &project_id,
            "task-source",
            "action-receipt-key",
            altered_request,
        ),
        Err(RepositoryError::TaskBindingInvalid)
    ));
}

#[test]
fn task_transitions_preserve_terminal_evidence_and_require_validated_completion() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-state", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "State".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-state", SnapshotId::parse),
        )
        .unwrap();
    for terminal in ["cancelled", "failed"] {
        let task_id = format!("task-{terminal}");
        repository
            .create_task(CreateTaskRequest {
                task_id: task_id.clone(),
                project_id: project_id.clone(),
                thread_id: "thread-state".into(),
                operation: "generateBrief".into(),
                input_revision: 0,
                model: None,
                target_json: "{}".into(),
                input_manifest_json: r#"{"authorIntent":"创作测试"}"#.into(),
                capability_hash: "b".repeat(64),
                capability_expires_at: 100,
            })
            .unwrap();
        let update = |status: &str| TaskUpdate {
            status: status.into(),
            error: Some("original".into()),
            result_summary: None,
            now: 10,
        };
        assert!(matches!(
            repository.update_task(&project_id, &task_id, "thread-state", update("completed")),
            Err(RepositoryError::TaskNotWritable)
        ));
        assert!(matches!(
            repository.complete_task(&project_id, &task_id, "thread-state", "done".into(), None, 10),
            Err(RepositoryError::TaskOutputMissing)
        ));
        for status in ["running", "waiting", "running"] {
            repository
                .update_task(&project_id, &task_id, "thread-state", update(status))
                .unwrap();
        }
        let finished = repository
            .update_task(&project_id, &task_id, "thread-state", update(terminal))
            .unwrap();
        let mut duplicate = update(terminal);
        duplicate.error = Some("replacement".into());
        duplicate.now = 20;
        assert_eq!(
            repository
                .update_task(&project_id, &task_id, "thread-state", duplicate)
                .unwrap(),
            finished
        );
        for status in [
            "queued",
            "running",
            "waiting",
            "completed",
            if terminal == "failed" {
                "cancelled"
            } else {
                "failed"
            },
        ] {
            assert!(matches!(
                repository.update_task(&project_id, &task_id, "thread-state", update(status)),
                Err(RepositoryError::TaskNotWritable)
            ));
        }
        assert!(matches!(
            repository.complete_task(&project_id, &task_id, "thread-state", "done".into(), None, 20),
            Err(RepositoryError::TaskNotWritable)
        ));
        assert_eq!(
            repository.task(&project_id, &task_id).unwrap(),
            Some(finished)
        );
    }
}

#[test]
fn a_claimed_conversation_step_survives_interruption_without_duplicate_execution() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-workflow", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Workflow".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-workflow", SnapshotId::parse),
        )
        .unwrap();
    let request = |id: &str, operation: &str, target: serde_json::Value| CreateTaskRequest {
        task_id: id.into(),
        project_id: project_id.clone(),
        thread_id: "thread-workflow".into(),
        operation: operation.into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: r#"{"authorIntent":"请拟定简报"}"#.into(),
        capability_hash: "c".repeat(64),
        capability_expires_at: 100,
    };
    repository
        .create_task(request(
            "task-advice",
            "advisory",
            json!({"type":"project"}),
        ))
        .unwrap();
    repository
        .record_task_action(
            &project_id,
            "task-advice",
            "action-key",
            TaskActionRequest {
                id: "action-brief".into(),
                source_task_id: "task-advice".into(),
                operation: "generateBrief".into(),
                target: json!({"type":"artifact"}),
                instruction: "生成简报".into(),
                input_revision: 0,
                intent_revision: 0,
                status: "proposed".into(),
                result_task_id: None,
                error: None,
            },
        )
        .unwrap();
    repository
        .complete_task(
            &project_id,
            "task-advice",
            "thread-workflow",
            "done".into(), None,
            10,
        )
        .unwrap();
    repository
        .approve_task_action(&project_id, "task-advice", "action-brief", 0, 10)
        .unwrap();
    let target = json!({"type":"artifact", "requestedByTaskId":"task-advice", "requestedActionId":"action-brief"});
    repository
        .create_task(request("task-first", "generateBrief", target.clone()))
        .unwrap();
    assert!(matches!(
        repository.create_task(request("task-duplicate", "generateBrief", target.clone())),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(
        repository.task(&project_id, "task-duplicate").unwrap(),
        None
    );
    let claimed = repository
        .work_task_for_attempt(&project_id, "task-first")
        .unwrap();
    assert_eq!(
        claimed.steps[1]
            .attempts
            .iter()
            .map(|attempt| attempt.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-first"]
    );
    assert_eq!(claimed.steps[1].status, "running");
    assert_eq!(claimed.steps[1].attempts[0].attempt_number, 1);
    assert!(claimed.steps[1].attempts[0].started_at > 0);
    assert_eq!(claimed.steps[1].attempts[0].completed_at, None);
    assert_eq!(
        claimed.steps[1].expected_output_kind.as_deref(),
        Some("brief")
    );
    assert_eq!(
        repository.task_actions(&project_id, "task-advice").unwrap()[0]
            .result_task_id
            .as_deref(),
        Some("task-first")
    );
    assert_eq!(
        repository.recover_orphaned_tasks(20).unwrap(),
        vec!["task-first"]
    );
    let interrupted = repository.work_task(&project_id, &claimed.id).unwrap();
    assert_eq!(interrupted.status, "blocked");
    assert_eq!(interrupted.steps[1].status, "blocked");
    assert!(interrupted.steps[1].attempts[0].completed_at.is_some());
    repository
        .control_work_task(&project_id, &claimed.id, "resume", interrupted.version, 21)
        .unwrap();
    repository
        .create_task(request("task-retry", "generateBrief", target))
        .unwrap();
    let retried = repository.work_task(&project_id, &claimed.id).unwrap();
    assert_eq!(
        retried.steps[1]
            .attempts
            .iter()
            .map(|attempt| attempt.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-first", "task-retry"]
    );
    assert_eq!(retried.steps[1].attempts[1].attempt_number, 2);
    repository
        .control_work_task(&project_id, &claimed.id, "cancel", retried.version, 30)
        .unwrap();
    assert_eq!(
        repository
            .task(&project_id, "task-retry")
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        repository
            .work_task(&project_id, &claimed.id)
            .unwrap()
            .status,
        "cancelled"
    );
}

#[test]
fn task_control_returns_its_committed_snapshot_and_rejects_stale_cross_connection_commands() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.sqlite");
    let mut first = SqliteStoryRepository::open(&path).unwrap();
    let project_id = parse("project-control-snapshot", ProjectId::parse);
    first
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Control".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-control", SnapshotId::parse),
        )
        .unwrap();
    first
        .create_task(CreateTaskRequest {
            task_id: "task-control".into(),
            project_id: project_id.clone(),
            thread_id: "thread-control".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: r#"{"type":"project"}"#.into(),
            input_manifest_json: r#"{"authorIntent":"创作测试"}"#.into(),
            capability_hash: "d".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    let mut second = SqliteStoryRepository::open(&path).unwrap();
    let before = second.work_task(&project_id, "work-task-control").unwrap();
    let paused = first
        .control_work_task(&project_id, &before.id, "pause", before.version, 10)
        .unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused, second.work_task(&project_id, &before.id).unwrap());
    assert!(matches!(
        second.control_work_task(&project_id, &before.id, "cancel", before.version, 11),
        Err(RepositoryError::WorkTaskStale)
    ));
    let page = second.work_tasks_page(&project_id, None).unwrap();
    assert_eq!(page.active_count, 1);
    assert_eq!(page.tasks, vec![paused.clone()]);
    let cancelled = second
        .control_work_task(&project_id, &before.id, "cancel", paused.version, 12)
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(cancelled, first.work_task(&project_id, &before.id).unwrap());
    assert_eq!(
        first
            .work_tasks_page(&project_id, None)
            .unwrap()
            .active_count,
        0
    );
}
