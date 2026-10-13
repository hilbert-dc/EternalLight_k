use novel_codex_story_core::{
    BranchId, ChangeConversationMessage, CreateConversationRequest, CreateTaskRequest, ProjectId,
    ProjectRecord, SnapshotId, SqliteStoryRepository, SubmitConversationMessage,
};
use serde_json::{Value, json};
fn seed(mut repo: SqliteStoryRepository) -> (SqliteStoryRepository, ProjectId) {
    let project = ProjectId::parse("message-project").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "Messages".into(),
            logline: "".into(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    for thread in ["thread-a", "thread-b"] {
        repo.conversation_store().create_conversation(
            &project,
            CreateConversationRequest {
                thread_id: thread.into(),
                scope_id: None,
                title: thread.into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            1,
        )
        .unwrap();
    }
    (repo, project)
}
fn submission(id: &str, trigger: Value) -> SubmitConversationMessage {
    serde_json::from_value(json!({"messageId":id,"input":{"text":"解释设定","focus":null,"connectionId":null},"trigger":trigger,"sessionId":"host-a"})).unwrap()
}
fn change(
    repo: &mut SqliteStoryRepository,
    p: &ProjectId,
    id: &str,
    body: Value,
) -> Result<Value, novel_codex_story_core::RepositoryError> {
    repo.conversation_store().change_conversation_message(
        p,
        "thread-a",
        id,
        serde_json::from_value::<ChangeConversationMessage>(body).unwrap(),
    )
}
#[test]
fn replay_after_reopen_does_not_duplicate_and_changed_input_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("messages.sqlite");
    let (mut repo, p) = seed(SqliteStoryRepository::open(&path).unwrap());
    let first = repo.conversation_store().submit_conversation_message(
            &p,
            "thread-a",
            submission("m1", json!({"kind":"immediate"})),
            1,
        )
        .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let mut retry = submission("m1", json!({"kind":"immediate"}));
    retry.session_id = "host-after-restart".into();
    assert_eq!(
        repo.conversation_store().submit_conversation_message(&p, "thread-a", retry, 2)
            .unwrap(),
        first
    );
    let mut changed = submission("m1", json!({"kind":"immediate"}));
    changed.input.text = "different".into();
    assert!(
        repo.conversation_store().submit_conversation_message(&p, "thread-a", changed, 3)
            .is_err()
    );
    assert_eq!(repo.conversation_store().conversation_messages(&p, "thread-a").unwrap().len(), 1);
    assert!(
        repo.conversation_store().submit_conversation_message(
            &p,
            "thread-b",
            submission("m1", json!({"kind":"immediate"})),
            4
        )
        .is_err()
    );
}
#[test]
fn claim_edit_cancel_are_cas_ordered_and_second_host_cannot_claim() {
    let (mut repo, p) = seed(SqliteStoryRepository::in_memory().unwrap());
    for id in ["m1", "m2"] {
        repo.conversation_store().submit_conversation_message(
            &p,
            "thread-a",
            submission(id, json!({"kind":"immediate"})),
            1,
        )
        .unwrap();
    }
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"claim","expectedRevision":0,"sessionId":"other-host"})
        )
        .is_err()
    );
    let claimed = change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"}),
    )
    .unwrap();
    assert_eq!(claimed["status"], "claimed");
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"cancel","expectedRevision":0})
        )
        .is_err()
    );
    assert!(change(&mut repo,&p,"m1",json!({"action":"edit","expectedRevision":1,"input":{"text":"changed","focus":null,"connectionId":null}})).is_err());
    assert!(
        change(
            &mut repo,
            &p,
            "m2",
            json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"})
        )
        .is_err()
    );
    let unknown = change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"fail","expectedRevision":1,"sessionId":"host-a","error":"response lost"}),
    )
    .unwrap();
    assert_eq!(unknown["status"], "claimed");
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"continue","expectedRevision":2,"sessionId":"host-a"})
        )
        .is_err()
    );
}
#[test]
fn after_work_waits_for_whole_work_and_control_changes_hold_the_message() {
    let (mut repo, p) = seed(SqliteStoryRepository::in_memory().unwrap());
    repo.create_task(CreateTaskRequest {
        task_id: "task-a".into(),
        project_id: p.clone(),
        thread_id: "thread-a".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: "{}".into(),
        input_manifest_json: json!({"authorIntent":"解释设定"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 100,
    })
    .unwrap();
    let work = repo.work_task(&p, "work-task-a").unwrap();
    repo.conversation_store().submit_conversation_message(&p,"thread-a",submission("m1",json!({"kind":"afterWork","workTaskId":work.id,"controlRevision":work.control_revision})),1).unwrap();
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"})
        )
        .is_err()
    );
    repo.control_work_task(&p, &work.id, "pause", work.control_revision, 2)
        .unwrap();
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"})
        )
        .is_err()
    );
    assert_eq!(
        repo.conversation_store().conversation_messages(&p, "thread-a").unwrap()[0]["status"],
        "queued"
    );
}
#[test]
fn unknown_dispatch_survives_reopen_and_never_becomes_ready() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unknown.sqlite");
    let (mut repo, p) = seed(SqliteStoryRepository::open(&path).unwrap());
    repo.conversation_store().submit_conversation_message(
        &p,
        "thread-a",
        submission("m1", json!({"kind":"immediate"})),
        1,
    )
    .unwrap();
    change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"}),
    )
    .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repo.conversation_store().conversation_messages(&p, "thread-a").unwrap()[0]["status"],
        "claimed"
    );
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"claim","expectedRevision":1,"sessionId":"host-after-restart"})
        )
        .is_err()
    );
}

