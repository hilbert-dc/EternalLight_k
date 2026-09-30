use novel_codex_story_core::{
    BranchId, CommitId, CreateTaskRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
    TaskActionRequest, TaskOutputBinding, TaskUpdate,
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

struct Fixture {
    repo: SqliteStoryRepository,
    project: ProjectId,
}

impl Fixture {
    fn new(operation: &str, target: Value) -> Self {
        let mut fixture = Self::proposed(operation, target);
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 0, 11)
            .unwrap();
        fixture
    }

    fn proposed(operation: &str, target: Value) -> Self {
        Self::proposed_in(
            SqliteStoryRepository::in_memory().unwrap(),
            operation,
            target,
        )
    }

    fn proposed_in(repo: SqliteStoryRepository, operation: &str, target: Value) -> Self {
        let mut fixture = Self {
            repo,
            project: ProjectId::parse("project-binding").unwrap(),
        };
        fixture
            .repo
            .create_project(
                ProjectRecord {
                    project_id: fixture.project.clone(),
                    title: "Binding".into(),
                    logline: String::new(),
                    created_at: 1,
                    updated_at: 1,
                },
                BranchId::parse("main").unwrap(),
                SnapshotId::parse("snapshot-binding").unwrap(),
            )
            .unwrap();
        fixture
            .repo
            .create_task(fixture.request("source", "advisory", json!({"type":"project"})))
            .unwrap();
        fixture
            .repo
            .record_task_action(
                &fixture.project,
                "source",
                "request-key",
                TaskActionRequest {
                    id: "action-binding".into(),
                    source_task_id: "source".into(),
                    operation: operation.into(),
                    target,
                    instruction: "生成候选".into(),
                    input_revision: 0,
                    intent_revision: 0,
                    status: "proposed".into(),
                    result_task_id: None,
                    error: None,
                },
            )
            .unwrap();
        fixture
            .repo
            .complete_task(
                &fixture.project,
                "source",
                "thread-binding",
                "done".into(),
                10,
            )
            .unwrap();
        fixture
    }

    fn request(&self, id: &str, operation: &str, mut target: Value) -> CreateTaskRequest {
        if id != "source" && id != "unrelated" {
            target["requestedByTaskId"] = json!("source");
            target["requestedActionId"] = json!("action-binding");
        }
        CreateTaskRequest {
            task_id: id.into(),
            project_id: self.project.clone(),
            thread_id: "thread-binding".into(),
            operation: operation.into(),
            input_revision: 0,
            model: None,
            target_json: target.to_string(),
            input_manifest_json: r#"{"authorIntent":"生成候选"}"#.into(),
            capability_hash: "f".repeat(64),
            capability_expires_at: 100,
        }
    }

    fn action(&self) -> TaskActionRequest {
        self.repo
            .task_actions(&self.project, "source")
            .unwrap()
            .remove(0)
    }

    fn save_brief(&mut self, task_id: &str) {
        self.repo
            .save_candidate(SaveCandidateRequest {
                project_id: self.project.clone(),
                branch_id: BranchId::parse("main").unwrap(),
                object_id: ObjectId::parse("artifact-brief").unwrap(),
                object_kind: "brief".into(),
                revision_id: RevisionId::parse("revision-binding").unwrap(),
                commit_id: CommitId::parse("commit-binding").unwrap(),
                idempotency_key: IdempotencyKey::parse("save-binding").unwrap(),
                request_digest: "request".into(),
                content_digest: "content".into(),
                expected_project_revision: 0,
                expected_snapshot_id: SnapshotId::parse("snapshot-binding").unwrap(),
                next_snapshot_id: SnapshotId::parse("snapshot-next").unwrap(),
                expected_candidate_revision_id: None,
                content_json: json!({"premise":"港城失去记忆"}).to_string(),
                chapter_projection: None,
                linked_impact_ids: Vec::new(),
                task_output: Some(TaskOutputBinding {
                    task_id: task_id.into(),
                    thread_id: "thread-binding".into(),
                    output_kind: "brief".into(),
                    completeness: None,
                }),
            })
            .unwrap();
    }
}

