use novel_codex_story_core::*;
use serde_json::{Value, json};
fn setup() -> (SqliteStoryRepository, ProjectId, String) {
    setup_in(SqliteStoryRepository::in_memory().unwrap())
}
fn setup_in(mut r:SqliteStoryRepository) -> (SqliteStoryRepository,ProjectId,String) {
    let p = ProjectId::parse("p").unwrap();
    r.create_project(
        ProjectRecord {
            project_id: p.clone(),
            title: "插问".into(),
            logline: "".into(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    r.conversation_store().create_conversation(
        &p,
        CreateConversationRequest {
            thread_id: "thread".into(),
            scope_id: None,
            title: "任务".into(),
            forked_from_id: None,
            last_turn_id: None,
        },
        1,
    )
    .unwrap();
    let digest = r.project_source_watermarks(&p).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let root = r
        .create_budgeted_root_task(
            task(&p, "root", json!({"type":"project"}), "原任务"),
            &InitialWorkBudget {
                authorization_id: "authority".into(),
                author_message_id: "author".into(),
                expected_base_snapshot_id: "initial".into(),
                expected_source_digest: digest,
                max_requests: Some(10),
                max_tokens: Some(1000),
                deadline_at: None,
            },
            1,
        )
        .unwrap();
    (r, p, root.task_id)
}
fn task(p: &ProjectId, id: &str, target: Value, text: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        task_id: id.into(),
        project_id: p.clone(),
        thread_id: "thread".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":text}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 9999999999,
    }
}
fn begin(work: &str, id: &str, parent: Option<&str>, auto: bool) -> BeginWorkInterlude {
    serde_json::from_value(json!({"id":id,"threadId":"thread","expectedControlRevision":0,"parentId":parent,"autoReturn":auto,"message":{"messageId":id,"input":{"text":"解释设定","focus":{"view":"chapters","readOnly":true},"connectionId":null},"trigger":{"kind":"interlude","interludeId":id,"workTaskId":work},"sessionId":"host"}})).unwrap()
}
fn ret(id: &str, automatic: bool) -> ReturnWorkInterlude {
    ReturnWorkInterlude {
        command_id: uuid::Uuid::new_v4().to_string(),
        interlude_id: id.into(),
        thread_id: "thread".into(),
        expected_control_revision: 0,
        expected_revision: 0,
        automatic,
    }
}
fn finish(r: &mut SqliteStoryRepository, p: &ProjectId, id: &str, requests: u64, tokens: u64) {
    r.complete_task(p, id, "thread", "答复".into(), None, 3)
        .unwrap();
    let grant = r.work_task_for_attempt(p, id).unwrap();
    let remaining_req = 10 - requests;
    let remaining_tokens = 1000 - tokens;
    r.charge_work_provider_usage(p,&grant.id,id,&json!({"runId":id,"budgetId":format!("budget-{id}"),"status":"completed","limits":{"maxRequests":remaining_req,"maxTokens":remaining_tokens,"maxDurationMs":null},"usage":{"requests":1,"tokens":25,"unknownTokens":0,"overrun":false}})).unwrap();
}
fn claim(r: &mut SqliteStoryRepository, p: &ProjectId, work_id: &str, id: &str, attempt: &str) {
    let message = r.conversation_store().change_conversation_message(
            p,
            "thread",
            id,
            serde_json::from_value(
                json!({"action":"claim","expectedRevision":0,"sessionId":"host"}),
            )
            .unwrap(),
        )
        .unwrap();
    let work = r.work_task(p, work_id).unwrap();
    let step = work
        .steps
        .iter()
        .find(|s| s.id == work.interlude.as_ref().unwrap().step_id)
        .unwrap();
    let mut target = step.target.clone();
    target["resumeStepId"] = json!(step.id);
    target["messageDispatch"] =
        json!({"messageId":id,"sessionId":"host","revision":message["revision"]});
    r.create_funded_task(task(p, attempt, target, "解释设定"), 2)
        .unwrap();
}
#[test]
fn original_steps_finish_then_read_only_question_uses_the_same_budget_and_manual_return() {
    let (mut r, p, root) = setup();
    let work = format!("work-{root}");
    let id = uuid::Uuid::new_v4().to_string();
    let b = begin(&work, &id, None, false);
    let w = r.begin_work_interlude(&p, &work, b.clone(), 2).unwrap();
    assert_eq!(w.status, "running");
    assert_eq!(w.interlude.unwrap().phase, "waiting");
    assert!(r.begin_work_interlude(&p, &work, b, 2).is_ok());
    assert_eq!(
        r.conversation_store().conversation_messages(&p, "thread").unwrap()[0]["triggerReady"],
        false
    );
    assert!(r.return_work_interlude(&p, &work, ret(&id, false)).is_err());
    finish(&mut r, &p, &root, 0, 0);
    assert_eq!(r.work_task(&p, &work).unwrap().status, "running");
    assert_eq!(
        r.conversation_store().conversation_messages(&p, "thread").unwrap()[0]["triggerReady"],
        true
    );
    claim(&mut r, &p, &work, &id, "question");
    let budget = r.task_provider_budget(&p, "question", 3).unwrap();
    assert_eq!(budget.work_task_id, work);
    assert_eq!(budget.max_requests, Some(9));
    assert_eq!(budget.max_tokens, Some(975));
    assert!(
        r.task_intent_store().append_task_intent(&p, "question", 0, "修改原任务", 3)
            .is_err()
    );
    finish(&mut r, &p, "question", 1, 25);
    assert_eq!(
        r.work_task(&p, &work).unwrap().interlude.unwrap().phase,
        "answered"
    );
    assert!(r.return_work_interlude(&p, &work, ret(&id, true)).is_err());
    let command = ret(&id, false);
    let returned = r.return_work_interlude(&p, &work, command.clone()).unwrap();
    assert!(returned.interlude.is_none());
    assert!(r.return_work_interlude(&p, &work, command.clone()).is_ok());
    let mut wrong = command;
    wrong.automatic = true;
    assert!(r.return_work_interlude(&p, &work, wrong).is_err());
}
#[test]
fn nested_return_only_closes_top_and_author_pause_invalidates_the_chain() {
    let (mut r, p, root) = setup();
    let work = format!("work-{root}");
    let outer = uuid::Uuid::new_v4().to_string();
    let inner = uuid::Uuid::new_v4().to_string();
    r.begin_work_interlude(&p, &work, begin(&work, &outer, None, false), 2)
        .unwrap();
    r.begin_work_interlude(&p, &work, begin(&work, &inner, Some(&outer), true), 2)
        .unwrap();
    assert!(
        r.return_work_interlude(&p, &work, ret(&outer, false))
            .is_err()
    );
    finish(&mut r, &p, &root, 0, 0);
    r.return_work_interlude(&p, &work, ret(&inner, false))
        .unwrap();
    assert_eq!(r.work_task(&p, &work).unwrap().interlude.unwrap().id, outer);
    let paused = r.control_work_task(&p, &work, "pause", 0, 4).unwrap();
    assert_eq!(paused.status, "paused");
    assert!(paused.interlude.is_none());
    assert!(
        r.return_work_interlude(&p, &work, ret(&outer, true))
            .is_err()
    );
    assert!(r.conversation_store().conversation_messages(&p, "thread").unwrap().is_empty());
}
#[test]
fn pause_during_question_revokes_attempt_and_never_requeues_it_on_resume() {
    let (mut r, p, root) = setup();
    let work = format!("work-{root}");
    let id = uuid::Uuid::new_v4().to_string();
    r.begin_work_interlude(&p, &work, begin(&work, &id, None, true), 2)
        .unwrap();
    finish(&mut r, &p, &root, 0, 0);
    claim(&mut r, &p, &work, &id, "question");
    let paused = r.control_work_task(&p, &work, "pause", 0, 4).unwrap();
    assert!(paused.interlude.is_none());
    assert_eq!(r.task(&p, "question").unwrap().unwrap().status, "cancelled");
    assert_eq!(
        paused
            .steps
            .iter()
            .find(|s| s.target["interludeId"] == id)
            .unwrap()
            .status,
        "superseded"
    );
    assert!(r.task_provider_budget(&p, "question", 5).is_err());
    assert!(r.return_work_interlude(&p, &work, ret(&id, true)).is_err());
}

#[test]
fn queued_question_edit_preserves_readonly_scope_and_cancellation_is_visible() {
    let (mut r,p,root)=setup();let work=format!("work-{root}");let id=uuid::Uuid::new_v4().to_string();
    r.begin_work_interlude(&p,&work,begin(&work,&id,None,false),2).unwrap();
    let mut ordinary=begin(&work,&uuid::Uuid::new_v4().to_string(),None,false).message;
    ordinary.trigger=MessageTrigger::Immediate;
    let queued=r.conversation_store().submit_conversation_message(&p,"thread",ordinary,2).unwrap();
    assert_eq!(queued["triggerReady"],false);
    for read_only in [false,true] {
        let result=r.conversation_store().change_conversation_message(&p,"thread",&id,serde_json::from_value(json!({"action":"edit","expectedRevision":0,"input":{"text":"解释人物设定","focus":{"view":"characters","readOnly":read_only},"connectionId":null}})).unwrap());
        assert_eq!(result.is_ok(),read_only);
    }
    let updated=r.work_task(&p,&work).unwrap();
    assert_eq!(updated.steps.last().unwrap().target["conversationFocus"]["view"],"characters");
    r.conversation_store().change_conversation_message(&p,"thread",&id,serde_json::from_value(json!({"action":"cancel","expectedRevision":1})).unwrap()).unwrap();
    assert_eq!(r.work_task(&p,&work).unwrap().interlude.unwrap().phase,"failed");
    finish(&mut r,&p,&root,0,0);
    r.return_work_interlude(&p,&work,ret(&id,false)).unwrap();
    assert!(r.conversation_store().conversation_messages(&p,"thread").unwrap().iter().any(|m|m["id"]==queued["id"] && m["triggerReady"]==true));
}

#[test]
fn reopening_preserves_hold_and_replaying_an_invalidated_question_never_reactivates_it(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("interlude.sqlite3");
 let(mut r,p,root)=setup_in(SqliteStoryRepository::open(&path).unwrap());let work=format!("work-{root}");
 let id=uuid::Uuid::new_v4().to_string();let input=begin(&work,&id,None,true);
 r.begin_work_interlude(&p,&work,input.clone(),2).unwrap();drop(r);
 let mut r=SqliteStoryRepository::open(&path).unwrap();
 assert_eq!(r.work_task(&p,&work).unwrap().interlude.unwrap().id,id);
 assert_eq!(r.conversation_store().conversation_message(&p,"thread",&id).unwrap()["sessionId"],"host");
 assert!(r.conversation_store().change_conversation_message(&p,"thread",&id,serde_json::from_value(json!({"action":"claim","expectedRevision":0,"sessionId":"new-host"})).unwrap()).is_err());
 r.control_work_task(&p,&work,"pause",0,4).unwrap();drop(r);
 let mut r=SqliteStoryRepository::open(&path).unwrap();
 let replay=r.begin_work_interlude(&p,&work,input,5).unwrap();
 assert_eq!(replay.status,"paused");assert!(replay.interlude.is_none());assert_eq!(replay.steps.len(),2);
 assert!(r.return_work_interlude(&p,&work,ret(&id,true)).is_err());
}

#[test]
fn exhausted_allowance_closes_completed_work_but_rolls_back_unfinished_parent_return() {
    for nested in [false, true] {
        let (mut r, p, root) = setup();
        let work = format!("work-{root}");
        let outer = uuid::Uuid::new_v4().to_string();
        let id = uuid::Uuid::new_v4().to_string();
        if nested {
            r.begin_work_interlude(&p, &work, begin(&work, &outer, None, false), 2).unwrap();
        }
        r.begin_work_interlude(&p, &work, begin(&work, &id, nested.then_some(outer.as_str()), true), 2).unwrap();
        finish(&mut r, &p, &root, 0, 0);
        claim(&mut r, &p, &work, &id, "question");
        r.complete_task(&p, "question", "thread", "回答完成".into(), None, 3).unwrap();
        r.charge_work_provider_usage(&p, &work, "question", &json!({
            "runId":"question","budgetId":"budget-question","status":"completed",
            "limits":{"maxRequests":9,"maxTokens":975,"maxDurationMs":null},
            "usage":{"requests":9,"tokens":25,"unknownTokens":0,"overrun":false}
        })).unwrap();
        let before = r.work_task(&p, &work).unwrap();
        let command = ret(&id, true);
        let result = r.return_work_interlude(&p, &work, command.clone());
        if nested {
            assert!(matches!(result, Err(RepositoryError::WorkUsageUnsettled)));
            let after = r.work_task(&p, &work).unwrap();
            assert_eq!(after.version, before.version);
            assert_eq!(after.interlude.unwrap().id, id);
        } else {
            let returned = result.unwrap();
            assert_eq!(returned.status, "completed");
            assert!(returned.interlude.is_none());
            assert_eq!(returned.steps.len(), before.steps.len());
            assert_eq!(r.return_work_interlude(&p, &work, command).unwrap().status, "completed");
        }
    }
}

fn proposed_action(source: &str, id: &str) -> TaskActionRequest {
    TaskActionRequest { id:id.into(), source_task_id:source.into(), operation:"generateBrief".into(),
        target:json!({"type":"artifact"}), instruction:"生成候选简报".into(), input_revision:0,
        intent_revision:0, status:"proposed".into(), result_task_id:None, error:None }
}

#[test]
fn interlude_preserves_parent_action_but_holds_dispatch_until_return() {
    let (mut r, p, root) = setup();
    let work = format!("work-{root}");
    let id = uuid::Uuid::new_v4().to_string();
    r.begin_work_interlude(&p, &work, begin(&work, &id, None, true), 2).unwrap();
    let action = r.record_task_action(&p, &root, "next", proposed_action(&root,"next")).unwrap();
    assert_eq!(action.status,"proposed");
    assert_eq!(r.record_task_action(&p,&root,"next",proposed_action(&root,"next")).unwrap(),action);
    assert_eq!(r.approve_task_action(&p,&root,"next",0,2).unwrap().status,"queued");
    let mut child = task(&p,"writer",json!({"type":"artifact","requestedActionId":"next","requestedByTaskId":root}),"生成候选简报");
    child.operation="generateBrief".into();
    assert_eq!(r.create_funded_task(child.clone(),2),Err(RepositoryError::TaskNotWritable));
    finish(&mut r,&p,&root,0,0);
    claim(&mut r,&p,&work,&id,"question");
    assert_eq!(r.record_task_action(&p,"question","forbidden",proposed_action("question","forbidden")),Err(RepositoryError::TaskOutputForbidden));
    finish(&mut r,&p,"question",1,25);
    assert_eq!(r.create_funded_task(child.clone(),3),Err(RepositoryError::TaskNotWritable));
    let returned=r.return_work_interlude(&p,&work,ret(&id,true)).unwrap();
    assert_eq!(returned.status,"running");
    assert!(returned.interlude.is_none());
    let created=r.create_funded_task(child,4).unwrap();
    assert_eq!(r.work_task_for_attempt(&p,&created.task_id).unwrap().id,work);
    let budget=r.task_provider_budget(&p,&created.task_id,4).unwrap();
    assert_eq!(budget.max_requests,Some(8));
    assert_eq!(budget.max_tokens,Some(950));
}

#[test]
fn explicit_pause_after_step_still_rejects_new_action_registration() {
    let (mut r,p,root)=setup();
    let work=format!("work-{root}");
    r.pause_work_after_step(&p,&work,0).unwrap();
    assert_eq!(r.record_task_action(&p,&root,"next",proposed_action(&root,"next")),Err(RepositoryError::TaskNotWritable));
    assert!(r.task_actions(&p,&root).unwrap().is_empty());
}

#[test]
fn skipping_unanswered_interlude_does_not_make_parent_action_depend_on_cancelled_question() {
    let (mut r,p,root)=setup();
    let work=format!("work-{root}");
    let id=uuid::Uuid::new_v4().to_string();
    r.begin_work_interlude(&p,&work,begin(&work,&id,None,false),2).unwrap();
    r.record_task_action(&p,&root,"next",proposed_action(&root,"next")).unwrap();
    r.approve_task_action(&p,&root,"next",0,2).unwrap();
    let w=r.work_task(&p,&work).unwrap();
    assert_eq!(w.steps.iter().find(|s|s.id=="next").unwrap().depends_on_step_id.as_deref(),Some(format!("step-{root}").as_str()));
    finish(&mut r,&p,&root,0,0);
    r.return_work_interlude(&p,&work,ret(&id,false)).unwrap();
    let mut child=task(&p,"writer",json!({"type":"artifact","requestedActionId":"next","requestedByTaskId":root}),"生成候选简报");
    child.operation="generateBrief".into();
    r.create_funded_task(child,4).unwrap();
    assert_eq!(r.task_provider_budget(&p,"writer",4).unwrap().max_requests,Some(9));
}

use novel_codex_story_core::{InitialWorkBudgetRepository, OrdinaryWorkBudgetRepository, ProjectRevisionRepository, TaskActionsRepository, TaskInputRepository, TaskProviderBudgetRepository, TasksRepository, WorkInterludesRepository, WorkPauseBarrierRepository, WorkTasksRepository};
