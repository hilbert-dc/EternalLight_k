use novel_codex_story_core::{
    BranchId, ChapterProjection, CommitId, CreateTaskRequest, GenerationHandoffRequest,
    IdempotencyKey, InitialWorkBudget, ObjectId, ProjectId, ProjectRecord, RevisionId,
    SaveCandidateRequest, SnapshotId, SqliteStoryRepository, TaskActionRequest, TaskOutputBinding,
    TaskRecord, UpdateModelWorkPlan, WorkDecisionRequest,
};
use serde_json::{Value, json};
fn fixture() -> (SqliteStoryRepository, TaskRecord) {
    fixture_with_source(false)
}
fn fixture_with_source(saved: bool) -> (SqliteStoryRepository, TaskRecord) {
    fixture_with_intent(saved, "先校正大纲再续写")
}
fn fixture_with_intent(saved: bool, author_intent: &str) -> (SqliteStoryRepository, TaskRecord) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = ProjectId::parse("project").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: p.clone(),
            title: "计划".into(),
            logline: "".into(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    if saved {
        repo.save_candidate(SaveCandidateRequest {
            project_id: p.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("artifact-brief").unwrap(),
            object_kind: "brief".into(),
            revision_id: RevisionId::parse("brief-existing").unwrap(),
            commit_id: CommitId::parse("brief-commit").unwrap(),
            idempotency_key: IdempotencyKey::parse("brief-save").unwrap(),
            request_digest: "brief".into(),
            content_digest: "brief".into(),
            content_json: json!({"premise":"东方幻想悬疑"}).to_string(),
            expected_project_revision: 0,
            expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
            next_snapshot_id: SnapshotId::parse("source-snapshot").unwrap(),
            expected_candidate_revision_id: None,
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: vec![],
        })
        .unwrap();
    }
    let digest = repo.project_source_watermarks(&p).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let t = repo
        .create_budgeted_root_task(
            CreateTaskRequest {
                task_id: "root".into(),
                project_id: p,
                thread_id: "thread".into(),
                operation: "advisory".into(),
                input_revision: 0,
                model: None,
                target_json: json!({"type":"project"}).to_string(),
                input_manifest_json: json!({"authorIntent":author_intent}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: 99999,
            },
            &InitialWorkBudget {
                authorization_id: "budget".into(),
                author_message_id: "message".into(),
                expected_base_snapshot_id: if saved { "source-snapshot" } else { "initial" }.into(),
                expected_source_digest: digest,
                max_requests: None,
                max_tokens: None,
                deadline_at: None,
            },
            1,
        )
        .unwrap();
    (repo, t)
}
fn goal(id: &str) -> Value {
    json!({"id":id,"title":"校正大纲","acceptance":"保存修订并核对时间线","kind":"deliverable","status":"pending","dependsOn":[],"readingQuestion":"核对原章节日次","checkScope":"相关章节与大纲","approach":"局部修订","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null})
}
fn request(
    version: i64,
    key: &str,
    goals: Vec<Value>,
    _active: Option<&str>,
) -> UpdateModelWorkPlan {
    request_mode(version, key, goals, "executeGoals")
}
fn request_mode(version: i64, key: &str, goals: Vec<Value>, mode: &str) -> UpdateModelWorkPlan {
    serde_json::from_value(
        json!({"expectedPlanVersion":version,"idempotencyKey":key,"plan":{"intentMode":mode,"goals":goals}}),
    )
    .unwrap()
}
fn saved_plan(repo: &SqliteStoryRepository, task: &TaskRecord) -> Value {
    let index = repo.read_model_work_plan(task, None).unwrap();
    let goals = index["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|goal| {
            repo.read_model_work_plan(task, goal["id"].as_str())
                .unwrap()
        })
        .collect::<Vec<_>>();
    json!({"index":index,"goals":goals})
}
fn next_decision(repo: &mut SqliteStoryRepository, origin: &TaskRecord, id: &str) -> TaskRecord {
    settle_fixture_attempt(repo, origin);
    let work = repo
        .work_task_for_attempt(&origin.project_id, &origin.task_id)
        .unwrap();
    let digest = repo.project_source_watermarks(&origin.project_id).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let step = repo
        .prepare_work_decision(
            &origin.project_id,
            &work.id,
            &WorkDecisionRequest {
                expected_work_version: work.version,
                expected_revision: origin.input_revision,
                expected_source_digest: digest,
            },
        )
        .unwrap()
        .step
        .unwrap();
    let mut target = step.target;
    target["resumeStepId"] = json!(step.id);
    repo.create_task(CreateTaskRequest {
        task_id: id.into(),
        project_id: origin.project_id.clone(),
        thread_id: origin.thread_id.clone(),
        operation: "advisory".into(),
        input_revision: origin.input_revision,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":"协调当前作者要求"}).to_string(),
        capability_hash: "c".repeat(64),
        capability_expires_at: 99999,
    })
    .unwrap()
}
fn settle_fixture_attempt(repo: &mut SqliteStoryRepository, task: &TaskRecord) {
    let work = repo
        .work_task_for_attempt(&task.project_id, &task.task_id)
        .unwrap();
    if let Some(pending) = repo
        .pending_work_provider_usage(&task.project_id, &work.id)
        .unwrap()
    {
        assert_eq!(pending.status, "completed");
        // This is a trusted usage ledger fixture, not a provider invocation.
        repo.charge_work_provider_usage(&task.project_id,&work.id,&pending.task_id,
            &json!({"runId":pending.task_id,"budgetId":format!("budget-{}",pending.task_id),"status":"completed",
                "limits":{"maxRequests":null,"maxTokens":null,"maxDurationMs":null},
                "usage":{"requests":1,"tokens":1,"unknownTokens":0,"overrun":false}})).unwrap();
    }
}
fn paused_original(repo: &mut SqliteStoryRepository, t: &TaskRecord) -> Value {
    let digest = repo.project_source_watermarks(&t.project_id).unwrap()["digest"].clone();
    let mut state = json!({"id":"generation-original","projectId":t.project_id.as_str(),"version":0,"status":"running","stage":"coordinate","rootThreadId":t.thread_id,"authorMessageId":"message-run","intentGeneration":0,"feedbackDecisions":[],"intentRevisions":[{"id":"intent","generation":0,"instruction":"续写原范围","effectiveFromChapter":1,"source":{"type":"authorMessage","authorMessageId":"message-run","createdAt":1}}],"sourceDigest":digest,"baseSnapshotId":"initial","startChapterNumber":1,"completedChapters":0,"chapterDeliveries":[],"targetChapters":2,"maxTaskAttempts":7,"usedTaskAttempts":0,"deadlineAt":null,"activeTaskId":null,"currentChapterId":null,"currentChapterNumber":null,"providerBudget":{"maxRequests":10,"maxTokens":100,"usedRequests":0,"usedTokens":0,"unknownTokens":0,"overrun":false,"charges":[]}});
    repo.create_generation_run(&t.project_id, "generation-original", &state.to_string())
        .unwrap();
    state["version"] = json!(1);
    state["status"] = json!("paused");
    repo.update_generation_run(&t.project_id, "generation-original", 0, &state.to_string())
        .unwrap();
    state
}
fn continuation_goal() -> Value {
    let mut g = goal("resume");
    g["kind"] = json!("continuation");
    g["title"] = json!("接回原创作范围");
    g["acceptance"] = json!("登记原任务的真实接续交接");
    g["continuation"] = json!({"runId":"generation-original","proposalId":null});
    g
}
fn review(status: &Value, goal_status: &str) -> Value {
    json!({"expectedWorkVersion":status["version"],"expectedSourceDigest":status["sourceDigest"],"goalStatus":goal_status,"goalAssessment":"已根据当前事实核对本轮交付","checkedScope":"原任务与本轮计划","unresolved":if goal_status=="partial"{json!([{"issue":{"issueId":"blocked-original","newIssue":{"summary":"原任务仍存在无法恢复的障碍"},"resolutionOf":[]},"observationId":null,"reason":"原任务仍存在无法恢复的障碍"}])}else{json!([])},"issueResolutions":[],"authorDecisions":[],"evidence":[]})
}

#[test]
fn simple_candidate_and_scope_proposal_need_no_method_plan_and_do_not_execute() {
    let (mut repo, t) = fixture_with_intent(false, "校正大纲并提出两章创作范围，待作者批准");
    let work_before = repo
        .work_task_for_attempt(&t.project_id, &t.task_id)
        .unwrap();
    let budget_before = serde_json::to_value(
        repo.work_budget_authorization(&t.project_id, &work_before.id)
            .unwrap(),
    )
    .unwrap();
    let scope_before = repo.execution_scope(&t.project_id, &t.task_id).unwrap();
    let action = TaskActionRequest {
        id: "fix-outline".into(),
        source_task_id: t.task_id.clone(),
        operation: "generateOutline".into(),
        target: json!({"type":"artifact"}),
        instruction: "校正大纲".into(),
        input_revision: t.input_revision,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    repo.record_task_action(&t.project_id, &t.task_id, "fix", action)
        .unwrap();
    let proposal = repo
        .propose_generation_handoff(
            &t,
            &GenerationHandoffRequest {
                target_chapters: 2,
                resume_run: None,
                instruction: "提出两章范围".into(),
                expected_intent_revision: 0,
                idempotency_key: "scope".into(),
            },
        )
        .unwrap();
    assert_eq!(proposal["state"], "proposed");
    assert_eq!(repo.read_model_work_plan(&t, None).unwrap()["version"], 0);
    let actions = repo.task_actions(&t.project_id, &t.task_id).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].status, "proposed");
    assert!(actions[0].result_task_id.is_none());
    let work_after = repo
        .work_task_for_attempt(&t.project_id, &t.task_id)
        .unwrap();
    assert_eq!(work_after.steps.len(), work_before.steps.len());
    assert_eq!(
        serde_json::to_value(
            repo.work_budget_authorization(&t.project_id, &work_after.id)
                .unwrap()
        )
        .unwrap(),
        budget_before
    );
    assert_eq!(
        repo.execution_scope(&t.project_id, &t.task_id).unwrap(),
        scope_before
    );
}

#[test]
fn simple_readonly_answer_needs_no_plan_and_method_mode_cannot_close_delivery() {
    let (mut repo, t) = fixture_with_intent(false, "仅说明目前创作进展，不执行任何行动");
    repo.complete_task(
        &t.project_id,
        &t.task_id,
        &t.thread_id,
        "当前进展已说明".into(),
        None,
        2,
    )
    .unwrap();
    assert_eq!(
        repo.work_task_for_attempt(&t.project_id, &t.task_id)
            .unwrap()
            .status,
        "completed"
    );
    let (mut repo, t) = fixture_with_intent(false, "修订大纲并核对");
    repo.update_model_work_plan(&t, &request(0, "method", vec![goal("repair")], None))
        .unwrap();
    assert_eq!(
        repo.task_work_decision(&t, None).unwrap()["deliveryReviewRequired"],
        true
    );
    // A new method mode is descriptive. It cannot finish the actual author Work.
    repo.update_model_work_plan(&t, &request_mode(1, "retire", vec![], "planningOnly"))
        .unwrap();
    let current = repo.task_work_decision(&t, None).unwrap();
    assert_eq!(current["deliveryReviewRequired"], true);
    assert_eq!(current["workPlan"]["pendingGoals"], 0);
    assert!(
        repo.complete_task(
            &t.project_id,
            &t.task_id,
            &t.thread_id,
            "计划已经改完，所以作者任务已完成".into(),
            None,
            3
        )
        .is_err()
    );
    assert_eq!(
        repo.task(&t.project_id, &t.task_id)
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );
}

#[test]
fn retiring_methods_cannot_cancel_a_registered_required_output() {
    let (mut repo, task) = fixture();
    repo.update_model_work_plan(&task, &request(0, "method", vec![goal("outline")], None))
        .unwrap();
    repo.record_task_action(
        &task.project_id,
        &task.task_id,
        "outline-action",
        TaskActionRequest {
            id: "required-outline".into(),
            source_task_id: task.task_id.clone(),
            operation: "generateOutline".into(),
            target: json!({"type":"artifact"}),
            instruction: "保存修订后的大纲".into(),
            input_revision: task.input_revision,
            intent_revision: 0,
            status: "proposed".into(),
            result_task_id: None,
            error: None,
        },
    )
    .unwrap();
    let before = repo.task_actions(&task.project_id, &task.task_id).unwrap();
    repo.update_model_work_plan(&task, &request_mode(1, "retire", vec![], "planningOnly"))
        .unwrap();
    let after = repo.task_actions(&task.project_id, &task.task_id).unwrap();
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    repo.complete_task(
        &task.project_id,
        &task.task_id,
        &task.thread_id,
        "已登记修订候选".into(),
        None,
        3,
    )
    .unwrap();
    let decision = next_decision(&mut repo, &task, "pending-output-coordinator");
    let status = repo.task_work_decision(&decision, None).unwrap();
    assert_eq!(status["canFinish"], false);
    assert!(
        status["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "requestedOutput" && r["status"] == "pending")
    );
    assert!(
        repo.complete_task(
            &task.project_id,
            &decision.task_id,
            &task.thread_id,
            "撤销计划后所有成果都完成了".into(),
            Some(review(&status, "fulfilled")),
            3
        )
        .is_err()
    );
    assert_eq!(
        repo.task(&task.project_id, &decision.task_id)
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );
}

#[test]
fn an_empty_method_window_cannot_waive_independent_assessment_of_a_real_saved_chapter() {
    let (mut repo, task) =
        fixture_with_intent(false, "保存本章候选并独立核对时间线，保留既定写作范围");
    repo.update_model_work_plan(
        &task,
        &request(0, "writer-method", vec![goal("write")], None),
    )
    .unwrap();
    let target = json!({"type":"chapter","chapterId":"chapter-current","chapterNumber":1,"readingChapter":1,"sceneOrder":1,"scope":{"type":"throughScene","endSceneOrder":1},"chapterAcceptance":{"requirements":[{"id":"whole-chapter","text":"时间线与作者要求一致"}],"length":{"requirement":"完整本章，不限制精确长度","minCharacters":null,"maxCharacters":null}}});
    repo.record_task_action(
        &task.project_id,
        &task.task_id,
        "write-action",
        TaskActionRequest {
            id: "required-chapter".into(),
            source_task_id: task.task_id.clone(),
            operation: "generateChapter".into(),
            target,
            instruction: "保存章节候选并核对时间线".into(),
            input_revision: 0,
            intent_revision: 0,
            status: "proposed".into(),
            result_task_id: None,
            error: None,
        },
    )
    .unwrap();
    repo.update_model_work_plan(
        &task,
        &request_mode(1, "new-window", vec![], "planningOnly"),
    )
    .unwrap();
    repo.complete_task(
        &task.project_id,
        &task.task_id,
        &task.thread_id,
        "已登记写作候选".into(),
        None,
        2,
    )
    .unwrap();
    settle_fixture_attempt(&mut repo, &task);
    let action = repo
        .approve_task_action(&task.project_id, &task.task_id, "required-chapter", 0, 3)
        .unwrap();
    let mut writer_target = action.target;
    writer_target["requestedByTaskId"] = json!(task.task_id);
    writer_target["requestedActionId"] = json!("required-chapter");
    let writer = repo.create_task(CreateTaskRequest {
        task_id:"writer".into(), project_id:task.project_id.clone(), thread_id:task.thread_id.clone(),
        operation:"generateChapter".into(),input_revision:0,model:None,target_json:writer_target.to_string(),
        input_manifest_json:json!({"authorIntent":"保存本章候选并独立核对时间线，保留既定写作范围"}).to_string(),
        capability_hash:"b".repeat(64),capability_expires_at:99999,
    }).unwrap();
    let body = "夜已过三更，林夕把昨日的账页放回木匣。";
    repo.save_candidate(SaveCandidateRequest {
        project_id:task.project_id.clone(),branch_id:BranchId::parse("main").unwrap(),
        object_id:ObjectId::parse("draft-chapter-current").unwrap(),object_kind:"draft".into(),
        revision_id:RevisionId::parse("draft-current").unwrap(),commit_id:CommitId::parse("draft-save").unwrap(),
        idempotency_key:IdempotencyKey::parse("draft-save").unwrap(),request_digest:"draft".into(),content_digest:"draft".into(),
        expected_project_revision:0,expected_snapshot_id:SnapshotId::parse("initial").unwrap(),
        next_snapshot_id:SnapshotId::parse("draft-snapshot").unwrap(),expected_candidate_revision_id:None,
        content_json:json!({"chapterId":"chapter-current","chapterNumber":1,"title":"账页","body":body,"sceneGroups":[{"id":"scene-one","order":1,"title":"账页","body":body,"locked":false}],"sourcePlanRefs":[],"version":1,"completeness":"chapterComplete"}).to_string(),
        chapter_projection:Some(ChapterProjection{chapter_id:"chapter-current".into(),number:1,title:"账页".into()}),
        task_output:Some(TaskOutputBinding{task_id:writer.task_id.clone(),thread_id:task.thread_id.clone(),output_kind:"draft".into(),completeness:Some("chapterComplete".into())}),
        linked_impact_ids:vec![],
    }).unwrap();
    repo.complete_task(
        &task.project_id,
        &writer.task_id,
        &task.thread_id,
        "候选已保存".into(),
        None,
        4,
    )
    .unwrap();
    let decision = next_decision(&mut repo, &task, "coordinator");
    let status = repo.task_work_decision(&decision, None).unwrap();
    assert_eq!(status["workPlan"]["intentMode"], "planningOnly");
    assert_eq!(status["canFinish"], false);
    assert!(
        status["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "revisionOutcome" && r["status"] == "pending")
    );
    assert!(
        status["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "assessRevisionOutcome" && a["required"] == true)
    );
    assert!(
        repo.complete_task(
            &task.project_id,
            &decision.task_id,
            &task.thread_id,
            "已存即通过".into(),
            Some(review(&status, "fulfilled")),
            5
        )
        .is_err()
    );
}

#[test]
fn planning_only_future_continuation_is_a_finished_plan_without_dispatch() {
    let (mut repo, t) =
        fixture_with_intent(false, "本轮只保存未来接续计划，不执行写作、不恢复原任务");
    let original = paused_original(&mut repo, &t);
    repo.update_model_work_plan(
        &t,
        &request_mode(0, "future-plan", vec![continuation_goal()], "planningOnly"),
    )
    .unwrap();
    let status = repo.task_work_decision(&t, None).unwrap();
    assert_eq!(status["workPlan"]["pendingGoals"], 0);
    assert_eq!(status["workPlan"]["intentMode"], "planningOnly");
    assert_eq!(status["deliveryReviewRequired"], false);
    repo.complete_task(
        &t.project_id,
        &t.task_id,
        &t.thread_id,
        "未来计划已保存，本轮结束".into(),
        None,
        2,
    )
    .unwrap();
    assert_eq!(
        repo.work_task_for_attempt(&t.project_id, &t.task_id)
            .unwrap()
            .status,
        "completed"
    );
    let stored: Value = serde_json::from_str(
        &repo
            .generation_run(&t.project_id, "generation-original")
            .unwrap()
            .state_json,
    )
    .unwrap();
    assert_eq!(stored, original);
}

#[test]
fn mutable_continuation_method_does_not_erase_real_handoff_or_required_delivery_review() {
    let (mut repo, t) =
        fixture_with_intent(false, "接回同一对话中已经暂停的原任务，保持原范围与预算");
    let original = paused_original(&mut repo, &t);
    let mut g = continuation_goal();
    repo.update_model_work_plan(&t, &request(0, "execute", vec![g.clone()], None))
        .unwrap();
    assert_eq!(
        repo.task_work_decision(&t, None).unwrap()["deliveryReviewRequired"],
        true
    );
    let proposal = repo
        .propose_generation_handoff(
            &t,
            &GenerationHandoffRequest {
                target_chapters: 2,
                resume_run: Some(json!({"id":"generation-original","expectedVersion":1})),
                instruction: "回到原范围继续创作".into(),
                expected_intent_revision: 0,
                idempotency_key: "handoff".into(),
            },
        )
        .unwrap();
    g["status"] = json!("satisfied");
    g["assessment"] = json!("接续交接已经真实登记，剩余章节由原任务继续");
    g["continuation"]["proposalId"] = proposal["proposalId"].clone();
    repo.update_model_work_plan(&t, &request(1, "record-proof", vec![g], None))
        .unwrap();
    // Retiring the method does not cancel registered transfer or minimum review.
    repo.update_model_work_plan(&t, &request_mode(2, "new-method", vec![], "planningOnly"))
        .unwrap();
    let status = repo.task_work_decision(&t, None).unwrap();
    assert_eq!(status["deliveryReviewRequired"], true);
    assert_eq!(
        repo.generation_handoff(&t.project_id, &t.task_id)
            .unwrap()
            .unwrap(),
        proposal
    );
    assert!(
        repo.complete_task(
            &t.project_id,
            &t.task_id,
            &t.thread_id,
            "交接完成".into(),
            None,
            3
        )
        .is_err()
    );
    let stored: Value = serde_json::from_str(
        &repo
            .generation_run(&t.project_id, "generation-original")
            .unwrap()
            .state_json,
    )
    .unwrap();
    assert_eq!(stored, original);
    let old = repo
        .read_model_work_plan_version(&t, 2, Some("resume"))
        .unwrap();
    assert_eq!(old["historicalReferenceOnly"], true);
    assert_eq!(
        old["goal"]["continuation"]["proposalId"],
        proposal["proposalId"]
    );
    assert!(old["evidenceCurrent"].is_null());
}

#[test]
fn model_work_plan_is_persisted_versioned_idempotent_and_does_not_change_work_allowance() {
    let (mut repo, t) = fixture();
    let before = repo.task_work_decision(&t, None).unwrap();
    let r = request(
        0,
        "first",
        vec![goal("outline"), {
            let mut g = goal("write");
            g["dependsOn"] = json!(["outline"]);
            g
        }],
        None,
    );
    let receipt = repo.update_model_work_plan(&t, &r).unwrap();
    assert_eq!(receipt["version"], 1);
    assert_eq!(repo.update_model_work_plan(&t, &r).unwrap(), receipt);
    let list = repo.read_model_work_plan(&t, None).unwrap();
    assert_eq!(list["goals"].as_array().unwrap().len(), 2);
    assert_eq!(
        repo.read_model_work_plan(&t, Some("outline")).unwrap()["goal"]["readingQuestion"],
        "核对原章节日次"
    );
    assert_eq!(
        repo.task_work_decision(&t, None).unwrap()["conversationRuns"],
        before["conversationRuns"]
    );
    assert!(
        repo.update_model_work_plan(&t, &request(0, "stale", vec![goal("outline")], None))
            .is_err()
    );
    assert!(
        repo.update_model_work_plan(&t, &request(1, "first", vec![goal("outline")], None))
            .is_err()
    );
    let mut foreign = t.clone();
    foreign.thread_id = "other".into();
    assert!(repo.read_model_work_plan(&foreign, None).is_err());
}

#[test]
fn model_methods_can_be_corrected_merged_retired_without_changing_author_intent_or_allowance() {
    let original_request =
        "保留已存原文，先逐项核对原文与大纲，再完成指定范围；不发布、不增加授权额度。";
    let (mut repo, task) = fixture_with_intent(false, original_request);
    let work_before = repo
        .work_task_for_attempt(&task.project_id, &task.task_id)
        .unwrap();
    let budget_before = serde_json::to_value(
        repo.work_budget_authorization(&task.project_id, &work_before.id)
            .unwrap(),
    )
    .unwrap();
    let scope_before = repo
        .execution_scope(&task.project_id, &task.task_id)
        .unwrap();
    let task_before = repo.task(&task.project_id, &task.task_id).unwrap().unwrap();
    let first = request(
        0,
        "initial-method",
        vec![goal("repair"), {
            let mut g = goal("check");
            g["kind"] = json!("check");
            g["dependsOn"] = json!(["repair"]);
            g
        }],
        None,
    );
    let receipt = repo.update_model_work_plan(&task, &first).unwrap();
    let old = repo
        .read_model_work_plan_version(&task, 1, Some("check"))
        .unwrap();
    let mut replacement = goal("check");
    replacement["kind"] = json!("deliverable");
    replacement["acceptance"] = json!("保存一个修订候选，实际作者验收由独立核验负责");
    replacement["dependsOn"] = json!([]);
    let changed = request(1, "merged-method", vec![replacement.clone()], None);
    let changed_receipt = repo.update_model_work_plan(&task, &changed).unwrap();
    assert_eq!(
        repo.update_model_work_plan(&task, &changed).unwrap(),
        changed_receipt
    );
    assert_eq!(
        repo.read_model_work_plan(&task, Some("check")).unwrap()["goal"],
        replacement
    );
    assert_eq!(
        repo.read_model_work_plan_version(&task, 1, Some("check"))
            .unwrap(),
        old
    );
    assert_eq!(repo.update_model_work_plan(&task, &first).unwrap(), receipt);
    assert_eq!(
        repo.read_model_work_plan(&task, None).unwrap()["version"],
        2
    );
    repo.update_model_work_plan(
        &task,
        &request_mode(2, "retired-window", vec![], "planningOnly"),
    )
    .unwrap();
    assert!(
        repo.read_model_work_plan(&task, None).unwrap()["goals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.task_work_decision(&task, None).unwrap()["deliveryReviewRequired"],
        true
    );
    let work_after = repo
        .work_task_for_attempt(&task.project_id, &task.task_id)
        .unwrap();
    assert_eq!(work_after.goal, work_before.goal);
    assert_eq!(work_after.steps.len(), work_before.steps.len());
    assert_eq!(
        serde_json::to_value(
            repo.work_budget_authorization(&task.project_id, &work_after.id)
                .unwrap()
        )
        .unwrap(),
        budget_before
    );
    assert_eq!(
        repo.execution_scope(&task.project_id, &task.task_id)
            .unwrap(),
        scope_before
    );
    assert_eq!(
        repo.task(&task.project_id, &task.task_id)
            .unwrap()
            .unwrap()
            .input_manifest_json,
        task_before.input_manifest_json
    );
    assert!(task_before.input_manifest_json.contains(original_request));
}

#[test]
fn corrected_methods_still_require_exact_current_source_evidence() {
    let (mut repo, task) = fixture_with_source(true);
    let mut context = json!({"target":{},"narrativeGoals":{"brief":{"objectId":"artifact-brief","sourceRevisionId":"brief-existing","authority":"candidate","content":{"premise":"东方幻想悬疑"}}},"coverage":{"missingScope":[]}});
    repo.project_task_sources(&task, &mut context).unwrap();
    let source = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == "/narrativeGoals/brief")
        .unwrap()["sourceId"]
        .as_str()
        .unwrap();
    let page = repo.task_source_content(&task, source, None).unwrap();
    let mut check = goal("source-check");
    check["kind"] = json!("check");
    check["acceptance"] = json!("按实际保存简报核对作品类型");
    let mut repair = goal("repair");
    repair["dependsOn"] = json!(["source-check"]);
    repo.update_model_work_plan(
        &task,
        &request(
            0,
            "execution-contract",
            vec![check.clone(), repair.clone()],
            None,
        ),
    )
    .unwrap();

    check["title"] = json!("核对当前简报");
    check["readingQuestion"] = json!("当前作品类型是否保留？");
    check["checkScope"] = json!("当前已保存简报");
    check["approach"] = json!("读取实际冻结简报，以原句对照");
    check["assessment"] = json!("已读取保存稿，原文为东方幻想悬疑");
    check["status"] = json!("satisfied");
    check["sourceIds"] = json!([source]);
    check["sourceChecks"] = json!([{"sourceId":source,"revisionId":"brief-existing","contentHash":page["contentHash"],"evidence":{"quote":"东方幻想悬疑"}}]);
    repair["dependsOn"] = json!(["source-check", "planning-check"]);
    let mut additional = goal("planning-check");
    additional["kind"] = json!("check");
    additional["acceptance"] = json!("修订前核对相关规划");
    let update = request(
        1,
        "method-and-coverage",
        vec![check.clone(), repair.clone(), additional.clone()],
        None,
    );
    let receipt = repo.update_model_work_plan(&task, &update).unwrap();
    assert_eq!(receipt["version"], 2);
    assert_eq!(
        repo.update_model_work_plan(&task, &update).unwrap(),
        receipt
    );
    let saved = repo
        .read_model_work_plan(&task, Some("source-check"))
        .unwrap();
    assert_eq!(saved["goal"], check);
    assert_eq!(saved["evidenceCurrent"], true);

    repair["dependsOn"] = json!(["planning-check", "source-check"]);
    repo.update_model_work_plan(
        &task,
        &request(
            2,
            "reorder-prerequisites",
            vec![check, repair, additional.clone()],
            None,
        ),
    )
    .unwrap();
    let previous = saved_plan(&repo, &task);
    additional["acceptance"] = json!("改为按需补读规划，独立作者验收仍负责完整目标");
    let goals = previous["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            if item["goal"]["id"] == "planning-check" {
                additional.clone()
            } else {
                item["goal"].clone()
            }
        })
        .collect();
    repo.update_model_work_plan(&task, &request(3, "correct-new-method", goals, None))
        .unwrap();
    assert_eq!(
        repo.read_model_work_plan(&task, Some("planning-check"))
            .unwrap()["goal"],
        additional
    );
    assert_eq!(
        repo.read_model_work_plan(&task, Some("source-check"))
            .unwrap()["evidenceCurrent"],
        true
    );
}

#[test]
fn bounded_rolling_window_retains_a_thousand_method_snapshots_without_forcing_full_plan() {
    let (mut repo, task) = fixture_with_intent(false, "持续推进长篇目标，保留作者范围与预算");
    for index in 0..1000_i64 {
        let mut method = goal(&format!("method-{}", index + 1));
        method["title"] = json!(format!("近期步骤 {}", index + 1));
        repo.update_model_work_plan(
            &task,
            &request(index, &format!("window-{index}"), vec![method], None),
        )
        .unwrap();
    }
    let current = repo.read_model_work_plan(&task, None).unwrap();
    assert_eq!(current["version"], 1000);
    assert_eq!(current["previousVersion"], 999);
    assert_eq!(current["goals"].as_array().unwrap().len(), 1);
    assert_eq!(current["goals"][0]["id"], "method-1000");
    for version in [1, 500, 999, 1000] {
        let snapshot = repo
            .read_model_work_plan_version(&task, version, Some(&format!("method-{version}")))
            .unwrap();
        assert_eq!(snapshot["version"], version);
        assert_eq!(snapshot["historicalReferenceOnly"], true);
        assert!(snapshot["evidenceCurrent"].is_null());
        assert!(serde_json::to_vec(&snapshot).unwrap().len() <= 6500);
    }
    assert!(repo.read_model_work_plan_version(&task, 0, None).is_err());
    assert!(
        repo.read_model_work_plan_version(&task, 1001, None)
            .is_err()
    );
    assert!(
        repo.read_model_work_plan_version(&task, 1, Some("method-1000"))
            .is_err()
    );
}

#[test]
fn invalid_method_graph_or_forged_completion_preserves_current_snapshot() {
    let (mut repo, t) = fixture();
    repo.update_model_work_plan(&t, &request(0, "one", vec![goal("outline")], None))
        .unwrap();
    let before = saved_plan(&repo, &t);
    let mut g = goal("outline");
    g["dependsOn"] = json!(["outline"]);
    assert!(
        repo.update_model_work_plan(&t, &request(1, "cycle", vec![g], None))
            .is_err()
    );
    let mut g = goal("outline");
    g["status"] = json!("satisfied");
    g["assessment"] = json!("保存好了");
    assert!(
        repo.update_model_work_plan(&t, &request(1, "unsupported", vec![g.clone()], None))
            .is_err()
    );
    g["evidence"] = json!([{"taskId":"invented","revisionId":"invented"}]);
    assert!(
        repo.update_model_work_plan(&t, &request(1, "forged", vec![g], None))
            .is_err()
    );
    assert_eq!(saved_plan(&repo, &t), before);
    let mut g = goal("outline");
    g["status"] = json!("inProgress");
    repo.update_model_work_plan(&t, &request(1, "active", vec![g], Some("outline")))
        .unwrap();
    assert_eq!(
        repo.task_work_decision(&t, None).unwrap()["workPlan"]["activeGoalId"],
        "outline"
    );
}
#[test]
fn model_work_plan_readonly_and_worker_tasks_cannot_change_plan() {
    let (mut repo, mut t) = fixture();
    t.target_json = json!({"type":"project","conversationFocus":{"readOnly":true}}).to_string();
    assert!(
        repo.update_model_work_plan(&t, &request(0, "readonly", vec![goal("outline")], None))
            .is_err()
    );
    t.operation = "generateChapter".into();
    assert!(repo.read_model_work_plan(&t, None).is_err());
}

#[test]
fn model_work_plan_rejects_goals_that_cannot_be_read_back() {
    let (mut repo, t) = fixture();
    let mut g = goal("large");
    for field in ["readingQuestion", "checkScope", "approach", "assessment"] {
        g[field] = json!("文".repeat(500));
    }
    assert!(
        repo.update_model_work_plan(&t, &request(0, "large", vec![g], None))
            .is_err()
    );
    assert_eq!(repo.read_model_work_plan(&t, None).unwrap()["version"], 0);
}

#[test]
fn model_work_plan_completed_check_can_cite_existing_project_source_without_new_output() {
    let (mut repo, t) = fixture_with_source(true);
    let mut context = json!({"target":{},"narrativeGoals":{"brief":{"objectId":"artifact-brief","sourceRevisionId":"brief-existing","authority":"candidate","content":{"premise":"东方幻想悬疑"}}},"coverage":{"missingScope":[]}});
    repo.project_task_sources(&t, &mut context).unwrap();
    let source = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == "/narrativeGoals/brief")
        .unwrap()["sourceId"]
        .as_str()
        .unwrap();
    let read = repo.task_source_content(&t, source, None).unwrap();
    assert_eq!(read["coverage"]["complete"], true);
    assert_eq!(read["sectionPath"], "/narrativeGoals/brief");
    assert_eq!(read["chapterNumbers"], json!([]));
    let mut checked = goal("brief-check");
    checked["kind"] = json!("check");
    checked["status"] = json!("satisfied");
    checked["assessment"] = json!("已核对保存的简报，定位仍为东方幻想悬疑");
    let missing = repo
        .update_model_work_plan(&t, &request(0, "missing", vec![checked.clone()], None))
        .unwrap_err();
    assert!(format!("{missing:?}").contains("sourceChecks"));
    checked["sourceIds"] = json!([source]);
    assert!(
        repo.update_model_work_plan(
            &t,
            &request(0, "directory-only", vec![checked.clone()], None)
        )
        .is_err()
    );
    checked["sourceChecks"] = json!([{"sourceId":source,"revisionId":"brief-existing","contentHash":read["contentHash"],"evidence":{"quote":"东方幻想悬疑"}}]);
    let mut forged = checked.clone();
    forged["sourceChecks"][0]["evidence"]["quote"] = json!("西方史诗");
    assert!(
        repo.update_model_work_plan(&t, &request(0, "forged-check", vec![forged], None))
            .is_err()
    );
    let mut pending = goal("review-plan");
    pending["dependsOn"] = json!(["brief-check"]);
    repo.update_model_work_plan(&t, &request(0, "checked", vec![checked, pending], None))
        .unwrap();
    assert_eq!(
        repo.read_model_work_plan(&t, Some("brief-check")).unwrap()["goal"]["status"],
        "satisfied"
    );
    assert_eq!(
        repo.read_model_work_plan(&t, Some("review-plan")).unwrap()["goal"]["status"],
        "pending"
    );
}

#[test]
fn new_author_turn_can_select_history_without_inheriting_authority() {
    let (mut repo, old) = fixture();
    repo.update_model_work_plan(&old, &request(0, "old", vec![goal("outline")], None))
        .unwrap();
    let old_work = repo.read_model_work_plan(&old, None).unwrap()["workTaskId"]
        .as_str()
        .unwrap()
        .to_owned();
    let p = old.project_id.clone();
    let digest = repo.project_source_watermarks(&p).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let create = |id: &str, thread: &str| CreateTaskRequest {
        task_id: id.into(),
        project_id: p.clone(),
        thread_id: thread.into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"type":"project"}).to_string(),
        input_manifest_json: json!({"authorIntent":"接着完成计划"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 99999,
    };
    let budget = |id: &str| InitialWorkBudget {
        authorization_id: format!("budget-{id}"),
        author_message_id: format!("message-{id}"),
        expected_base_snapshot_id: "initial".into(),
        expected_source_digest: digest.clone(),
        max_requests: None,
        max_tokens: None,
        deadline_at: None,
    };
    let mut last = None;
    for n in 0..8 {
        let id = format!("next-{n}");
        let t = repo
            .create_budgeted_root_task(create(&id, "thread"), &budget(&id), 2 + n)
            .unwrap();
        repo.update_model_work_plan(&t, &request(0, "plan", vec![goal("outline")], None))
            .unwrap();
        last = Some(t);
    }
    let t = last.unwrap();
    let current = repo.read_model_work_plan(&t, None).unwrap();
    assert_eq!(current["priorPlans"].as_array().unwrap().len(), 6);
    let cursor = current["nextBeforePlanRowId"].as_i64().unwrap();
    let second = repo
        .read_model_work_plan_reference(&t, None, None, Some(cursor))
        .unwrap();
    assert_eq!(second["priorPlans"].as_array().unwrap().len(), 2);
    assert!(second["nextBeforePlanRowId"].is_null());
    let before = repo.task_work_decision(&t, None).unwrap();
    let selected = repo
        .read_model_work_plan_reference(&t, Some(&old_work), Some("outline"), None)
        .unwrap();
    assert_eq!(selected["historicalReferenceOnly"], true);
    assert!(selected["evidenceCurrent"].is_null());
    assert_eq!(selected["goal"]["acceptance"], "保存修订并核对时间线");
    assert_eq!(repo.task_work_decision(&t, None).unwrap(), before);
    assert!(
        repo.read_model_work_plan_reference(&t, Some(&old_work), None, Some(cursor))
            .is_err()
    );
    let mut large = vec![goal("outline")];
    for n in 0..11 {
        let mut g = goal(&format!("long-{n}-{}", "x".repeat(65)));
        g["title"] = json!("创".repeat(100));
        large.push(g);
    }
    repo.update_model_work_plan(&t, &request(1, "large-index", large, None))
        .unwrap();
    let large_read = repo.read_model_work_plan(&t, None).unwrap();
    assert!(serde_json::to_vec(&large_read).unwrap().len() <= 6500);
    if let Some(cursor) = large_read["nextBeforePlanRowId"].as_i64() {
        let page = repo
            .read_model_work_plan_reference(&t, None, None, Some(cursor))
            .unwrap();
        assert_eq!(page["directoryOnly"], true);
        assert!(!page["priorPlans"].as_array().unwrap().is_empty());
    }
    let foreign = repo
        .create_budgeted_root_task(create("foreign", "other"), &budget("foreign"), 20)
        .unwrap();
    assert!(
        repo.read_model_work_plan(&foreign, None).unwrap()["priorPlans"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        repo.read_model_work_plan_reference(&foreign, Some(&old_work), None, None)
            .is_err()
    );
}

#[test]
fn media_plan_uses_committed_current_document_before_the_coordinator_finishes() {
    use novel_codex_story_core::{CreateCreativeProjectRequest, CreativeMedium, CreateConversationRequest, SaveMultimediaDocument, TaskUpdate};
    for medium in [CreativeMedium::Comic, CreativeMedium::Animation] {
        let mut repo = SqliteStoryRepository::in_memory().unwrap();
        let created = repo.create_creative_project(&CreateCreativeProjectRequest {
            title: "对话创作".into(), logline: "".into(), medium,
            idempotency_key: "create".into(),
        }, 100).unwrap();
        let project = ProjectId::parse(created.project.id.clone()).unwrap();
        let edition = &created.editions[0];
        let workspace = repo.multimedia_workspace(&project, &edition.id, None).unwrap();
        repo.conversation_store().create_conversation(&project, CreateConversationRequest {
            scope_id: Some(edition.scope_id.clone()), thread_id: "media-thread".into(),
            title: "设计".into(), forked_from_id: None, last_turn_id: None,
        }, 100).unwrap();
        let task = repo.create_budgeted_root_task(CreateTaskRequest {
            task_id: "media-root".into(), project_id: project.clone(), thread_id: "media-thread".into(),
            operation: "advisory".into(), input_revision: workspace.revision, model: None,
            target_json: json!({"type":"edition","editionId":edition.id,"scopeId":edition.scope_id,"branchId":edition.branch_id,"revisionId":workspace.revision_id}).to_string(),
            input_manifest_json: json!({"authorIntent":"先保存创作设计，再推进分镜"}).to_string(),
            capability_hash: "a".repeat(64), capability_expires_at: 4_000_000_000,
        }, &InitialWorkBudget {
            authorization_id: "media-budget".into(), author_message_id: "media-author".into(),
            expected_base_snapshot_id: workspace.head_snapshot_id,
            expected_source_digest: workspace.source_digest,
            max_requests: None, max_tokens: None, deadline_at: None,
        }, 100).unwrap();
        let task = repo.update_task(&project, &task.task_id, &task.thread_id, TaskUpdate {
            status: "running".into(), error: None, result_summary: None, now: 101,
        }).unwrap();
        let mut design = goal("design");
        design["title"] = json!("保存创作设计");
        design["acceptance"] = json!("创作设计已保存为当前文档");
        let mut next = goal("storyboard");
        next["dependsOn"] = json!(["design"]);
        repo.update_model_work_plan(&task, &request(0, "initial", vec![design.clone(), next.clone()], None)).unwrap();
        let mut document = serde_json::to_value(workspace.document).unwrap();
        document["creativeDesign"] = json!("雨夜相遇，黎明揭晓；先整理故事，再逐页安排分镜。");
        let saved = repo.save_task_multimedia_document(&task, &SaveMultimediaDocument {
            expected_revision_id: workspace.revision_id, idempotency_key: "design-save".into(),
            document: serde_json::from_value(document).unwrap(), read_set: vec![], sources: vec![],
        }).unwrap();
        design["status"] = json!("satisfied");
        design["assessment"] = json!("创作设计已核对并保存，尚未生成图片或视频");
        design["evidence"] = json!([{"taskId":task.task_id,"revisionId":saved.revision_id}]);
        next["status"] = json!("inProgress");
        let update = request(1, "saved-design", vec![design.clone(), next.clone()], None);
        let receipt = repo.update_model_work_plan(&task, &update).unwrap();
        assert_eq!(repo.update_model_work_plan(&task, &update).unwrap(), receipt);
        assert_eq!(repo.read_model_work_plan(&task, Some("design")).unwrap()["evidenceCurrent"], true);
        assert_eq!(repo.task(&project, &task.task_id).unwrap().unwrap().status, "running");
        let mut forged = design.clone();
        forged["evidence"][0]["taskId"] = json!("other-task");
        assert!(repo.update_model_work_plan(&task, &request(2, "foreign", vec![forged], None)).is_err());
        assert_eq!(repo.read_model_work_plan(&task, None).unwrap()["version"], 2);
        assert_eq!(receipt["grantsExecution"], false);
        // A second real output invalidates the first claim even while this task remains active.
        let mut revised = serde_json::to_value(&saved.document).unwrap();
        revised["creativeDesign"] = json!("雨夜相遇，黎明揭晓；补充人物动机后安排分镜。");
        let saved = repo.save_task_multimedia_document(&task, &SaveMultimediaDocument {
            expected_revision_id: saved.revision_id, idempotency_key: "design-revise".into(),
            document: serde_json::from_value(revised).unwrap(), read_set: vec![], sources: vec![],
        }).unwrap();
        assert_eq!(repo.read_model_work_plan(&task, Some("design")).unwrap()["evidenceCurrent"], false);
        assert!(repo.update_model_work_plan(&task, &request(2, "stale-evidence", vec![design.clone(), next.clone()], None)).is_err());
        design["evidence"][0]["revisionId"] = json!(saved.revision_id);
        repo.update_model_work_plan(&task, &request(2, "revised-evidence", vec![design.clone(), next.clone()], None)).unwrap();
        assert_eq!(repo.read_model_work_plan(&task, Some("design")).unwrap()["evidenceCurrent"], true);
        // An author's later save is real, but is not an output of this Work.
        let mut changed = serde_json::to_value(&saved.document).unwrap();
        changed["creativeDesign"] = json!("作者更改结局，需要重新核对分镜。");
        let newer = repo.save_multimedia_document(&project, &edition.id, None, &SaveMultimediaDocument {
            expected_revision_id: saved.revision_id, idempotency_key: "author-edit".into(),
            document: serde_json::from_value(changed).unwrap(), read_set: vec![], sources: vec![],
        }).unwrap();
        assert!(matches!(repo.read_model_work_plan(&task, Some("design")), Err(novel_codex_story_core::RepositoryError::TaskInputStale)));
        design["evidence"][0]["revisionId"] = json!(newer.revision_id);
        assert!(repo.update_model_work_plan(&task, &request(3, "claim-author-output", vec![design, next], None)).is_err());
    }
}

use novel_codex_story_core::{AuthoringRunsRepository, ExecutionScopeRepository, GenerationHandoffRepository, InitialWorkBudgetRepository, ModelWorkPlanRepository, MultimediaRepository, OrdinaryWorkBudgetRepository, ProjectRevisionRepository, TaskActionsRepository, TaskInputRepository, TaskSourcesRepository, TasksRepository, WorkBudgetAuthorizationRepository, WorkDecisionRepository, WorkTasksRepository};
