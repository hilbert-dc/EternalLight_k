use novel_codex_story_core::{
    BranchId, ChapterProjection, CommitId, CreateTaskRequest, IdempotencyKey, InitialWorkBudget,
    ObjectId, ProjectId, ProjectRecord, PublishModelingBatch, RevisionId, SaveCandidateRequest,
    SnapshotId, SqliteStoryRepository, TaskActionRequest, TaskOutputBinding, WorkActionRequest,
    WorkBudgetAuthorization, WorkDecisionRequest,
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

struct Fixture {
    repo: SqliteStoryRepository,
    project: ProjectId,
}

impl Fixture {
    fn new(path: &std::path::Path) -> Self {
        let mut fixture = Self {
            repo: SqliteStoryRepository::open(path).unwrap(),
            project: ProjectId::parse("delivery-project").unwrap(),
        };
        fixture
            .repo
            .create_project(
                ProjectRecord {
                    project_id: fixture.project.clone(),
                    title: "交付".into(),
                    logline: String::new(),
                    created_at: 1,
                    updated_at: 1,
                },
                BranchId::parse("main").unwrap(),
                SnapshotId::parse("initial").unwrap(),
            )
            .unwrap();
        fixture
    }

    fn request(&self, operation: &str) -> CreateTaskRequest {
        CreateTaskRequest {
            task_id: "root".into(),
            project_id: self.project.clone(),
            thread_id: "thread".into(),
            operation: operation.into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: json!({"authorIntent":"分析人物并提出建议"}).to_string(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        }
    }

    fn save_brief(&mut self, revision: &str, binding: Option<TaskOutputBinding>) {
        let head = self
            .repo
            .branch_head(&self.project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap();
        let previous = self
            .repo
            .object_heads(&self.project)
            .unwrap()
            .into_iter()
            .find(|head| head.object_id.as_str() == "artifact-brief")
            .and_then(|head| head.candidate_revision_id);
        self.repo
            .save_candidate(SaveCandidateRequest {
                project_id: self.project.clone(),
                branch_id: BranchId::parse("main").unwrap(),
                object_id: ObjectId::parse("artifact-brief").unwrap(),
                object_kind: "brief".into(),
                revision_id: RevisionId::parse(revision).unwrap(),
                commit_id: CommitId::parse(format!("commit-{revision}")).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("save-{revision}")).unwrap(),
                request_digest: revision.into(),
                expected_project_revision: 0,
                expected_snapshot_id: head.snapshot_id,
                next_snapshot_id: SnapshotId::parse(format!("snapshot-{revision}")).unwrap(),
                expected_candidate_revision_id: previous,
                content_digest: revision.into(),
                content_json: json!({"premise":revision}).to_string(),
                chapter_projection: None,
                task_output: binding,
                linked_impact_ids: Vec::new(),
            })
            .unwrap();
    }

    fn complete(&mut self, task: &str, summary: &str) {
        self.repo
            .complete_task(
                &self.project,
                task,
                "thread",
                summary.into(),
                None,
                /*now*/ 10,
            )
            .unwrap();
    }

    fn work(&self, task: &str) -> Value {
        serde_json::to_value(
            self.repo
                .work_task_for_attempt(&self.project, task)
                .unwrap(),
        )
        .unwrap()
    }
}

#[test]
fn discussion_termination_is_not_artifact_or_goal_certification() {
    for summary in ["林夕的动机仍待作者确认", ""] {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::new(&directory.path().join("discussion.sqlite3"));
        fixture
            .repo
            .create_task(fixture.request("advisory"))
            .unwrap();
        fixture.complete("root", summary);
        let work = fixture.work("root");
        assert_eq!(
            (work["status"].clone(), work["deliveryStatus"].clone()),
            (json!("completed"), json!("discussion_ended"))
        );
        assert_eq!(
            work["deliveryReport"],
            json!({
                "evidence":[{"stepId":"step-root","attemptId":"root","kind":"completionReport",
                    "referenceId":if summary.is_empty() { Value::Null } else { json!("root") },
                    "state":if summary.is_empty() { "missing" } else { "recorded" }}],
                "pendingActionIds":[],"unsettledAttemptIds":[],"deliveryReview":null
            })
        );
        assert_eq!(work["steps"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn an_unexecuted_proposal_prevents_a_false_delivery_claim() {
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(&directory.path().join("proposal.sqlite3"));
    fixture
        .repo
        .create_task(fixture.request("advisory"))
        .unwrap();
    fixture
        .repo
        .record_task_action(
            &fixture.project,
            "root",
            "proposal-key",
            TaskActionRequest {
                id: "proposal".into(),
                source_task_id: "root".into(),
                operation: "generateBrief".into(),
                target: json!({}),
                instruction: "生成候选简报".into(),
                input_revision: 0,
                intent_revision: 0,
                status: "proposed".into(),
                result_task_id: None,
                error: None,
            },
        )
        .unwrap();
    fixture.complete("root", "建议生成简报，尚未获得作者许可");
    let pending = fixture.work("root");
    assert_eq!(
        (
            pending["status"].clone(),
            pending["deliveryStatus"].clone(),
            pending["deliveryReport"]["pendingActionIds"].clone()
        ),
        (json!("completed"), json!("pending"), json!(["proposal"]))
    );
    fixture
        .repo
        .decline_task_action(
            &fixture.project,
            "root",
            "proposal",
            /*expected_intent_revision*/ 0,
        )
        .unwrap();
    let ended = fixture.work("root");
    assert_eq!(ended["deliveryStatus"], "discussion_ended");
    assert_eq!(ended["deliveryReport"]["pendingActionIds"], json!([]));
    assert_eq!(fixture.repo.tasks(&fixture.project).unwrap().len(), 1);
}

#[test]
fn canonical_outputs_not_presentation_metadata_determine_delivery_after_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("artifact.sqlite3");
    let mut fixture = Fixture::new(&path);
    fixture
        .repo
        .create_task(fixture.request("generateBrief"))
        .unwrap();
    fixture.save_brief(
        "brief-one",
        Some(TaskOutputBinding {
            task_id: "root".into(),
            thread_id: "thread".into(),
            output_kind: "brief".into(),
            completeness: None,
        }),
    );
    fixture.complete("root", "候选已保存");
    let ready = fixture.work("root");
    assert_eq!(ready["deliveryStatus"], "ready_for_review");
    let report = json!({"evidence":[{"stepId":"step-root","attemptId":"root","kind":"brief",
        "referenceId":"brief-one","state":"recorded"}],"pendingActionIds":[],"unsettledAttemptIds":[],"deliveryReview":null});
    assert_eq!(ready["deliveryReport"], report);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET output_refs_json='[]' WHERE task_id='root'",
        [],
    )
    .unwrap();
    assert_eq!(fixture.work("root")["deliveryReport"], report);
    fixture.save_brief("brief-two", None);
    let superseded = fixture.work("root");
    assert_eq!(superseded["status"], "completed");
    assert_eq!(superseded["deliveryStatus"], "partial");
    assert_eq!(
        superseded["deliveryReport"],
        json!({"evidence":[{"stepId":"step-root",
        "attemptId":"root","kind":"brief","referenceId":"brief-one","state":"superseded"}],
        "pendingActionIds":[],"unsettledAttemptIds":[],"deliveryReview":null})
    );
    fixture.repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(fixture.work("root"), superseded);
    assert_eq!(
        serde_json::to_value(
            fixture
                .repo
                .work_tasks_page(&fixture.project, None)
                .unwrap()
        )
        .unwrap()["tasks"][0],
        superseded
    );
    // Corrupt the canonical binding while leaving convincing display metadata.
    db.execute("UPDATE story_tasks_v2 SET output_refs_json='[{\"kind\":\"brief\",\"revisionId\":\"brief-one\"}]' WHERE task_id='root'", []).unwrap();
    db.execute("DELETE FROM story_task_outputs_v2 WHERE task_id='root'", [])
        .unwrap();
    let missing = fixture.work("root");
    assert_eq!(
        (
            missing["deliveryStatus"].clone(),
            missing["deliveryReport"]["evidence"][0].clone()
        ),
        (
            json!("pending"),
            json!({"stepId":"step-root","attemptId":"root","kind":"brief","referenceId":null,"state":"missing"})
        )
    );
}

#[test]
fn trusted_usage_settlement_updates_delivery_without_new_execution_or_quota() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite3");
    let mut fixture = Fixture::new(&path);
    let digest = fixture
        .repo
        .project_source_watermarks(&fixture.project)
        .unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let task = fixture
        .repo
        .create_budgeted_root_task(
            fixture.request("generateBrief"),
            &InitialWorkBudget {
                authorization_id: "author-confirmation".into(),
                author_message_id: "author-message".into(),
                expected_base_snapshot_id: "initial".into(),
                expected_source_digest: digest,
                max_requests: Some(3),
                max_tokens: Some(700),
                deadline_at: Some(100),
            },
            /*now*/ 1,
        )
        .unwrap();
    fixture.save_brief(
        "brief-budget",
        Some(TaskOutputBinding {
            task_id: task.task_id.clone(),
            thread_id: "thread".into(),
            output_kind: "brief".into(),
            completeness: None,
        }),
    );
    fixture.complete(&task.task_id, "候选已保存");
    let pending = fixture.work(&task.task_id);
    assert_eq!(
        (
            pending["status"].clone(),
            pending["deliveryStatus"].clone(),
            pending["deliveryReport"]["unsettledAttemptIds"].clone()
        ),
        (json!("completed"), json!("partial"), json!([task.task_id]))
    );
    let work_id = pending["id"].as_str().unwrap();
    let receipt = json!({"runId":task.task_id,"budgetId":format!("budget-{}",task.task_id),"status":"completed",
        "limits":{"maxRequests":3,"maxTokens":700,"maxDurationMs":1000},
        "usage":{"requests":1,"tokens":25,"unknownTokens":0,"overrun":false}});
    let budget = fixture
        .repo
        .charge_work_provider_usage(&fixture.project, work_id, &task.task_id, &receipt)
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .charge_work_provider_usage(&fixture.project, work_id, &task.task_id, &receipt)
            .unwrap(),
        budget
    );
    let ready = fixture.work(&task.task_id);
    assert_eq!(
        (
            ready["status"].clone(),
            ready["deliveryStatus"].clone(),
            ready["deliveryReport"]["unsettledAttemptIds"].clone()
        ),
        (json!("completed"), json!("ready_for_review"), json!([]))
    );
    fixture.repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(fixture.work(&task.task_id), ready);
    assert_eq!(fixture.repo.tasks(&fixture.project).unwrap().len(), 1);
    assert_eq!(
        (
            budget.max_requests,
            budget.used_requests,
            budget.max_tokens,
            budget.used_tokens
        ),
        (Some(3), 1, Some(700), 25)
    );
}

#[test]
fn chapter_delivery_waits_for_its_actual_published_knowledge_and_detects_supersession() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chapter.sqlite3");
    let mut fixture = Fixture::new(&path);
    let mut request = fixture.request("generateChapter");
    request.target_json = json!({"chapterId":"chapter-one"}).to_string();
    fixture.repo.create_task(request).unwrap();
    let object = ObjectId::parse("draft-chapter-one").unwrap();
    let revision = RevisionId::parse("draft-one").unwrap();
    fixture
        .repo
        .save_candidate(SaveCandidateRequest {
            project_id: fixture.project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: object.clone(),
            object_kind: "draft".into(),
            revision_id: revision.clone(),
            commit_id: CommitId::parse("draft-save").unwrap(),
            idempotency_key: IdempotencyKey::parse("draft-save-key").unwrap(),
            request_digest: "draft".into(),
            expected_project_revision: 0,
            expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
            next_snapshot_id: SnapshotId::parse("draft-snapshot").unwrap(),
            expected_candidate_revision_id: None,
            content_digest: "draft".into(),
            content_json: json!({"chapterId":"chapter-one","body":"林夕抄写灯塔记录。"})
                .to_string(),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-one".into(),
                number: 1,
                title: "灯塔".into(),
            }),
            task_output: Some(TaskOutputBinding {
                task_id: "root".into(),
                thread_id: "thread".into(),
                output_kind: "draft".into(),
                completeness: Some("chapterComplete".into()),
            }),
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    fixture.complete("root", "草稿完成");
    let initial = fixture.work("root");
    assert_eq!(initial["steps"].as_array().unwrap().len(), 1);
    let work_id = initial["id"].as_str().unwrap().to_owned();
    let digest = fixture
        .repo
        .project_source_watermarks(&fixture.project)
        .unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    fixture
        .repo
        .authorize_work_budget(
            &fixture.project,
            &work_id,
            &WorkBudgetAuthorization {
                expected_version: initial["version"].as_i64().unwrap(),
                expected_project_revision: 0,
                expected_source_digest: digest.clone(),
                max_requests: None,
                max_tokens: None,
                deadline_at: None,
            },
            /*now*/ 10,
        )
        .unwrap();
    let work = fixture.repo.work_task(&fixture.project, &work_id).unwrap();
    let decision = fixture
        .repo
        .prepare_work_decision(
            &fixture.project,
            &work_id,
            &WorkDecisionRequest {
                expected_work_version: work.version,
                expected_revision: 0,
                expected_source_digest: digest,
            },
        )
        .unwrap()
        .step
        .unwrap();
    let mut parent_request = fixture.request("advisory");
    parent_request.task_id = "decision-parent".into();
    let mut target = decision.target;
    target["resumeStepId"] = json!(decision.id);
    parent_request.target_json = target.to_string();
    let parent = fixture.repo.create_task(parent_request).unwrap();
    let status = fixture.repo.task_work_decision(&parent, None).unwrap();
    assert_eq!(status["canFinish"], true);
    assert_eq!(status["requirements"], json!([]));
    let action = status["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["operation"] == "modelKnowledge")
        .unwrap();
    assert_eq!(action["required"], false);
    let selected = fixture
        .repo
        .select_work_action(
            &parent,
            &WorkActionRequest {
                expected_work_version: status["version"].as_i64().unwrap(),
                expected_source_digest: status["sourceDigest"].as_str().unwrap().into(),
                action_key: Some(action["actionKey"].as_str().unwrap().into()),
                proposal: None,
                instruction: "分析本次已保存章节的事实".into(),
                idempotency_key: "select-knowledge".into(),
                recovery_plan: None,
            },
        )
        .unwrap();
    fixture.complete("decision-parent", "检查本次正文的事实");
    settle_followup(&mut fixture, &work_id, "decision-parent");
    let selected_step = selected["step"]["id"].as_str().unwrap();
    let pending = fixture.work("root");
    let mut expected_report = json!({"evidence":[
        {"stepId":"step-root","attemptId":"root","kind":"draft","referenceId":"draft-one","state":"recorded"},
        {"stepId":decision.id,"attemptId":"decision-parent","kind":"completionReport","referenceId":"decision-parent","state":"recorded"},
        {"stepId":selected_step,"attemptId":null,"kind":"knowledgeBatch","referenceId":null,"state":"pending"}
    ],"pendingActionIds":[],"unsettledAttemptIds":[],"deliveryReview":null});
    assert_eq!(pending["deliveryStatus"], "partial");
    assert_eq!(pending["deliveryReport"], expected_report);
    let batch = selected["step"]["target"]["batchId"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut request = fixture.request("modelKnowledge");
    request.task_id = "modeling".into();
    let mut target = selected["step"]["target"].clone();
    target["resumeStepId"] = json!(selected_step);
    request.target_json = target.to_string();
    fixture.repo.create_task(request).unwrap();
    fixture
        .repo
        .publish_modeling_batch(PublishModelingBatch {
            project_id: &fixture.project,
            batch_id: &batch,
            task_id: "modeling",
            thread_id: "thread",
            entities: &[],
            assertions: &[],
            now: 12,
        })
        .unwrap();
    fixture.complete("modeling", "分析完成");
    settle_followup(&mut fixture, &work_id, "modeling");
    let (final_step, submitted_review) = finish_work(&mut fixture, &work_id);
    let ready = fixture.work("root");
    assert_eq!(ready["deliveryStatus"], "ready_for_review");
    expected_report["evidence"][2]["attemptId"] = json!("modeling");
    expected_report["evidence"][2]["referenceId"] = json!(batch);
    expected_report["evidence"][2]["state"] = json!("recorded");
    expected_report["evidence"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "stepId":final_step,"attemptId":"decision-final","kind":"completionReport",
            "referenceId":"decision-final","state":"recorded"
        }));
    expected_report["deliveryReview"] = submitted_review.clone();
    assert_eq!(ready["deliveryReport"], expected_report);
    let stored_review: String = rusqlite::Connection::open(&path).unwrap().query_row(
        "SELECT content_json FROM story_work_delivery_reviews_v1 WHERE decision_task_id='decision-final' AND work_task_id=?1",
        [&work_id], |row| row.get(0),
    ).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&stored_review).unwrap(),
        submitted_review
    );
    assert_eq!(submitted_review["goalStatus"], "fulfilled");
    assert_eq!(
        submitted_review["evidence"],
        json!([{"taskId":"root","revisionId":"draft-one"}])
    );
    // A published historical batch stays inspectable, but a stale head cannot
    // prove that the current candidate's dependency is ready.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_modeling_batches_v2 SET status='stale' WHERE batch_id=?1",
        [&batch],
    )
    .unwrap();
    let stale = fixture.work("root");
    assert_eq!(stale["status"], "completed");
    assert_eq!(stale["deliveryStatus"], "partial");
    let mut expected = ready["deliveryReport"].clone();
    expected["evidence"][2]["state"] = json!("superseded");
    assert_eq!(stale["deliveryReport"], expected);
}

