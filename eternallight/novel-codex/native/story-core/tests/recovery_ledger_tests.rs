use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn project() -> ProjectId {
    ProjectId::parse("recovery-project").unwrap()
}
fn setup(path: &std::path::Path) -> SqliteStoryRepository {
    let mut repo = SqliteStoryRepository::open(path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "恢复".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "task".into(),
        project_id: project(),
        thread_id: "root".into(),
        operation: "generateBrief".into(),
        input_revision: 0,
        model: None,
        target_json: "{}".into(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 9999999999,
    })
    .unwrap();
    repo.bind_task_turn(&project(), "task", "root", "turn")
        .unwrap();
    repo
}
fn binding(call: &str, tool: &str) -> RecoveryOperationBinding {
    RecoveryOperationBinding {
        project_id: project(),
        task_id: "task".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: call.into(),
        tool_name: tool.into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 10,
    }
}
fn candidate() -> SaveCandidateRequest {
    SaveCandidateRequest {
        project_id: project(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("artifact-brief").unwrap(),
        object_kind: "brief".into(),
        revision_id: RevisionId::parse("brief-one").unwrap(),
        commit_id: CommitId::parse("commit-one").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-one").unwrap(),
        request_digest: "digest".into(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("snapshot-one").unwrap(),
        expected_candidate_revision_id: None,
        content_digest: "content".into(),
        content_json: json!({"payload":{"premise":"秘密正文，不进入恢复账本"}}).to_string(),
        chapter_projection: None,
        task_output: Some(TaskOutputBinding {
            task_id: "task".into(),
            thread_id: "root".into(),
            output_kind: "brief".into(),
            completeness: None,
        }),
        linked_impact_ids: vec![],
    }
}
fn snapshot(repo: &mut SqliteStoryRepository) -> serde_json::Value {
    repo.recovery_snapshot(
        &project(),
        &["task".into()],
        /*fence*/ None,
        /*cursor*/ 0,
        /*limit*/ 500,
        /*now*/ 20,
    )
    .unwrap()
}

#[test]
fn recovery_commit_receipt_survives_reopen_and_replay_without_body_or_author_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let mut repo = setup(&path);
    repo.bind_recovery_operation(binding("save", "save_brief"))
        .unwrap();
    let receipt = repo.save_candidate(candidate()).unwrap();
    drop(repo); // Host receives no successful event; only the database survives.
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let first = snapshot(&mut repo);
    assert_eq!(first["operations"][0]["state"], "committed");
    assert_eq!(
        first["operations"][0]["receipt_ref"],
        "runtime:candidate:brief-one"
    );
    assert!(!first.to_string().contains("秘密正文"));
    assert_eq!(
        repo.object_heads(&project()).unwrap()[0].accepted_revision_id,
        None
    );
    repo.bind_recovery_operation(binding("replay", "save_brief"))
        .unwrap();
    assert_eq!(repo.save_candidate(candidate()).unwrap(), receipt);
    repo.bind_recovery_operation(binding("complete", "complete_task"))
        .unwrap();
    repo.complete_task(
        &project(),
        "task",
        "root",
        "not persisted in recovery".into(), None,
        /*now*/ 30,
    )
    .unwrap();
    let all = snapshot(&mut repo);
    assert_eq!(all["tasks"][0]["status"], "completed");
    assert_eq!(
        all["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["receipt_ref"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("runtime:candidate:brief-one"),
            json!("runtime:candidate:brief-one"),
            json!("runtime:task:task:completed")
        ]
    );
    assert!(!all.to_string().contains("not persisted in recovery"));
}

#[test]
fn rejected_binding_or_atomic_proof_failure_never_commits_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let mut repo = setup(&path);
    let mut wrong = binding("identity", "save_brief");
    wrong.turn_id = "another-turn".into();
    assert!(repo.bind_recovery_operation(wrong).is_err());
    repo.bind_recovery_operation(binding("pending", "complete_task"))
        .unwrap();
    assert!(repo.save_candidate(candidate()).is_err()); // Proof mismatch rolls back business effects too.
    assert_eq!(repo.object_heads(&project()).unwrap(), vec![]);
    let pending = snapshot(&mut repo);
    assert_eq!(pending["operations"][0]["state"], "accepted");
    assert_eq!(
        pending["operations"][0]["receipt_ref"],
        serde_json::Value::Null
    );
    let mut reused = binding("pending", "complete_task");
    reused.request_hash = "c".repeat(64);
    assert_eq!(
        repo.bind_recovery_operation(reused),
        Err(RepositoryError::IdempotencyConflict)
    );
    drop(repo);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(snapshot(&mut reopened)["operations"], pending["operations"]);
}

