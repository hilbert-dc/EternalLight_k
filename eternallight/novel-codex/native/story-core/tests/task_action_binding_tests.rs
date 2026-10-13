use novel_codex_story_core::{
    BranchId, CommitId, CreateTaskRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
    TaskActionRequest, TaskOutputBinding, TaskRecord, TaskUpdate, UpdateModelWorkPlan,
    WorkActionRequest, WorkBudgetAuthorization, WorkDecisionRequest,
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[test]
fn unsaved_selection_discussion_rejects_candidate_and_generation_authority() {
    let mut fixture = Fixture::proposed("generateBrief", json!({"type":"artifact"}));
    let request = fixture.request("unrelated", "advisory", json!({"type":"project",
        "conversationFocus":{"view":"chapters","readOnly":true,"excerpts":[{"state":"unsaved","text":"未保存正文"}]}}));
    fixture.repo.create_task(request).unwrap();
    let task = fixture
        .repo
        .task(&fixture.project, "unrelated")
        .unwrap()
        .unwrap();
    let action = TaskActionRequest {
        id: "local-edit-action".into(),
        source_task_id: "unrelated".into(),
        operation: "generateBrief".into(),
        target: json!({"type":"artifact"}),
        instruction: "修改正文".into(),
        input_revision: 0,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    assert_eq!(
        fixture
            .repo
            .record_task_action(&fixture.project, "unrelated", "unsaved-action", action),
        Err(RepositoryError::TaskOutputForbidden)
    );
    assert!(
        fixture
            .repo
            .task_actions(&fixture.project, "unrelated")
            .unwrap()
            .is_empty()
    );
    let result = fixture.repo.propose_generation_handoff(
        &task,
        &novel_codex_story_core::GenerationHandoffRequest {
            target_chapters: 1,
            resume_run: None,
            instruction: "继续写下一章".into(),
            expected_intent_revision: 0,
            idempotency_key: "unsaved-generation".into(),
        },
    );
    assert!(matches!(result, Err(RepositoryError::TaskOutputForbidden)));
}

struct Fixture {
    repo: SqliteStoryRepository,
    project: ProjectId,
}

fn chapter_target() -> Value {
    json!({"type":"chapter","chapterId":"chapter-one","chapterNumber":1,
        "readingChapter":1,"sceneOrder":1,"scope":{"type":"throughScene","endSceneOrder":1},
        "chapterAcceptance":{"requirements":[{"id":"candidate","text":"生成可审阅候选"}],
            "length":{"requirement":"作者未指定篇幅","minCharacters":null,"maxCharacters":null}}})
}

#[test]
fn a_pending_chapter_revision_is_not_registered_again_while_host_awaits_the_turn() {
    let mut fixture = Fixture::proposed("generateBrief", json!({"type":"artifact"}));
    fixture
        .repo
        .create_task(fixture.request("unrelated", "advisory", json!({"type":"project"})))
        .unwrap();
    fixture.plan_execution("unrelated");
    let action = TaskActionRequest {
        id: "first-revision".into(),
        source_task_id: "unrelated".into(),
        operation: "generateChapter".into(),
        target: chapter_target(),
        instruction: "修正已授权章节".into(),
        input_revision: 0,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    let saved = fixture
        .repo
        .record_task_action(&fixture.project, "unrelated", "first-key", action.clone())
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .record_task_action(&fixture.project, "unrelated", "first-key", action.clone())
            .unwrap(),
        saved
    );
    let before = fixture
        .repo
        .work_task_for_attempt(&fixture.project, "unrelated")
        .unwrap();
    let mut duplicate = action;
    duplicate.id = "duplicate-revision".into();
    duplicate.instruction = "仍没保存，再修一次".into();
    let result =
        fixture
            .repo
            .record_task_action(&fixture.project, "unrelated", "second-key", duplicate);
    assert!(
        matches!(result,Err(RepositoryError::InvalidContent(ref detail)) if detail.contains("already registered"))
    );
    assert_eq!(
        fixture
            .repo
            .task_actions(&fixture.project, "unrelated")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fixture
            .repo
            .work_task_for_attempt(&fixture.project, "unrelated")
            .unwrap(),
        before
    );
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
        fixture.plan_execution("source");
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
                None,
                10,
            )
            .unwrap();
        fixture
    }

    fn plan_execution(&mut self, id: &str) {
        let task = self.repo.task(&self.project, id).unwrap().unwrap();
        let request:UpdateModelWorkPlan=serde_json::from_value(json!({"expectedPlanVersion":0,"idempotencyKey":"candidate-goals","plan":{"intentMode":"executeGoals","goals":[{"id":"candidate","title":"生成可审阅候选","acceptance":"在原授权内保存候选成果","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"","checkScope":"作者指定的候选","approach":"登记授权候选并核对真实回执","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null}]}})).unwrap();
        self.repo.update_model_work_plan(&task, &request).unwrap();
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

    fn claim_reconsidered_planning(
        &mut self,
        source_id: &str,
        action_id: &str,
        attempt_id: &str,
    ) -> TaskRecord {
        let original_source = self.repo.task(&self.project, source_id).unwrap().unwrap();
        let original_action = self
            .repo
            .task_actions(&self.project, source_id)
            .unwrap()
            .into_iter()
            .find(|a| a.id == action_id)
            .unwrap();
        let original_permission = self
            .repo
            .task_action_permission(&self.project, source_id, action_id)
            .unwrap()
            .unwrap();
        let work = self
            .repo
            .work_task_for_attempt(&self.project, source_id)
            .unwrap();
        let pending = work.steps.iter().find(|s| s.id == action_id).unwrap();
        assert_eq!(pending.status, "superseded");
        assert_eq!(pending.current_attempt_id, None);
        assert!(pending.attempts.is_empty());
        let digest = self.repo.project_source_watermarks(&self.project).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        self.repo
            .authorize_work_budget(
                &self.project,
                &work.id,
                &WorkBudgetAuthorization {
                    expected_version: work.version,
                    expected_project_revision: 0,
                    expected_source_digest: digest.clone(),
                    max_requests: Some(16),
                    max_tokens: Some(10_000),
                    deadline_at: None,
                },
                15,
            )
            .unwrap();
        let work = self.repo.work_task(&self.project, &work.id).unwrap();
        let step = self
            .repo
            .prepare_work_decision(
                &self.project,
                &work.id,
                &WorkDecisionRequest {
                    expected_work_version: work.version,
                    expected_revision: 0,
                    expected_source_digest: digest,
                },
            )
            .unwrap()
            .step
            .unwrap();
        let mut target = step.target;
        target["resumeStepId"] = json!(step.id);
        let mut request = self.request("unrelated", "advisory", target);
        request.task_id = format!("decision-{action_id}");
        let mut wrong_thread = request.clone();
        wrong_thread.thread_id = "foreign-conversation".into();
        assert_eq!(
            self.repo.create_task(wrong_thread),
            Err(RepositoryError::TaskBindingInvalid)
        );
        assert_eq!(
            self.repo.task(&self.project, &request.task_id).unwrap(),
            None
        );
        let parent = self.repo.create_task(request).unwrap();
        let manifest: Value = serde_json::from_str(&parent.input_manifest_json).unwrap();
        assert_eq!(manifest["authorIntent"], "生成候选");
        assert_eq!(manifest["authorIntentBinding"]["sourceTaskId"], source_id);
        assert_eq!(parent.thread_id, original_source.thread_id);
        assert_eq!(
            self.repo
                .work_task_for_attempt(&self.project, &parent.task_id)
                .unwrap()
                .id,
            work.id
        );
        let status = self.repo.task_work_decision(&parent, None).unwrap();
        assert_eq!(
            status["canFinish"], false,
            "source refresh cannot discharge the original output"
        );
        let ready = status["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["operation"] == original_action.operation && a["sourceTaskId"] == source_id)
            .unwrap();
        assert_eq!(ready["authorization"], "existingAuthorPlanningTarget");
        let selection = WorkActionRequest {
            expected_work_version: status["version"].as_i64().unwrap(),
            expected_source_digest: status["sourceDigest"].as_str().unwrap().into(),
            action_key: Some(ready["actionKey"].as_str().unwrap().into()),
            proposal: None,
            instruction: "读取当前简报，执行原已批准大纲目标，不采用".into(),
            idempotency_key: format!("select-{action_id}"),
            recovery_plan: None,
            chapter_review: None,
            chapter_acceptance: None,
        };
        for proposal in [
            json!({"operation":"generateOutline","target":{"sourceTaskId":"foreign-task"}}),
            json!({"operation":"generateChapter","target":{"chapterId":"unauthorized-chapter"}}),
        ] {
            let invalid = WorkActionRequest {
                action_key: None,
                proposal: Some(serde_json::from_value(proposal).unwrap()),
                ..WorkActionRequest {
                    expected_work_version: selection.expected_work_version,
                    expected_source_digest: selection.expected_source_digest.clone(),
                    action_key: selection.action_key.clone(),
                    proposal: None,
                    instruction: selection.instruction.clone(),
                    idempotency_key: selection.idempotency_key.clone(),
                    recovery_plan: None,
                    chapter_review: None,
                    chapter_acceptance: None,
                }
            };
            assert_eq!(
                self.repo.select_work_action(&parent, &invalid),
                Err(RepositoryError::TaskOutputForbidden)
            );
        }
        let selected = self.repo.select_work_action(&parent, &selection).unwrap();
        assert_eq!(
            self.repo.select_work_action(&parent, &selection).unwrap(),
            selected
        );
        assert_eq!(selected["step"]["id"], action_id);
        assert_eq!(selected["workTaskId"], work.id);
        assert_eq!(selected["threadId"], original_source.thread_id);
        assert_eq!(
            self.repo.task(&self.project, source_id).unwrap().unwrap(),
            original_source
        );
        assert_eq!(
            self.repo
                .task_actions(&self.project, source_id)
                .unwrap()
                .into_iter()
                .find(|a| a.id == action_id)
                .unwrap(),
            original_action
        );
        assert_eq!(
            self.repo
                .task_action_permission(&self.project, source_id, action_id)
                .unwrap()
                .unwrap(),
            original_permission
        );
        let grant = self
            .repo
            .task_provider_budget(&self.project, &parent.task_id, 16)
            .unwrap();
        self.repo
            .complete_task(
                &self.project,
                &parent.task_id,
                &parent.thread_id,
                "按当前已保存来源选择原大纲目标".into(),
                None,
                17,
            )
            .unwrap();
        self.repo.charge_work_provider_usage(&self.project, &work.id, &parent.task_id,
            &json!({"runId":parent.task_id,"budgetId":grant.budget_id,"status":"completed",
                "limits":{"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"maxDurationMs":null},
                "usage":{"requests":1,"tokens":1,"unknownTokens":0,"overrun":false}})).unwrap();
        let mut target = selected["step"]["target"].clone();
        target["resumeStepId"] = selected["step"]["id"].clone();
        let mut request = self.request("unrelated", &original_action.operation, target);
        request.task_id = attempt_id.into();
        let child = self.repo.create_task(request).unwrap();
        let manifest: Value = serde_json::from_str(&child.input_manifest_json).unwrap();
        assert_eq!(manifest["authorIntent"], "生成候选");
        assert_eq!(manifest["authorIntentBinding"]["sourceTaskId"], source_id);
        assert_eq!(
            self.repo
                .work_task_for_attempt(&self.project, attempt_id)
                .unwrap()
                .id,
            work.id
        );
        child
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
    fixture.plan_execution("discussion");
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
            before.control_revision,
            /*now*/ 12,
        )
        .unwrap();
    assert_eq!(paused.resume_block_reason.as_deref(), Some("inputChanged"));
    assert!(matches!(
        fixture.repo.control_work_task(
            &fixture.project,
            &before.id,
            "resume",
            paused.control_revision,
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
    fixture.plan_execution("discussion");
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
            None,
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
            None,
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
            work.control_revision,
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
            paused.control_revision,
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
    assert_eq!(
        fixture.repo.create_task(second),
        Err(RepositoryError::TaskBindingInvalid),
        "source refresh returns an unattempted planning step to the model, not directly to execution"
    );
    let second = fixture.claim_reconsidered_planning("discussion", "second", "second-attempt");
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
    let target = chapter_target();
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
            None,
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
            None,
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

#[test]
fn sequential_candidate_approval_consumes_only_completed_sibling_receipts_and_freezes_new_sources()
{
    let mut fixture = Fixture::proposed("generateBrief", json!({"type":"artifact"}));
    let mut root = fixture.request("unrelated", "advisory", json!({"type":"project"}));
    root.task_id = "chain-root".into();
    fixture.repo.create_task(root).unwrap();
    fixture.plan_execution("chain-root");
    for (id, operation) in [
        ("chain-brief", "generateBrief"),
        ("chain-outline", "generateOutline"),
    ] {
        fixture
            .repo
            .record_task_action(
                &fixture.project,
                "chain-root",
                id,
                TaskActionRequest {
                    id: id.into(),
                    source_task_id: "chain-root".into(),
                    operation: operation.into(),
                    target: json!({"type":"artifact"}),
                    instruction: "生成可审阅候选，不采用".into(),
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
            "chain-root",
            "thread-binding",
            "登记候选链".into(),
            None,
            10,
        )
        .unwrap();
    fixture
        .repo
        .approve_task_action(&fixture.project, "chain-root", "chain-brief", 0, 11)
        .unwrap();
    let mut child = fixture.request("child", "generateBrief", json!({"type":"artifact"}));
    child.target_json = json!({"type":"artifact","requestedByTaskId":"chain-root","requestedActionId":"chain-brief"}).to_string();
    fixture.repo.create_task(child).unwrap();
    fixture.save_brief("child");
    assert_eq!(
        fixture
            .repo
            .approve_task_action(&fixture.project, "chain-root", "chain-outline", 0, 12),
        Err(RepositoryError::TaskInputStale),
        "a checkpoint is not a completed sibling"
    );
    fixture
        .repo
        .complete_task(
            &fixture.project,
            "child",
            "thread-binding",
            "保存简报".into(),
            None,
            13,
        )
        .unwrap();
    let original = fixture
        .repo
        .task(&fixture.project, "chain-root")
        .unwrap()
        .unwrap();
    assert_eq!(
        fixture.repo.assert_task_inputs_current(&original),
        Err(RepositoryError::TaskInputStale),
        "original frozen inputs are never silently rewritten"
    );
    fixture
        .repo
        .approve_task_action(&fixture.project, "chain-root", "chain-outline", 0, 14)
        .unwrap();
    let mut next = fixture.request("next", "generateOutline", json!({"type":"artifact"}));
    next.target_json = json!({"type":"artifact","requestedByTaskId":"chain-root","requestedActionId":"chain-outline"}).to_string();
    assert_eq!(
        fixture.repo.create_task(next),
        Err(RepositoryError::TaskBindingInvalid),
        "approval retains the permission but cannot bypass the current-source model decision"
    );
    let next = fixture.claim_reconsidered_planning("chain-root", "chain-outline", "next");
    let (heads, _) = fixture.repo.task_context_origins(&next).unwrap();
    assert!(heads.iter().any(|head| {
        head.object_id.as_str() == "artifact-brief"
            && head
                .candidate_revision_id
                .as_ref()
                .map(|revision| revision.as_str())
                == Some("revision-binding")
    }));
}

use novel_codex_story_core::{GenerationHandoffRepository, ModelWorkPlanRepository, OrdinaryWorkBudgetRepository, ProjectRevisionRepository, TaskActionPermitRepository, TaskActionsRepository, TaskInputRepository, TaskProviderBudgetRepository, TaskSourcesRepository, TasksRepository, WorkBudgetAuthorizationRepository, WorkDecisionActionsRepository, WorkDecisionRepository, WorkTasksRepository};