#[test]
fn attempt_creation_and_message_binding_commit_together_and_cannot_bind_twice() {
    let (mut repo, p) = seed(SqliteStoryRepository::in_memory().unwrap());
    repo.conversation_store().submit_conversation_message(
        &p,
        "thread-a",
        submission("m1", json!({"kind":"immediate"})),
        1,
    )
    .unwrap();
    change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"}),
    )
    .unwrap();
    let request = |id: &str, text: &str| CreateTaskRequest {
        task_id: id.into(),
        project_id: p.clone(),
        thread_id: "thread-a".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json:
            json!({"messageDispatch":{"messageId":"m1","sessionId":"host-a","revision":1}})
                .to_string(),
        input_manifest_json: json!({"authorIntent":text}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 100,
    };
    assert!(
        repo.create_task(request("foreign", "changed text"))
            .is_err()
    );
    assert!(repo.task(&p, "foreign").unwrap().is_none());
    repo.create_task(request("original", "解释设定")).unwrap();
    let work=repo.work_task_for_attempt(&p,"original").unwrap();
    assert!(work.steps[0].target.get("messageDispatch").is_none(),"A retry must not claim the original delivered message again");
    let bound = repo.conversation_store().conversation_message(&p, "thread-a", "m1").unwrap();
    assert_eq!(bound["taskId"], "original");
    assert_eq!(bound["revision"], 2);
    assert!(repo.create_task(request("duplicate", "解释设定")).is_err());
    assert!(repo.task(&p, "duplicate").unwrap().is_none());
    change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"sending","expectedRevision":2,"sessionId":"host-a"}),
    )
    .unwrap();
    assert!(change(&mut repo,&p,"m1",json!({"action":"delivered","expectedRevision":3,"sessionId":"host-a","turnId":"unproven-turn"})).is_err());
}

