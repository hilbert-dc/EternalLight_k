use novel_codex_story_core::{
    BranchId, CommitId, CreateTaskRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn generation_chapter_and_run_advance_roll_back_together_on_late_write_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("atomic-generation.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = ProjectId::parse("project-generation-atomic").unwrap();
    let branch_id = BranchId::parse("main").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Atomic generation".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            branch_id.clone(),
            SnapshotId::parse("snapshot-generation-0").unwrap(),
        )
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project_id.clone(),
            branch_id: branch_id.clone(),
            object_id: ObjectId::parse("artifact-plan").unwrap(),
            object_kind: "plan".into(),
            revision_id: RevisionId::parse("plan-generation-atomic").unwrap(),
            commit_id: CommitId::parse("candidate-plan-atomic").unwrap(),
            idempotency_key: IdempotencyKey::parse("save-plan-atomic").unwrap(),
            request_digest: "plan-request".into(),
            expected_project_revision: 0,
            expected_snapshot_id: SnapshotId::parse("snapshot-generation-0").unwrap(),
            next_snapshot_id: SnapshotId::parse("snapshot-generation-plan").unwrap(),
            expected_candidate_revision_id: None,
            content_digest: "plan-content".into(),
            content_json: json!({"payload":{"chapters":[{"chapterPlanId":"plan-1","chapterNumber":1,"publicTitle":"开篇"}]}}).to_string(),
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    let source_digest = repository.project_source_watermarks(&project_id).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let snapshot_id = repository
        .branch_head(&project_id, &branch_id)
        .unwrap()
        .unwrap()
        .snapshot_id;
    let initial = json!({
        "id": "generation-atomic", "projectId": project_id.as_str(),
        "version": 0, "status": "running", "stage": "rollingPlan",
        "authorMessageId":"message-generation-atomic", "intentGeneration":0,
        "feedbackDecisions":[], "intentRevisions": [{"id":"intent-atomic","generation":0,"instruction": "继续故事","effectiveFromChapter":1,"source":{"type":"authorMessage","authorMessageId":"message-generation-atomic","createdAt":1}}],
        "startChapterNumber": 1, "completedChapters": 0, "targetChapters": 1,
        "chapterDeliveries":[],
        "sourceDigest": source_digest, "baseSnapshotId": snapshot_id.as_str(),
        "deadlineAt": 1_000, "maxTaskAttempts":10,"usedTaskAttempts":0,"activeTaskId": null,
        "currentChapterId": null, "currentChapterNumber": null,
        "stepToken": "initial-step",
        "providerBudget": {"maxRequests":10,"maxTokens":100,"usedRequests":0,"usedTokens":0,"unknownTokens":0,"overrun":false,"charges":[]}
    });
    repository
        .create_generation_run(&project_id, "generation-atomic", &initial.to_string())
        .unwrap();
    let mut ready = initial;
    ready["version"] = json!(1);
    ready["stage"] = json!("materializeChapter");
    repository
        .update_generation_run(&project_id, "generation-atomic", 0, &ready.to_string())
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "task-interrupted-by-chapter".into(),
            project_id: project_id.clone(),
            thread_id: "thread-interrupted-by-chapter".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"inspect existing draft"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 1_000,
        })
        .unwrap();
    let mut proposed = ready.clone();
    proposed["version"] = json!(2);
    proposed["stage"] = json!("draft");
    proposed["currentChapterId"] = json!("chapter-00000000000040008000000000000001");
    proposed["currentChapterNumber"] = json!(1);
    proposed["stepToken"] = json!("next-step");

    let fault = rusqlite::Connection::open(&path).unwrap();
    let original_plan: String = fault.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE revision_id='plan-generation-atomic'",
        [], |row| row.get(0),
    ).unwrap();
    let run_before = repository
        .generation_run(&project_id, "generation-atomic")
        .unwrap();
    let task_before = repository
        .task(&project_id, "task-interrupted-by-chapter")
        .unwrap();
    for (field, value) in [
        ("povCharacterId", json!(false)),
        ("povCharacterId", json!(" ")),
        ("storyTime", json!("tomorrow")),
        ("storyTime", json!(1.5)),
    ] {
        let mut malformed: serde_json::Value = serde_json::from_str(&original_plan).unwrap();
        malformed["payload"]["chapters"][0][field] = value;
        fault.execute("UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='plan-generation-atomic'",[malformed.to_string()]).unwrap();
        assert!(matches!(
            repository.materialize_generation_chapter(
                &project_id,
                "generation-atomic",
                /*expected_version*/ 1,
                &proposed.to_string(),
                /*now*/ 10
            ),
            Err(RepositoryError::InvalidChapter)
        ));
        assert_eq!(
            repository
                .generation_run(&project_id, "generation-atomic")
                .unwrap(),
            run_before
        );
        assert_eq!(
            repository
                .task(&project_id, "task-interrupted-by-chapter")
                .unwrap(),
            task_before
        );
        assert!(repository.chapters(&project_id).unwrap().is_empty());
        assert!(repository.chapter_bindings(&project_id).unwrap().is_empty());
        assert_eq!(repository.project_revision(&project_id).unwrap(), 0);
    }
    fault.execute("UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='plan-generation-atomic'",[original_plan]).unwrap();
    fault
        .execute_batch(
            "CREATE TRIGGER fail_generation_advance BEFORE UPDATE ON story_generation_runs_v1
             BEGIN SELECT RAISE(ABORT, 'simulated late write failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        repository.materialize_generation_chapter(
            &project_id,
            "generation-atomic",
            1,
            &proposed.to_string(),
            10
        ),
        Err(RepositoryError::Storage(_))
    ));
    assert!(repository.chapters(&project_id).unwrap().is_empty());
    assert_eq!(repository.project_revision(&project_id).unwrap(), 0);
    assert_eq!(
        repository
            .task(&project_id, "task-interrupted-by-chapter")
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );
    let unchanged = repository
        .generation_run(&project_id, "generation-atomic")
        .unwrap();
    assert_eq!(unchanged.version, 1);
    assert_eq!(unchanged.state_json, ready.to_string());

    fault
        .execute_batch("DROP TRIGGER fail_generation_advance;")
        .unwrap();
    let applied = repository
        .materialize_generation_chapter(
            &project_id,
            "generation-atomic",
            1,
            &proposed.to_string(),
            10,
        )
        .unwrap();
    let state: serde_json::Value = serde_json::from_str(&applied.state_json).unwrap();
    assert_eq!(state["stage"], "draft");
    assert_eq!(state["currentChapterNumber"], 1);
    assert_ne!(state["sourceDigest"], source_digest);
    assert_eq!(repository.chapters(&project_id).unwrap().len(), 1);
    let bindings = repository.chapter_bindings(&project_id).unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].plan_revision_id, "plan-generation-atomic");
    assert_eq!(bindings[0].chapter_plan_id, "plan-1");
    assert_eq!(
        bindings[0].chapter_id,
        "chapter-00000000000040008000000000000001"
    );
    assert_eq!(repository.project_revision(&project_id).unwrap(), 1);
    let cancelled = repository
        .task(&project_id, "task-interrupted-by-chapter")
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(cancelled.capability_revoked_at, Some(10));
    let work = repository
        .work_task(&project_id, "work-task-interrupted-by-chapter")
        .unwrap();
    assert_eq!(work.status, "cancelled");
    assert_eq!(work.steps[0].status, "cancelled");
    assert!(matches!(
        repository.materialize_generation_chapter(
            &project_id,
            "generation-atomic",
            1,
            &proposed.to_string(),
            10
        ),
        Err(RepositoryError::AuthoringRunStale)
    ));
    assert_eq!(repository.chapters(&project_id).unwrap().len(), 1);
}