#[test]
fn recovery_pages_freeze_operation_states_task_status_and_scope_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let mut repo = setup(&path);
    repo.bind_recovery_operation(binding("first", "save_brief"))
        .unwrap();
    repo.save_candidate(candidate()).unwrap();
    repo.bind_recovery_operation(binding("second", "complete_task"))
        .unwrap();
    let first = repo
        .recovery_snapshot(
            &project(),
            &["task".into()],
            /*fence*/ None,
            /*cursor*/ 0,
            /*limit*/ 1,
            /*now*/ 20,
        )
        .unwrap();
    assert_eq!(first["complete"], false);
    repo.complete_task(&project(), "task", "root", String::new(), None, /*now*/ 30)
        .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let fence = first["fence"].as_str().unwrap();
    let second = repo
        .recovery_snapshot(
            &project(),
            &["task".into()],
            Some(fence),
            /*cursor*/ 1,
            /*limit*/ 1,
            /*now*/ 40,
        )
        .unwrap();
    assert_eq!(second["tasks"], first["tasks"]);
    assert_eq!(second["operations"][0]["state"], "accepted");
    assert_eq!(second["complete"], true);
    assert_eq!(snapshot(&mut repo)["operations"][1]["state"], "committed");
    assert!(
        repo.recovery_snapshot(
            &ProjectId::parse("another").unwrap(),
            &["task".into()],
            Some(fence),
            /*cursor*/ 1,
            /*limit*/ 1,
            /*now*/ 40
        )
        .is_err()
    );
    assert!(
        repo.recovery_snapshot(
            &project(),
            &["other-task".into()],
            Some(fence),
            /*cursor*/ 1,
            /*limit*/ 1,
            /*now*/ 40
        )
        .is_err()
    );
    assert!(
        repo.recovery_snapshot(
            &project(),
            &["task".into()],
            Some(fence),
            /*cursor*/ 1,
            /*limit*/ 2,
            /*now*/ 40
        )
        .is_err()
    );
    assert!(
        repo.recovery_snapshot(
            &project(),
            &["task".into()],
            Some(fence),
            /*cursor*/ 1,
            /*limit*/ 1,
            /*now*/ 4000
        )
        .is_err()
    );
}

