use novel_codex_story_core::{
    BranchId, CreateTaskRequest, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository, TaskActionRequest, TaskRecord, TaskUpdate,
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn project() -> ProjectId {
    ProjectId::parse("intent-project").unwrap()
}

fn request(id: &str, operation: &str, target: Value) -> CreateTaskRequest {
    CreateTaskRequest {
        task_id: id.into(),
        project_id: project(),
        thread_id: format!("thread-{id}"),
        operation: operation.into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":"客户端旧指令"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 1000,
    }
}

fn setup(repo: &mut SqliteStoryRepository) {
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "意图恢复".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.create_task(request("source", "advisory", json!({"type":"project"})))
        .unwrap();
    repo.append_task_intent(
        &project(),
        "source",
        /*expected_intent_revision*/ 0,
        "只改人物动机，保留结局",
        /*now*/ 2,
    )
    .unwrap();
}

fn fail(repo: &mut SqliteStoryRepository, task: &TaskRecord) {
    repo.update_task(
        &project(),
        &task.task_id,
        &task.thread_id,
        TaskUpdate {
            status: "failed".into(),
            error: Some("interrupted".into()),
            result_summary: None,
            now: 5,
        },
    )
    .unwrap();
}

#[test]
fn candidate_attempts_reconstruct_author_history_after_reopen_and_ignore_client_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("intents.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    setup(&mut repo);
    repo.deliver_task_intent(
        &project(),
        "source",
        /*intent_revision*/ 1,
        /*now*/ 3,
    )
    .unwrap();
    repo.record_task_action(
        &project(),
        "source",
        "action-key",
        TaskActionRequest {
            id: "action".into(),
            source_task_id: "source".into(),
            operation: "generateBrief".into(),
            target: json!({"type":"artifact"}),
            instruction: "把复仇改为寻找证词".into(),
            input_revision: 0,
            intent_revision: 1,
            status: "proposed".into(),
            result_task_id: None,
            error: None,
        },
    )
    .unwrap();
    repo.complete_task(
        &project(),
        "source",
        "thread-source",
        "ready".into(),
        /*now*/ 4,
    )
    .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.approve_task_action(&project(), "source", "action", 1, 4)
        .unwrap();
    let target =
        json!({"type":"artifact","requestedByTaskId":"source","requestedActionId":"action"});
    let child = repo
        .create_task(request("child", "generateBrief", target.clone()))
        .unwrap();
    let expected = json!({
        "authorIntent":"客户端旧指令\n\n作者追加要求：\n只改人物动机，保留结局",
        "candidateInstruction":"把复仇改为寻找证词",
        "authorIntentBinding":{"sourceTaskId":"source","intentRevision":1,"actionId":"action"}
    });
    let first: Value = serde_json::from_str(&child.input_manifest_json).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(&first[key], value);
    }
    fail(&mut repo, &child);
    let work = repo.work_task_for_attempt(&project(), "child").unwrap();
    repo.control_work_task(&project(), &work.id, "resume", work.version, /*now*/ 6)
        .unwrap();
    let mut retry = request("retry", "generateBrief", target);
    retry.input_manifest_json = json!({"authorIntent":"篡改为重写结局"}).to_string();
    let retry = repo.create_task(retry).unwrap();
    let second: Value = serde_json::from_str(&retry.input_manifest_json).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(&second[key], value);
    }
}

#[test]
fn advisory_retry_inherits_delivered_instructions_but_pending_delivery_blocks_resume() {
    for delivered in [false, true] {
        let mut repo = SqliteStoryRepository::in_memory().unwrap();
        setup(&mut repo);
        if delivered {
            repo.deliver_task_intent(
                &project(),
                "source",
                /*intent_revision*/ 1,
                /*now*/ 3,
            )
            .unwrap();
            repo.record_task_action(
                &project(),
                "source",
                "old-action",
                TaskActionRequest {
                    id: "old-action".into(),
                    source_task_id: "source".into(),
                    operation: "generateBrief".into(),
                    target: json!({"type":"artifact"}),
                    instruction: "旧候选".into(),
                    input_revision: 0,
                    intent_revision: 1,
                    status: "proposed".into(),
                    result_task_id: None,
                    error: None,
                },
            )
            .unwrap();
        }
        let source = repo.task(&project(), "source").unwrap().unwrap();
        fail(&mut repo, &source);
        let work = repo.work_task_for_attempt(&project(), "source").unwrap();
        let resumed =
            repo.control_work_task(&project(), &work.id, "resume", work.version, /*now*/ 6);
        let retry = request(
            "retry",
            "advisory",
            json!({"type":"project","resumeStepId":"step-source"}),
        );
        if delivered {
            resumed.unwrap();
            let retry = repo.create_task(retry).unwrap();
            let manifest: Value = serde_json::from_str(&retry.input_manifest_json).unwrap();
            assert_eq!(
                manifest["authorIntent"],
                "客户端旧指令\n\n作者追加要求：\n只改人物动机，保留结局"
            );
            assert_eq!(
                repo.task_actions(&project(), "source").unwrap()[0].status,
                "superseded"
            );
        } else {
            assert_eq!(work.resume_block_reason.as_deref(), Some("inputChanged"));
            assert!(matches!(resumed, Err(RepositoryError::TaskInputStale)));
            assert!(matches!(
                repo.create_task(retry),
                Err(RepositoryError::TaskInputStale)
            ));
            assert!(repo.task(&project(), "retry").unwrap().is_none());
        }
    }
}

#[test]
fn cumulative_author_budget_is_checked_before_pending_intent_or_step_mutation() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    setup(&mut repo);
    repo.deliver_task_intent(
        &project(),
        "source",
        /*intent_revision*/ 1,
        /*now*/ 3,
    )
    .unwrap();
    repo.append_task_intent(
        &project(),
        "source",
        /*expected_intent_revision*/ 1,
        &"甲".repeat(5000),
        /*now*/ 4,
    )
    .unwrap();
    repo.deliver_task_intent(
        &project(),
        "source",
        /*intent_revision*/ 2,
        /*now*/ 5,
    )
    .unwrap();
    let before = repo.work_task_for_attempt(&project(), "source").unwrap();
    let history = repo.task_intents(&project(), "source").unwrap();
    assert!(matches!(
        repo.append_task_intent(
            &project(),
            "source",
            /*expected_intent_revision*/ 2,
            &"乙".repeat(4000),
            /*now*/ 6
        ),
        Err(RepositoryError::InvalidContent(_))
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project(), "source").unwrap(),
        before
    );
    assert_eq!(repo.task_intents(&project(), "source").unwrap(), history);
}