fn settle_followup(fixture: &mut Fixture, work: &str, task: &str) {
    fixture
        .repo
        .charge_work_provider_usage(
            &fixture.project,
            work,
            task,
            &json!({"runId":task,"budgetId":format!("budget-{task}"),"status":"completed",
            "limits":{"maxRequests":null,"maxTokens":null,"maxDurationMs":null},
            "usage":{"requests":1,"tokens":1,"unknownTokens":0,"overrun":false}}),
        )
        .unwrap();
}

fn finish_work(fixture: &mut Fixture, work: &str) -> (String, Value) {
    let current = fixture.repo.work_task(&fixture.project, work).unwrap();
    let digest = fixture
        .repo
        .project_source_watermarks(&fixture.project)
        .unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let step = fixture
        .repo
        .prepare_work_decision(
            &fixture.project,
            work,
            &WorkDecisionRequest {
                expected_work_version: current.version,
                expected_revision: 0,
                expected_source_digest: digest,
            },
        )
        .unwrap()
        .step
        .unwrap();
    let mut request = fixture.request("advisory");
    request.task_id = "decision-final".into();
    let mut target = step.target;
    target["resumeStepId"] = json!(step.id);
    request.target_json = target.to_string();
    let parent = fixture.repo.create_task(request).unwrap();
    assert_eq!(
        fixture.repo.task_work_decision(&parent, None).unwrap()["canFinish"],
        true
    );
    let mut status = fixture.repo.task_work_decision(&parent, None).unwrap();
    let mut evidence = Vec::new();
    loop {
        for receipt in status["receipts"]["items"].as_array().unwrap() {
            for output in receipt["outputs"].as_array().unwrap() {
                if output["revisionId"].is_string() {
                    evidence.push(
                        json!({"taskId":receipt["taskId"],"revisionId":output["revisionId"]}),
                    );
                }
            }
        }
        let Some(cursor) = status["pagination"]["nextCursor"].as_str() else {
            break;
        };
        status = fixture
            .repo
            .task_work_decision(&parent, Some(cursor))
            .unwrap();
    }
    let review = json!({"expectedWorkVersion":status["version"],"expectedSourceDigest":status["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"已保存本次正文和分析","checkedScope":"已核对实际保存内容，未进行其他检查","unresolved":[],"authorDecisions":[],"evidence":evidence});
    fixture
        .repo
        .complete_task(
            &fixture.project,
            "decision-final",
            "thread",
            "已保存正文和分析；未进行其他检查".into(),
            Some(review.clone()),
            10,
        )
        .unwrap();
    settle_followup(fixture, work, "decision-final");
    (step.id, review)
}