#[test]
fn persisted_permission_rejects_changed_action_or_author_evidence_after_reopen() {
    for mutation in [
        "UPDATE story_task_action_requests_v1 SET instruction='替换为另一个目标'",
        "UPDATE story_task_action_requests_v1 SET target_json='{\"type\":\"artifact\",\"extra\":true}'",
        "UPDATE story_tasks_v2 SET input_manifest_json=json_set(input_manifest_json,'$.authorIntent','新的未授权指令') WHERE task_id='source'",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("permission.sqlite3");
        let mut fixture = Fixture::proposed_in(
            SqliteStoryRepository::open(&path).unwrap(),
            "generateBrief",
            json!({"type":"artifact"}),
        );
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 0, 11)
            .unwrap();
        let approved = fixture.action();
        fixture.repo = SqliteStoryRepository::open(&path).unwrap();
        assert_eq!(
            fixture
                .repo
                .approve_task_action(&fixture.project, "source", "action-binding", 0, 12)
                .unwrap(),
            approved
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute(mutation, []).unwrap();
        assert!(matches!(
            fixture
                .repo
                .approve_task_action(&fixture.project, "source", "action-binding", 0, 13),
            Err(RepositoryError::TaskBindingInvalid)
        ));
        let target = fixture.action().target;
        assert!(matches!(
            fixture
                .repo
                .create_task(fixture.request("child", "generateBrief", target)),
            Err(RepositoryError::TaskBindingInvalid)
        ));
        assert_eq!(fixture.repo.task(&fixture.project, "child").unwrap(), None);
    }
}