#[test]
fn explicit_reconciliation_fences_an_unsent_claim_and_late_creation_is_rejected() {
    let (mut repo, p) = seed(SqliteStoryRepository::in_memory().unwrap());
    repo.conversation_store().submit_conversation_message(
        &p,
        "thread-a",
        submission("m1", json!({"kind":"immediate"})),
        1,
    )
    .unwrap();
    change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"}),
    )
    .unwrap();
    let result = change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"reconcile","expectedRevision":1}),
    )
    .unwrap();
    assert_eq!(result["status"], "queued");
    assert_eq!(result["requiresContinue"],true);
    assert!(change(&mut repo,&p,"m1",json!({"action":"claim","expectedRevision":2,"sessionId":"host-a"})).is_err());
    let late = repo.create_task(CreateTaskRequest {
        task_id: "late".into(),
        project_id: p.clone(),
        thread_id: "thread-a".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json:
            json!({"messageDispatch":{"messageId":"m1","sessionId":"host-a","revision":1}})
                .to_string(),
        input_manifest_json: json!({"authorIntent":"解释设定"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 100,
    });
    assert!(late.is_err());
    assert!(repo.task(&p, "late").unwrap().is_none());
    let continued=change(&mut repo,&p,"m1",json!({"action":"continue","expectedRevision":2,"sessionId":"host-a"})).unwrap();
    assert_eq!(continued["requiresContinue"],false);
    let claimed=change(&mut repo,&p,"m1",json!({"action":"claim","expectedRevision":3,"sessionId":"host-a"})).unwrap();
    assert_eq!(claimed["status"],"claimed");
}

#[test]
fn after_turn_keeps_the_original_control_revision_and_rejects_a_foreign_turn() {
    let (mut repo, p) = seed(SqliteStoryRepository::in_memory().unwrap());
    repo.create_task(CreateTaskRequest {
        task_id: "turn-task".into(),
        project_id: p.clone(),
        thread_id: "thread-a".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: "{}".into(),
        input_manifest_json: json!({"authorIntent":"解释设定"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 100,
    })
    .unwrap();
    repo.bind_task_turn(&p, "turn-task", "thread-a", "turn-a")
        .unwrap();
    let work = repo.work_task(&p, "work-turn-task").unwrap();
    assert!(repo.conversation_store().submit_conversation_message(&p,"thread-a",submission("wrong",json!({"kind":"afterTurn","taskId":"turn-task","turnId":"foreign","controlRevision":work.control_revision})),1).is_err());
    repo.conversation_store().submit_conversation_message(&p,"thread-a",submission("m1",json!({"kind":"afterTurn","taskId":"turn-task","turnId":"turn-a","controlRevision":work.control_revision})),1).unwrap();
    assert_eq!(
        repo.conversation_store().conversation_messages(&p, "thread-a").unwrap()[0]["triggerReady"],
        false
    );
    repo.control_work_task(&p, &work.id, "pause", work.control_revision, 2)
        .unwrap();
    assert!(
        change(
            &mut repo,
            &p,
            "m1",
            json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"})
        )
        .is_err()
    );
    let held = repo.conversation_store().conversation_messages(&p, "thread-a")
        .unwrap()
        .remove(0);
    assert_eq!(held["triggerReady"], false);
    assert_eq!(held["holdReason"], "authorControlChanged");
    let rearmed = change(
        &mut repo,
        &p,
        "m1",
        json!({"action":"continue","expectedRevision":0,"sessionId":"host-b"}),
    )
    .unwrap();
    assert_eq!(
        rearmed["trigger"]["controlRevision"],
        work.control_revision + 1
    );
    assert_eq!(rearmed["trigger"]["turnId"], "turn-a");
    assert_eq!(rearmed["holdReason"], "paused");
    assert_eq!(rearmed["triggerReady"], false);
    assert_eq!(repo.work_task(&p, &work.id).unwrap().status, "paused");
}

#[test]
fn queued_author_input_prevents_archiving_but_not_renaming() {
    let (mut repo, p) = seed(SqliteStoryRepository::in_memory().unwrap());
    repo.conversation_store().submit_conversation_message(
        &p,
        "thread-a",
        submission("m1", json!({"kind":"immediate"})),
        1,
    )
    .unwrap();
    let update = |title, archived| novel_codex_story_core::UpdateConversationRequest {
        expected_version: 0,
        title,
        is_pinned: None,
        is_archived: archived,
    };
    assert!(
        repo.conversation_store().update_conversation(&p, "thread-a", update(None, Some(true)), 2)
            .is_err()
    );
    assert!(
        repo.conversation_store().update_conversation(&p, "thread-a", update(Some("新名称".into()), None), 2)
            .is_ok()
    );
}

#[test]
fn pending_steer_is_queryable_after_reopen_but_never_requeued_or_delivered_by_queue_actions() {
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("steer.sqlite");
    let (mut repo,p)=seed(SqliteStoryRepository::open(&path).unwrap());
    repo.create_task(CreateTaskRequest{task_id:"steer-task".into(),project_id:p.clone(),thread_id:"thread-a".into(),operation:"advisory".into(),input_revision:0,model:None,target_json:json!({"type":"project"}).to_string(),input_manifest_json:json!({"authorIntent":"解释设定"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:100}).unwrap();
    let intent=repo.task_intent_store().append_task_intent(&p,"steer-task",0,"保留原结局",2).unwrap();
    drop(repo);let mut repo=SqliteStoryRepository::open(&path).unwrap();
    let messages=repo.conversation_store().conversation_messages(&p,"thread-a").unwrap();
    assert_eq!(messages.len(),1);assert_eq!(messages[0]["id"],intent.author_message_id);
    assert_eq!(messages[0]["trigger"]["kind"],"steer");assert_eq!(messages[0]["status"],"sending");
    assert_eq!(messages[0]["requiresContinue"],false);
    assert!(repo.conversation_store().conversation_message(&p,"thread-b",&intent.author_message_id).is_err());
    assert_eq!(change(&mut repo,&p,&intent.author_message_id,json!({"action":"reconcile","expectedRevision":0})).unwrap(),messages[0]);
    for action in ["continue","claim","delivered"] {
        let mut body=json!({"action":action,"expectedRevision":0,"sessionId":"host-a"});
        if action=="delivered"{body["turnId"]=json!("guessed-turn");}
        assert!(change(&mut repo,&p,&intent.author_message_id,body).is_err());
    }
    assert!(repo.conversation_store().submit_conversation_message(&p,"thread-a",submission(&intent.author_message_id,json!({"kind":"immediate"})),3).is_err());
    assert_eq!(repo.task_intent_store().task_intents(&p,"steer-task").unwrap().len(),1);
    repo.task_intent_store().deliver_task_intent(&p,"steer-task",1,3).unwrap();
    assert!(repo.conversation_store().conversation_messages(&p,"thread-a").unwrap().is_empty());
    assert_eq!(repo.conversation_store().conversation_message(&p,"thread-a",&intent.author_message_id).unwrap()["status"],"delivered");
}

#[test]
fn original_message_reconciles_only_a_proven_turn_in_its_own_resumed_step() {
    let (mut repo,p)=seed(SqliteStoryRepository::in_memory().unwrap());
    repo.conversation_store().submit_conversation_message(&p,"thread-a",submission("resumed-message",json!({"kind":"immediate"})),1).unwrap();
    change(&mut repo,&p,"resumed-message",json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"})).unwrap();
    let request=|id:&str,target:Value|CreateTaskRequest{task_id:id.into(),project_id:p.clone(),thread_id:"thread-a".into(),operation:"advisory".into(),input_revision:0,model:None,target_json:target.to_string(),input_manifest_json:json!({"authorIntent":"解释设定"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:100};
    repo.create_task(request("original",json!({"type":"project","messageDispatch":{"messageId":"resumed-message","sessionId":"host-a","revision":1}}))).unwrap();
    let work=repo.work_task_for_attempt(&p,"original").unwrap();
    let paused=repo.control_work_task(&p,&work.id,"pause",work.control_revision,2).unwrap();
    repo.control_work_task(&p,&work.id,"resume",paused.control_revision,3).unwrap();
    repo.create_task(request("unrelated",json!({"type":"project"}))).unwrap();
    repo.bind_task_turn(&p,"unrelated","thread-a","foreign-turn").unwrap();
    let check=|repo:&mut SqliteStoryRepository|{
        let prior=repo.conversation_store().conversation_message(&p,"thread-a","resumed-message").unwrap();
        change(repo,&p,"resumed-message",json!({"action":"reconcile","expectedRevision":prior["revision"]})).unwrap()
    };
    assert_eq!(check(&mut repo)["status"],"claimed");
    repo.create_task(request("resumed",json!({"type":"project","resumeStepId":work.steps[0].id}))).unwrap();
    assert_eq!(check(&mut repo)["status"],"claimed");
    repo.bind_task_turn(&p,"resumed","thread-a","resumed-turn").unwrap();
    let delivered=check(&mut repo);
    assert_eq!(delivered["status"],"delivered");assert_eq!(delivered["taskId"],"resumed");assert_eq!(delivered["turnId"],"resumed-turn");
    assert!(change(&mut repo,&p,"resumed-message",json!({"action":"sending","expectedRevision":2,"sessionId":"host-a"})).is_err());
}

use novel_codex_story_core::{ProjectRevisionRepository, TasksRepository, WorkTasksRepository};
