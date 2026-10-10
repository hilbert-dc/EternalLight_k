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
        "ready".into(), None,
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
    repo.control_work_task(&project(), &work.id, "resume", work.control_revision, /*now*/ 6)
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
            repo.control_work_task(&project(), &work.id, "resume", work.control_revision, /*now*/ 6);
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


#[test]
fn append_replay_keeps_the_original_source_without_reopening_paused_work() {
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("intent-replay.sqlite");
    let mut repo=SqliteStoryRepository::open(&path).unwrap();setup(&mut repo);
    let original=repo.task_intents(&project(),"source").unwrap()[0].clone();
    let work=repo.work_task_for_attempt(&project(),"source").unwrap();
    assert_eq!(repo.append_task_intent(&project(),"source",0,&original.author_text,99).unwrap(),original);
    assert_eq!(repo.work_task_for_attempt(&project(),"source").unwrap(),work);
    assert!(repo.append_task_intent(&project(),"source",0,"不同原文",99).is_err());
    repo.deliver_task_intent(&project(),"source",1,3).unwrap();
    let next=repo.append_task_intent(&project(),"source",1,"维持人物名字",4).unwrap();
    assert_eq!(repo.append_task_intent(&project(),"source",0,&original.author_text,100).unwrap().status,"delivered");
    let paused=repo.control_work_task(&project(),&work.id,"pause",work.control_revision,5).unwrap();
    drop(repo);let mut repo=SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(repo.append_task_intent(&project(),"source",1,&next.author_text,101).unwrap(),next);
    assert_eq!(repo.work_task_for_attempt(&project(),"source").unwrap(),paused);
    assert_eq!(repo.task_intents(&project(),"source").unwrap().len(),2);
    assert!(repo.append_task_intent(&project(),"source",2,"暂停后不得追加",102).is_err());
}


#[test]
fn proven_unsent_intent_pauses_and_resumes_in_original_work_after_reopen() {
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("unsent.sqlite");
    let mut repo=SqliteStoryRepository::open(&path).unwrap();setup(&mut repo);
    let work=repo.work_task_for_attempt(&project(),"source").unwrap();
    let receipt=repo.retain_unsent_task_intent(&project(),"source",1,3).unwrap();
    assert_eq!(receipt.status,"notSent");assert_eq!(receipt.delivered_at,None);
    let held=repo.work_task_for_attempt(&project(),"source").unwrap();
    assert_eq!(held.status,"paused");assert_eq!(held.id,work.id);
    assert_eq!(repo.retain_unsent_task_intent(&project(),"source",1,4).unwrap(),receipt);
    assert_eq!(repo.work_task_for_attempt(&project(),"source").unwrap().control_revision,held.control_revision);
    assert!(repo.deliver_task_intent(&project(),"source",1,4).is_err());
    drop(repo);let mut repo=SqliteStoryRepository::open(&path).unwrap();
    let resumed=repo.control_work_task(&project(),&held.id,"resume",held.control_revision,5).unwrap();
    let step=resumed.steps.iter().find(|s|s.status=="queued").unwrap();
    let mut next=request("continued","advisory",json!({"type":"project","resumeStepId":step.id}));
    next.thread_id="thread-source".into();
    let task=repo.create_task(next).unwrap();
    let manifest:Value=serde_json::from_str(&task.input_manifest_json).unwrap();
    assert_eq!(manifest["authorIntent"],"客户端旧指令\n\n作者追加要求：\n只改人物动机，保留结局");
    assert_eq!(manifest["authorIntentBinding"]["sourceTaskId"],"source");
    assert_eq!(repo.work_task_for_attempt(&project(),"continued").unwrap().id,work.id);
    assert_eq!(repo.task_intents(&project(),"source").unwrap()[0],receipt);
}

#[test]
fn delivered_intent_cannot_be_reclassified_as_unsent() {
    let mut repo=SqliteStoryRepository::in_memory().unwrap();setup(&mut repo);
    repo.deliver_task_intent(&project(),"source",1,3).unwrap();
    assert!(repo.retain_unsent_task_intent(&project(),"source",1,4).is_err());
    assert_eq!(repo.work_task_for_attempt(&project(),"source").unwrap().status,"running");
}


#[test]
fn unsent_receipt_does_not_reopen_author_cancelled_work() {
    let mut repo=SqliteStoryRepository::in_memory().unwrap();setup(&mut repo);
    let work=repo.work_task_for_attempt(&project(),"source").unwrap();
    let cancelled=repo.control_work_task(&project(),&work.id,"cancel",work.control_revision,3).unwrap();
    let receipt=repo.retain_unsent_task_intent(&project(),"source",1,4).unwrap();
    assert_eq!(receipt.status,"notSent");
    let after=repo.work_task_for_attempt(&project(),"source").unwrap();
    assert_eq!(after.status,"cancelled");assert_eq!(after.control_revision,cancelled.control_revision);
    assert!(repo.control_work_task(&project(),&work.id,"resume",after.control_revision,5).is_err());
}