#[test]
fn permission_and_executable_step_commit_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("permission-atomic.sqlite3");
    let mut fixture = Fixture::proposed_in(
        SqliteStoryRepository::open(&path).unwrap(),
        "generateBrief",
        json!({"type":"artifact"}),
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_step BEFORE INSERT ON story_work_steps_v1 BEGIN SELECT RAISE(ABORT,'test step failure'); END;").unwrap();
    assert!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 0, 11)
            .is_err()
    );
    assert_eq!(fixture.action().status, "proposed");
    let permits: i64 = db
        .query_row("SELECT COUNT(*) FROM story_action_permits_v1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(permits, 0);
    db.execute_batch("DROP TRIGGER reject_step").unwrap();
    fixture
        .repo
        .approve_task_action(&fixture.project, "source", "action-binding", 0, 12)
        .unwrap();
    fixture
        .repo
        .create_task(fixture.request("child", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
}

#[test]
fn model_proposal_cannot_create_a_candidate_until_the_author_approves_it() {
    let mut fixture = Fixture::proposed("generateBrief", json!({"type":"artifact"}));
    let proposal = fixture.action();
    assert_eq!(proposal.status, "proposed");
    let work = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "source")
        .unwrap();
    assert_eq!(work.steps.len(), 1);
    assert!(matches!(
        fixture.repo.create_task(fixture.request(
            "child",
            "generateBrief",
            json!({"type":"artifact"})
        )),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(fixture.repo.task(&fixture.project, "child").unwrap(), None);
    assert!(matches!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 1, 11),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    let approved = fixture
        .repo
        .approve_task_action(&fixture.project, "source", "action-binding", 0, 11)
        .unwrap();
    assert_eq!(approved.status, "queued");
    assert_eq!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 0, 12)
            .unwrap(),
        approved
    );
    assert!(matches!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 1, 12),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    let work = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "source")
        .unwrap();
    assert_eq!(work.steps.len(), 2);
    fixture
        .repo
        .create_task(fixture.request("child", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
}

#[test]
fn author_decline_is_durable_idempotent_and_never_creates_an_executable_step() {
    let mut fixture = Fixture::proposed("generateBrief", json!({"type":"artifact"}));
    let before = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "source")
        .unwrap();
    assert!(matches!(
        fixture
            .repo
            .decline_task_action(&fixture.project, "source", "action-binding", 1),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    let declined = fixture
        .repo
        .decline_task_action(&fixture.project, "source", "action-binding", 0)
        .unwrap();
    assert_eq!(declined.status, "declined");
    assert_eq!(fixture.action(), declined);
    assert_eq!(
        fixture
            .repo
            .decline_task_action(&fixture.project, "source", "action-binding", 0)
            .unwrap(),
        declined
    );
    let after = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "source")
        .unwrap();
    assert_eq!(after.steps, before.steps);
    assert_eq!(after.version, before.version + 1);
    assert!(
        fixture
            .repo
            .proposed_task_actions(&fixture.project)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 0, 12),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert!(matches!(
        fixture.repo.create_task(fixture.request(
            "child",
            "generateBrief",
            json!({"type":"artifact"})
        )),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(fixture.repo.task(&fixture.project, "child").unwrap(), None);
}

#[test]
fn stale_proposal_cannot_be_approved_after_a_project_change() {
    let mut fixture = Fixture::proposed("generateBrief", json!({"type":"artifact"}));
    fixture
        .repo
        .create_task(fixture.request("unrelated", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
    fixture.save_brief("unrelated");
    assert!(matches!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "source", "action-binding", 0, 12),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(fixture.action().status, "proposed");
    assert_eq!(
        fixture
            .repo
            .work_task_for_attempt(&fixture.project, "source")
            .unwrap()
            .steps
            .len(),
        1
    );
}

#[test]
fn action_registration_rechecks_source_heads_in_its_transaction_without_rewriting_receipts() {
    let mut fixture = Fixture::new("generateBrief", json!({"type":"artifact"}));
    let mut discussion = fixture.request("source", "advisory", json!({"type":"project"}));
    discussion.task_id = "discussion".into();
    fixture.repo.create_task(discussion).unwrap();
    let action = TaskActionRequest {
        id: "planned".into(),
        source_task_id: "discussion".into(),
        operation: "generateOutline".into(),
        target: json!({"type":"artifact"}),
        instruction: "生成大纲".into(),
        input_revision: 0,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    let receipt = fixture
        .repo
        .record_task_action(
            &fixture.project,
            "discussion",
            "planned-key",
            action.clone(),
        )
        .unwrap();
    fixture
        .repo
        .create_task(fixture.request("unrelated", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
    fixture.save_brief("unrelated");
    let before = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "discussion")
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .record_task_action(
                &fixture.project,
                "discussion",
                "planned-key",
                action.clone()
            )
            .unwrap(),
        receipt
    );
    for operation in [
        "generateBrief",
        "generateOutline",
        "generatePlan",
        "generatePropertyRules",
        "generateChapter",
    ] {
        let mut late = action.clone();
        late.id = format!("late-{operation}");
        late.operation = operation.into();
        if operation == "generateChapter" {
            late.target = json!({"type":"chapter","chapterId":"chapter-one","chapterNumber":1,"readingChapter":1,"sceneOrder":1,"scope":{"type":"wholeChapter"}});
        }
        assert!(matches!(
            fixture.repo.record_task_action(
                &fixture.project,
                "discussion",
                &format!("late-{operation}"),
                late
            ),
            Err(RepositoryError::TaskInputStale)
        ));
    }
    assert_eq!(
        fixture
            .repo
            .work_task_for_attempt(&fixture.project, "discussion")
            .unwrap(),
        before
    );
    assert_eq!(
        fixture
            .repo
            .task_actions(&fixture.project, "discussion")
            .unwrap(),
        vec![receipt]
    );
}

#[test]
fn queued_action_claim_rejects_external_candidate_changes_without_creating_an_attempt() {
    let mut fixture = Fixture::new("generateOutline", json!({"type":"artifact"}));
    fixture
        .repo
        .create_task(fixture.request("unrelated", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
    fixture.save_brief("unrelated");
    let before = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "source")
        .unwrap();
    let action = fixture.action();
    assert!(matches!(
        fixture.repo.create_task(fixture.request(
            "child",
            "generateOutline",
            json!({"type":"artifact"})
        )),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(fixture.repo.task(&fixture.project, "child").unwrap(), None);
    assert_eq!(fixture.action(), action);
    assert_eq!(
        fixture
            .repo
            .work_task_for_attempt(&fixture.project, "source")
            .unwrap(),
        before
    );
    let paused = fixture
        .repo
        .control_work_task(
            &fixture.project,
            &before.id,
            "pause",
            before.version,
            /*now*/ 12,
        )
        .unwrap();
    assert_eq!(paused.resume_block_reason.as_deref(), Some("inputChanged"));
    assert!(matches!(
        fixture.repo.control_work_task(
            &fixture.project,
            &before.id,
            "resume",
            paused.version,
            /*now*/ 13
        ),
        Err(RepositoryError::TaskInputStale)
    ));
}

#[test]
fn queued_action_claim_uses_its_own_preceding_output_as_the_next_source() {
    let mut fixture = Fixture::new("generateBrief", json!({"type":"artifact"}));
    // A fresh conversation registers both actions before either one runs.
    let mut discussion = fixture.request("source", "advisory", json!({"type":"project"}));
    discussion.task_id = "discussion".into();
    fixture.repo.create_task(discussion).unwrap();
    for (id, operation) in [("first", "generateBrief"), ("second", "generateOutline")] {
        fixture
            .repo
            .record_task_action(
                &fixture.project,
                "discussion",
                id,
                TaskActionRequest {
                    id: id.into(),
                    source_task_id: "discussion".into(),
                    operation: operation.into(),
                    target: json!({"type":"artifact"}),
                    instruction: "创作".into(),
                    input_revision: 0,
                    intent_revision: 0,
                    status: "proposed".into(),
                    result_task_id: None,
                    error: None,
                },
            )
            .unwrap();
    }
    fixture
        .repo
        .complete_task(
            &fixture.project,
            "discussion",
            "thread-binding",
            "ready".into(),
            /*now*/ 10,
        )
        .unwrap();
    for action_id in ["first", "second"] {
        fixture
            .repo
            .approve_task_action(&fixture.project, "discussion", action_id, 0, 10)
            .unwrap();
    }
    let mut first = fixture.request("first-attempt", "generateBrief", json!({"type":"artifact"}));
    first.target_json =
        json!({"type":"artifact","requestedByTaskId":"discussion","requestedActionId":"first"})
            .to_string();
    fixture.repo.create_task(first).unwrap();
    fixture.save_brief("first-attempt");
    fixture
        .repo
        .complete_task(
            &fixture.project,
            "first-attempt",
            "thread-binding",
            "saved".into(),
            /*now*/ 11,
        )
        .unwrap();
    let work = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "first-attempt")
        .unwrap();
    let paused = fixture
        .repo
        .control_work_task(
            &fixture.project,
            &work.id,
            "pause",
            work.version,
            /*now*/ 12,
        )
        .unwrap();
    assert_eq!(paused.resume_block_reason, None);
    fixture
        .repo
        .control_work_task(
            &fixture.project,
            &work.id,
            "resume",
            paused.version,
            /*now*/ 13,
        )
        .unwrap();
    let mut second = fixture.request(
        "second-attempt",
        "generateOutline",
        json!({"type":"artifact"}),
    );
    second.target_json =
        json!({"type":"artifact","requestedByTaskId":"discussion","requestedActionId":"second"})
            .to_string();
    let second = fixture.repo.create_task(second).unwrap();
    assert_eq!(second.status, "queued");
    let actions = fixture
        .repo
        .task_actions(&fixture.project, "discussion")
        .unwrap();
    assert_eq!(
        actions
            .iter()
            .map(|action| action.status.as_str())
            .collect::<Vec<_>>(),
        vec!["completed", "running"]
    );
}

#[test]
fn claiming_a_candidate_cannot_rewrite_the_persisted_target_or_scope() {
    let target = json!({"type":"chapter", "chapterId":"chapter-one", "chapterNumber":1,
        "readingChapter":1, "sceneOrder":1, "scope":{"type":"throughScene", "endSceneOrder":1}});
    let mut fixture = Fixture::new("generateChapter", target.clone());
    let before = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "source")
        .unwrap();
    let mut targets = Vec::new();
    let mut changed = target.clone();
    changed["chapterId"] = json!("chapter-other");
    targets.push(changed);
    let mut changed = target.clone();
    changed["scope"] = json!({"type":"wholeChapter"});
    targets.push(changed);
    let mut changed = target.clone();
    changed["povCharacterId"] = json!("other");
    targets.push(changed);
    let mut changed = target.clone();
    changed["runId"] = json!("unrelated-run");
    targets.push(changed);
    for (field, value) in [
        ("runId", Value::Null),
        ("sourceTaskId", json!("source")),
        ("resumeStepId", json!("step-source")),
    ] {
        let mut changed = target.clone();
        changed[field] = value;
        targets.push(changed);
    }
    for changed in targets {
        let result =
            fixture
                .repo
                .create_task(fixture.request("child", "generateChapter", changed.clone()));
        assert!(
            matches!(result, Err(RepositoryError::TaskBindingInvalid)),
            "target: {changed}; result: {result:?}"
        );
        assert_eq!(fixture.repo.task(&fixture.project, "child").unwrap(), None);
        assert_eq!(
            fixture
                .repo
                .work_task_for_attempt(&fixture.project, "source")
                .unwrap(),
            before
        );
    }
    for source in [None, Some(Value::Null), Some(json!(42))] {
        let mut request = fixture.request("child", "generateChapter", target.clone());
        let mut changed: Value = serde_json::from_str(&request.target_json).unwrap();
        if let Some(source) = source {
            changed["requestedByTaskId"] = source;
        } else {
            changed.as_object_mut().unwrap().remove("requestedByTaskId");
        }
        request.target_json = changed.to_string();
        assert!(matches!(
            fixture.repo.create_task(request),
            Err(RepositoryError::TaskBindingInvalid)
        ));
        assert_eq!(
            fixture
                .repo
                .work_task_for_attempt(&fixture.project, "source")
                .unwrap(),
            before
        );
        assert_eq!(fixture.repo.task(&fixture.project, "child").unwrap(), None);
    }
    fixture
        .repo
        .create_task(fixture.request("child", "generateChapter", target))
        .unwrap();
    let work = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "child")
        .unwrap();
    assert_eq!(work.steps[1].current_attempt_id.as_deref(), Some("child"));
    assert_eq!(work.steps[1].attempts.len(), 1);
}

#[test]
fn domain_completion_requires_a_claimed_completed_attempt_and_preserves_receipts() {
    let mut fixture = Fixture::new("generateBrief", json!({"type":"artifact"}));
    fixture
        .repo
        .create_task(fixture.request("unrelated", "advisory", json!({"type":"project"})))
        .unwrap();
    fixture
        .repo
        .complete_task(
            &fixture.project,
            "unrelated",
            "thread-binding",
            "done".into(),
            11,
        )
        .unwrap();
    let queued = fixture.action();
    assert!(matches!(
        fixture.repo.finish_task_action(
            &fixture.project,
            "source",
            "action-binding",
            Some("unrelated"),
            None
        ),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(fixture.action(), queued);
    fixture
        .repo
        .create_task(fixture.request("child", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
    let running = fixture.action();
    assert!(matches!(
        fixture.repo.finish_task_action(
            &fixture.project,
            "source",
            "action-binding",
            Some("child"),
            None
        ),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(fixture.action(), running);
    fixture.save_brief("child");
    fixture
        .repo
        .complete_task(
            &fixture.project,
            "child",
            "thread-binding",
            "draft done".into(),
            12,
        )
        .unwrap();
    let completed = fixture.action();
    assert_eq!(completed.status, "completed");
    assert_eq!(
        fixture
            .repo
            .finish_task_action(
                &fixture.project,
                "source",
                "action-binding",
                Some("child"),
                None
            )
            .unwrap(),
        completed
    );
    assert!(matches!(
        fixture.repo.finish_task_action(
            &fixture.project,
            "source",
            "action-binding",
            None,
            Some("late failure")
        ),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(fixture.action(), completed);
}

#[test]
fn failed_attempts_and_invalid_results_cannot_rewrite_action_evidence() {
    let mut fixture = Fixture::new("generateBrief", json!({"type":"artifact"}));
    let queued = fixture.action();
    for (result, error) in [
        (None, None),
        (Some("child"), Some("error")),
        (None, Some(" ")),
    ] {
        assert!(matches!(
            fixture.repo.finish_task_action(
                &fixture.project,
                "source",
                "action-binding",
                result,
                error
            ),
            Err(RepositoryError::TaskBindingInvalid)
        ));
        assert_eq!(fixture.action(), queued);
    }
    fixture
        .repo
        .create_task(fixture.request("child", "generateBrief", json!({"type":"artifact"})))
        .unwrap();
    fixture
        .repo
        .update_task(
            &fixture.project,
            "child",
            "thread-binding",
            TaskUpdate {
                status: "failed".into(),
                error: Some("original failure".into()),
                result_summary: None,
                now: 20,
            },
        )
        .unwrap();
    let failed = fixture.action();
    assert_eq!(failed.status, "failed");
    assert_eq!(
        fixture
            .repo
            .finish_task_action(
                &fixture.project,
                "source",
                "action-binding",
                None,
                Some("duplicate failure")
            )
            .unwrap(),
        failed
    );
    assert!(matches!(
        fixture.repo.finish_task_action(
            &fixture.project,
            "source",
            "action-binding",
            Some("child"),
            None
        ),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(fixture.action(), failed);
}