#[test]
fn advisory_action_and_knowledge_publication_have_typed_proofs_not_candidate_guesses() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let mut repo = setup(&path);
    let make_task = |id: &str, operation: &str, target: serde_json::Value| CreateTaskRequest {
        task_id: id.into(),
        project_id: project(),
        thread_id: "root".into(),
        operation: operation.into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: r#"{"authorIntent":"创建候选大纲"}"#.into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 9999999999,
    };
    repo.create_task(make_task("advisory", "advisory", json!({"type":"project"})))
        .unwrap();
    let mut action_binding = binding("action-call", "request_candidate_task");
    action_binding.task_id = "advisory".into();
    repo.bind_recovery_operation(action_binding.clone())
        .unwrap();
    let action = TaskActionRequest {
        id: "action".into(),
        source_task_id: "advisory".into(),
        operation: "generateOutline".into(),
        target: json!({"type":"artifact"}),
        instruction: "创建候选大纲".into(),
        input_revision: 0,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    repo.record_task_action(&project(), "advisory", "action-key", action.clone())
        .unwrap();
    action_binding.call_id = "action-replay".into();
    repo.bind_recovery_operation(action_binding).unwrap();
    assert_eq!(
        repo.record_task_action(&project(), "advisory", "action-key", action.clone())
            .unwrap(),
        action
    );
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let mut source = candidate();
    source.task_output = None;
    source.object_id = ObjectId::parse("draft-chapter").unwrap();
    source.object_kind = "draft".into();
    source.content_json =
        json!({"chapterNumber":1,"title":"正文","body":"信件被藏起来。"}).to_string();
    source.chapter_projection = Some(ChapterProjection {
        chapter_id: "chapter".into(),
        number: 1,
        title: "正文".into(),
    });
    repo.save_candidate(source).unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project(),
            object_id: &ObjectId::parse("draft-chapter").unwrap(),
            source_revision_id: &RevisionId::parse("brief-one").unwrap(),
            expected_revision: 0,
            extractor_version: "test",
            now: 1,
        })
        .unwrap();
    repo.create_task(make_task("knowledge","modelKnowledge",json!({"type":"chapter","chapterId":"chapter","batchId":batch.id,"sourceRevisionId":"brief-one"}))).unwrap();
    let mut publish = binding("publish", "publish_knowledge_batch");
    publish.task_id = "knowledge".into();
    let mut invalid = publish.clone();
    invalid.call_id = "invalid-publish".into();
    repo.bind_recovery_operation(invalid.clone()).unwrap();
    assert!(matches!(
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project(),
            batch_id: &batch.id,
            task_id: "knowledge",
            thread_id: "root",
            entities: &[json!("not an entity object")],
            assertions: &[],
            now: 10,
        }),
        Err(RepositoryError::InvalidContent(_))
    ));
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let rejected = repo
        .recovery_snapshot(
            &project(),
            &["knowledge".into()],
            /*fence*/ None,
            /*cursor*/ 0,
            /*limit*/ 500,
            /*now*/ 20,
        )
        .unwrap();
    let operation = &rejected["operations"][0];
    assert_eq!(
        json!({"state":operation["state"],"receipt":operation["receipt_ref"],"call":operation["tool_call_id"]}),
        json!({"state":"rejected","receipt":null,"call":"invalid-publish"}),
    );
    assert_eq!(
        repo.modeling_batch(&project(), &batch.id)
            .unwrap()
            .as_json(),
        batch.as_json()
    );
    assert!(matches!(
        repo.bind_recovery_operation(invalid.clone()),
        Err(RepositoryError::InvalidContent(_))
    ));
    invalid.request_hash = "c".repeat(64);
    assert!(matches!(
        repo.bind_recovery_operation(invalid),
        Err(RepositoryError::IdempotencyConflict)
    ));
    repo.bind_recovery_operation(publish.clone()).unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project(),
        batch_id: &batch.id,
        task_id: "knowledge",
        thread_id: "root",
        entities: &[],
        assertions: &[],
        now: 10,
    })
    .unwrap();
    publish.call_id = "publish-replay".into();
    repo.bind_recovery_operation(publish).unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project(),
        batch_id: &batch.id,
        task_id: "knowledge",
        thread_id: "root",
        entities: &[],
        assertions: &[],
        now: 11,
    })
    .unwrap();
    let all = repo
        .recovery_snapshot(
            &project(),
            &["advisory".into(), "knowledge".into()],
            /*fence*/ None,
            /*cursor*/ 0,
            /*limit*/ 500,
            /*now*/ 20,
        )
        .unwrap();
    let receipts = all["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["state"] == "committed")
        .map(|v| v["receipt_ref"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        receipts,
        vec![
            "runtime:action:action",
            "runtime:action:action",
            &format!("runtime:knowledge:{}", batch.id),
            &format!("runtime:knowledge:{}", batch.id)
        ]
    );
}

#[test]
fn planning_report_proof_is_stable_on_exact_replay_and_not_a_formal_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let mut repo = setup(&path);
    let mut outline = candidate();
    outline.task_output = None;
    outline.object_id = ObjectId::parse("artifact-outline").unwrap();
    outline.object_kind = "outline".into();
    outline.content_json =
        json!({"payload":{"nodes":[{"id":"book","kind":"book","title":"全书"}]}}).to_string();
    repo.save_candidate(outline).unwrap();
    repo.create_planning_review(CreatePlanningReview {
        run_id: "review",
        project_id: &project(),
        artifact_kind: "outline",
        revision_id: "brief-one",
        target_node_id: None,
        author_intent: "分析结构",
        roles: &["structure".into(), "experience".into()],
        author_message_id: "author-review",
        now: 1,
    })
    .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let source = repo.project_source_watermarks(&project()).unwrap();
    repo.authorize_work_budget(
        &project(),
        "work-review",
        &WorkBudgetAuthorization {
            expected_version: 0,
            expected_project_revision: 0,
            expected_source_digest: source["digest"].as_str().unwrap().into(),
            max_requests: Some(3),
            max_tokens: Some(1000),
            deadline_at: Some(now + 600),
        },
        now,
    )
    .unwrap();
    repo.create_planning_review_seat_task(&project(),"review","structure",CreateTaskRequest{task_id:"review-task".into(),project_id:project(),thread_id:"root".into(),operation:"reviewPlanningSeat".into(),input_revision:0,model:None,target_json:json!({"type":"artifact","artifactKind":"outline","revisionId":"brief-one","reviewRunId":"review","role":"structure"}).to_string(),input_manifest_json:"{}".into(),capability_hash:"a".repeat(64),capability_expires_at:9999999999}).unwrap();
    let mut meta = binding("report", "submit_planning_review_report");
    meta.task_id = "review-task".into();
    repo.bind_recovery_operation(meta.clone()).unwrap();
    let report = json!({"summary":"候选评估","strengths":[],"findings":[],"coverage":[{"checkId":"structure:book","status":"checked"}]});
    let saved = repo
        .save_planning_review_report(SavePlanningReviewReport {
            project_id: &project(),
            run_id: "review",
            task_id: "review-task",
            thread_id: "root",
            expected_revision: 0,
            report: &report,
            now: 10,
        })
        .unwrap();
    meta.call_id = "report-replay".into();
    repo.bind_recovery_operation(meta).unwrap();
    assert_eq!(
        repo.save_planning_review_report(SavePlanningReviewReport {
            project_id: &project(),
            run_id: "review",
            task_id: "review-task",
            thread_id: "root",
            expected_revision: 0,
            report: &report,
            now: 11
        })
        .unwrap(),
        saved
    );
    let all = repo
        .recovery_snapshot(
            &project(),
            &["review-task".into()],
            /*fence*/ None,
            /*cursor*/ 0,
            /*limit*/ 500,
            /*now*/ 20,
        )
        .unwrap();
    let expected = format!("runtime:report:{}", saved["id"].as_str().unwrap());
    assert_eq!(
        all["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["receipt_ref"].clone())
            .collect::<Vec<_>>(),
        vec![json!(expected), json!(expected)]
    );
    assert_eq!(
        repo.object_heads(&project()).unwrap()[0].accepted_revision_id,
        None
    );
}
