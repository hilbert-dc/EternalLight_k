use crate::*;
use pretty_assertions::assert_eq;
use serde_json::json;

pub(crate) fn source_repository() -> (SqliteStoryRepository, ProjectId) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("model-recovery").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "model recovery".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source").unwrap(),
        commit_id: CommitId::parse("save-source").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-source").unwrap(),
        request_digest: "digest".into(),
        content_digest: "digest".into(),
        content_json: json!({"chapterNumber":1,"title":"正文","body":"信件被藏起来。"}).to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("saved").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: 1,
            title: "正文".into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    (repo, project)
}

fn modeling_attempt() -> (SqliteStoryRepository, ProjectId, String) {
    let (mut repo, project) = source_repository();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-chapter-1").unwrap(),
            source_revision_id: &RevisionId::parse("source").unwrap(),
            expected_revision: 0,
            extractor_version: "test-author-request",
            now: 1,
        })
        .unwrap()
        .id;
    repo.create_task(CreateTaskRequest {
        task_id: "model".into(),
        project_id: project.clone(),
        thread_id: "root".into(),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"batchId":batch,"chapterId":"chapter-1"}).to_string(),
        input_manifest_json: json!({"authorIntent":"分析我保存的正文"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    (repo, project, batch)
}

fn failed_publication() -> (SqliteStoryRepository, ProjectId, String, WorkTask) {
    let (mut repo, project, batch) = modeling_attempt();
    repo.bind_recovery_operation(RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: "model".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: "publish".into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch,
        task_id: "model",
        thread_id: "root",
        entities: &[],
        assertions: &[],
        now: 2,
    })
    .unwrap();
    repo.update_task(
        &project,
        "model",
        "root",
        TaskUpdate {
            status: "failed".into(),
            error: Some("provider_output_limit".into()),
            result_summary: None,
            now: 3,
        },
    )
    .unwrap();
    let work = repo.work_task_for_attempt(&project, "model").unwrap();
    (repo, project, batch, work)
}

#[test]
fn author_resume_recovers_committed_modeling_without_republication_or_a_new_attempt() {
    let (mut repo, project, batch, work) = failed_publication();
    assert_eq!(
        (work.status.as_str(), work.resume_block_reason.as_deref()),
        ("blocked", None)
    );
    assert_eq!(work.resume_mode, Some(WorkResumeMode::RecoverSavedResult));
    let saved = repo.modeling_batch(&project, &batch).unwrap().as_json();
    let recovered = repo
        .control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 4)
        .unwrap();
    assert_eq!(
        (
            recovered.status.as_str(),
            recovered.steps[0].status.as_str(),
            recovered.steps[0].attempts.len()
        ),
        ("completed", "completed", 1)
    );
    let task = repo.task(&project, "model").unwrap().unwrap();
    assert_eq!(
        (
            task.status.as_str(),
            task.error.as_deref(),
            task.capability_revoked_at
        ),
        ("completed", Some("provider_output_limit"), Some(4))
    );
    assert_eq!(
        repo.modeling_batch(&project, &batch).unwrap().as_json(),
        saved
    );
    let operations: Vec<(String, String)> = repo
        .connection
        .prepare("SELECT tool_name,state FROM story_tool_operations_v1")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        operations,
        vec![("publish_knowledge_batch".into(), "committed".into())]
    );
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", work.control_revision, 5),
        Err(RepositoryError::WorkTaskStale)
    ));
}

#[test]
fn unpublished_output_limit_requires_settlement_before_model_recovery() {
    let (mut repo, project, batch) = modeling_attempt();
    repo.update_task(
        &project,
        "model",
        "root",
        TaskUpdate {
            status: "failed".into(),
            error: Some("provider_output_limit".into()),
            result_summary: None,
            now: 2,
        },
    )
    .unwrap();
    let work = repo.work_task_for_attempt(&project, "model").unwrap();
    let batch_before = repo.modeling_batch(&project, &batch).unwrap().as_json();
    assert_eq!(work.resume_mode, None);
    // An unpublished batch is not an accepted unknown tool operation.
    // Without a settled retryable attempt, execution itself remains blocked.
    assert_eq!(work.resume_block_reason.as_deref(), Some("executionBlocked"));
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 3),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(repo.work_task_for_attempt(&project, "model").unwrap(), work);
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", work.control_revision, 3),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(repo.work_task_for_attempt(&project, "model").unwrap(), work);
    assert_eq!(
        repo.modeling_batch(&project, &batch).unwrap().as_json(),
        batch_before
    );
    assert_eq!(
        repo.task(&project, "model").unwrap().unwrap().status,
        "failed"
    );
}

#[test]
fn accepted_unknown_write_cannot_be_recovered_as_completed_knowledge() {
    let (mut repo, project, _batch, work) = failed_publication();
    repo.connection
        .execute(
            "UPDATE story_tool_operations_v1 SET state='accepted',receipt_ref=NULL",
            [],
        )
        .unwrap();
    let before = repo.work_task_for_attempt(&project, "model").unwrap();
    assert_eq!(
        before.resume_block_reason.as_deref(),
        Some("pendingRecovery")
    );
    assert_eq!(before.resume_mode, None);
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        before
    );
    assert_eq!(
        repo.task(&project, "model").unwrap().unwrap().status,
        "failed"
    );
    assert_eq!(
        repo.connection
            .query_row("SELECT state FROM story_tool_operations_v1", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "accepted"
    );
}

#[test]
fn stale_publication_does_not_bypass_author_resume_source_checks() {
    let (mut repo, project, _batch, work) = failed_publication();
    repo.connection
        .execute(
            "UPDATE story_workspaces_v2 SET revision=revision+1 WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    let before = repo.work_task_for_attempt(&project, "model").unwrap();
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", before.control_revision, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        before
    );
}

#[test]
fn pausing_an_active_attempt_does_not_hide_an_unknown_write_on_resume() {
    let (mut repo, project, _batch) = modeling_attempt();
    repo.bind_recovery_operation(RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: "model".into(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: "unknown".into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    })
    .unwrap();
    let work = repo.work_task_for_attempt(&project, "model").unwrap();
    let paused = repo
        .control_work_task(&project, &work.id, "pause", work.control_revision, 3)
        .unwrap();
    assert_eq!(
        paused.resume_block_reason.as_deref(),
        Some("pendingRecovery")
    );
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", paused.control_revision, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        paused
    );
}

#[test]
fn a_ready_batch_without_its_active_knowledge_head_cannot_satisfy_resume() {
    let (mut repo, project, batch, work) = failed_publication();
    repo.connection
        .execute(
            "DELETE FROM story_knowledge_heads_v2 WHERE batch_id=?1",
            [batch],
        )
        .unwrap();
    let before = repo.work_task_for_attempt(&project, "model").unwrap();
    assert_eq!(before.resume_block_reason.as_deref(), Some("inputChanged"));
    assert_eq!(before.resume_mode, None);
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", before.control_revision, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(
        repo.work_task_for_attempt(&project, "model").unwrap(),
        before
    );
}

#[test]
fn saved_result_recovery_leaves_unexecuted_follow_up_author_paused() {
    let (mut repo, project, _batch, work) = failed_publication();
    repo.connection.execute(
        "INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status) \
         VALUES('follow-up',?1,2,'advisory','{}',0,?2,'queued')",
        rusqlite::params![work.id,work.steps[0].id],
    ).unwrap();
    let before = repo.work_task(&project, &work.id).unwrap();
    assert_eq!(before.resume_mode, Some(WorkResumeMode::RecoverSavedResult));
    let recovered = repo
        .control_work_task(&project, &work.id, "recoverSavedResult", before.control_revision, 4)
        .unwrap();
    assert_eq!(
        (
            recovered.status.as_str(),
            &recovered.resume_mode,
            recovered.steps[0].status.as_str(),
            recovered.steps[1].status.as_str()
        ),
        (
            "paused",
            &Some(WorkResumeMode::ContinueExecution),
            "completed",
            "queued"
        )
    );
    assert!(recovered.steps[1].attempts.is_empty());
    let resumed = repo
        .control_work_task(&project, &work.id, "resume", recovered.control_revision, 5)
        .unwrap();
    assert_eq!(resumed.status, "running");
    assert!(resumed.steps[1].attempts.is_empty());
}

#[test]
fn local_recovery_rechecks_publication_even_when_work_version_is_unchanged() {
    let (mut repo, project, batch, work) = failed_publication();
    assert_eq!(work.resume_mode, Some(WorkResumeMode::RecoverSavedResult));
    repo.connection
        .execute(
            "DELETE FROM story_knowledge_heads_v2 WHERE batch_id=?1",
            [batch],
        )
        .unwrap();
    let before = repo.work_task(&project, &work.id).unwrap();
    assert_eq!(before.version, work.version);
    assert_eq!(before.resume_mode, None);
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 4),
        Err(RepositoryError::TaskInputStale)
    ));
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), before);
}

fn funded_failed_publication(
    unlimited: bool,
    settled: bool,
) -> (
    SqliteStoryRepository,
    ProjectId,
    String,
    TaskRecord,
    WorkTask,
) {
    let (mut repo, project) = source_repository();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-chapter-1").unwrap(),
            source_revision_id: &RevisionId::parse("source").unwrap(),
            expected_revision: 0,
            extractor_version: "funded-recovery",
            now: 1,
        })
        .unwrap()
        .id;
    let digest = repo.project_source_watermarks(&project).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let task = repo
        .create_budgeted_root_task(
            CreateTaskRequest {
                task_id: "native-mints-model".into(),
                project_id: project.clone(),
                thread_id: "root".into(),
                operation: "modelKnowledge".into(),
                input_revision: 0,
                model: None,
                target_json: json!({"batchId":batch,"chapterId":"chapter-1"}).to_string(),
                input_manifest_json: json!({"authorIntent":"分析我保存的正文"}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            },
            &InitialWorkBudget {
                authorization_id: "knowledge-recovery-authority".into(),
                author_message_id: "knowledge-recovery-author-message".into(),
                expected_base_snapshot_id: "saved".into(),
                expected_source_digest: digest,
                max_requests: (!unlimited).then_some(10),
                max_tokens: (!unlimited).then_some(1000),
                deadline_at: None,
            },
            1,
        )
        .unwrap();
    let grant = repo
        .task_provider_budget(&project, &task.task_id, 1)
        .unwrap();
    repo.bind_recovery_operation(RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: task.task_id.clone(),
        thread_id: "root".into(),
        turn_id: "turn".into(),
        call_id: "publish".into(),
        tool_name: "publish_knowledge_batch".into(),
        request_hash: "b".repeat(64),
        idempotency_ref: None,
        now: 2,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch,
        task_id: &task.task_id,
        thread_id: "root",
        entities: &[],
        assertions: &[],
        now: 2,
    })
    .unwrap();
    repo.update_task(
        &project,
        &task.task_id,
        "root",
        TaskUpdate {
            status: "failed".into(),
            error: Some("provider_output_limit".into()),
            result_summary: None,
            now: 3,
        },
    )
    .unwrap();
    if settled {
        repo.charge_work_provider_usage(&project, &grant.work_task_id, &task.task_id,
            &json!({"runId":task.task_id,"budgetId":grant.budget_id,"status":"completed",
                "limits":{"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"maxDurationMs":null},
                "usage":{"requests":1,"tokens":20,"unknownTokens":10,"overrun":false}})).unwrap();
    }
    let work = repo.work_task_for_attempt(&project, &task.task_id).unwrap();
    (repo, project, batch, task, work)
}

fn execution_facts(repo: &SqliteStoryRepository, work: &str) -> (i64, i64, String) {
    repo.connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM story_tasks_v2), \
         (SELECT COUNT(*) FROM story_task_provider_budgets_v1), \
         (SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1)",
            [work],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
}

#[test]
fn unknown_usage_allows_only_current_saved_result_recovery_without_a_new_grant() {
    for unlimited in [false, true] {
        for paused in [false, true] {
            let (mut repo, project, batch, task, mut work) =
                funded_failed_publication(unlimited, true);
            if paused {
                work = repo
                    .control_work_task(&project, &work.id, "pause", work.control_revision, 4)
                    .unwrap();
            }
            assert_eq!(work.resume_block_reason.as_deref(), if unlimited { None } else { Some("unknownUsage") });
            assert_eq!(work.resume_mode, Some(WorkResumeMode::RecoverSavedResult));
            let facts = execution_facts(&repo, &work.id);
            let publication = repo.modeling_batch(&project, &batch).unwrap().as_json();
            if !unlimited { assert!(matches!(
                repo.control_work_task(&project, &work.id, "resume", work.control_revision, 5),
                Err(RepositoryError::WorkUsageUnknown)
            )); }
            assert_eq!(repo.work_task(&project, &work.id).unwrap(), work);
            let recovered = repo
                .control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 5)
                .unwrap();
            assert_eq!(recovered.status, "completed");
            assert_eq!(recovered.steps[0].attempts.len(), 1);
            assert_eq!(execution_facts(&repo, &work.id), facts);
            assert_eq!(
                repo.modeling_batch(&project, &batch).unwrap().as_json(),
                publication
            );
            let completed = repo.task(&project, &task.task_id).unwrap().unwrap();
            assert_eq!(completed.status, "completed");
            assert_eq!(completed.error.as_deref(), Some("provider_output_limit"));
        }
    }
}

#[test]
fn saved_result_recovery_does_not_unblock_queued_execution_or_clear_unknown_usage() {
    let (mut repo, project, _batch, _task, work) = funded_failed_publication(false, true);
    repo.connection.execute(
        "INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status) \
         VALUES('follow-up',?1,2,'advisory','{}',0,?2,'queued')",
        rusqlite::params![work.id,work.steps[0].id],
    ).unwrap();
    let before = repo.work_task(&project, &work.id).unwrap();
    let facts = execution_facts(&repo, &work.id);
    let recovered = repo
        .control_work_task(&project, &work.id, "recoverSavedResult", before.control_revision, 4)
        .unwrap();
    assert_eq!(recovered.status, "paused");
    assert_eq!(
        recovered.resume_block_reason.as_deref(),
        Some("unknownUsage")
    );
    assert_eq!(recovered.resume_mode, None);
    assert_eq!(recovered.steps[0].status, "completed");
    assert_eq!(recovered.steps[1].status, "queued");
    assert!(recovered.steps[1].attempts.is_empty());
    assert!(matches!(
        repo.control_work_task(&project, &work.id, "resume", recovered.control_revision, 5),
        Err(RepositoryError::WorkUsageUnknown)
    ));
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), recovered);
    assert_eq!(execution_facts(&repo, &work.id), facts);
}

#[test]
fn unknown_usage_never_waives_saved_publication_or_pending_result_guards() {
    for fence in [
        "source",
        "head",
        "accepted",
        "unsettled",
        "cancelled",
        "project",
    ] {
        let (mut repo, project, batch, task, work) =
            funded_failed_publication(false, fence != "unsettled");
        match fence {
            "source" => {
                repo.connection
                    .execute(
                        "UPDATE story_workspaces_v2 SET revision=revision+1 WHERE project_id=?1",
                        [project.as_str()],
                    )
                    .unwrap();
            }
            "head" => {
                repo.connection
                    .execute(
                        "DELETE FROM story_knowledge_heads_v2 WHERE batch_id=?1",
                        [&batch],
                    )
                    .unwrap();
            }
            "accepted" => {
                repo.connection
                    .execute(
                        "UPDATE story_tool_operations_v1 SET state='accepted',receipt_ref=NULL",
                        [],
                    )
                    .unwrap();
            }
            "cancelled" => {
                repo.control_work_task(&project, &work.id, "cancel", work.control_revision, 4)
                    .unwrap();
            }
            "unsettled" | "project" => {}
            _ => unreachable!(),
        }
        let before = repo.work_task(&project, &work.id).unwrap();
        if fence != "project" {
            assert_eq!(before.resume_mode, None, "{fence}");
        }
        let request_project = if fence == "project" {
            ProjectId::parse("foreign-project").unwrap()
        } else {
            project.clone()
        };
        let facts = execution_facts(&repo, &work.id);
        let publication = repo.modeling_batch(&project, &batch).unwrap().as_json();
        assert!(
            repo.control_work_task(
                &request_project,
                &work.id,
                "recoverSavedResult",
                before.control_revision,
                5
            )
            .is_err(),
            "{fence}"
        );
        assert_eq!(repo.work_task(&project, &work.id).unwrap(), before);
        assert_eq!(
            repo.task(&project, &task.task_id).unwrap().unwrap().status,
            "failed"
        );
        assert_eq!(
            repo.modeling_batch(&project, &batch).unwrap().as_json(),
            publication
        );
        assert_eq!(execution_facts(&repo, &work.id), facts);
    }
    let (repo, project, _batch, _task, work) = funded_failed_publication(false, true);
    for reason in [
        "readerRestart",
        "inputChanged",
        "pendingRecovery",
        "partialOutput",
        "attemptBudget",
        "budgetExpired",
    ] {
        assert_eq!(
            crate::work_modeling_recovery::resume_mode(
                &repo.connection,
                &project,
                &work.id,
                &work.kind,
                &work.status,
                Some(reason)
            )
            .unwrap(),
            None,
            "{reason}"
        );
    }
}

fn saved_review_with_unknown_usage(
    unlimited: bool,
    partial: bool,
) -> (SqliteStoryRepository, ProjectId, String, WorkTask) {
    saved_review_with_unknown_usage_kind(unlimited, partial, false)
}

fn saved_review_with_unknown_usage_kind(
    unlimited: bool,
    partial: bool,
    conversation: bool,
) -> (SqliteStoryRepository, ProjectId, String, WorkTask) {
    let (mut repo, project) = source_repository();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("artifact-outline").unwrap(),
        object_kind: "outline".into(),
        revision_id: RevisionId::parse("review-source").unwrap(),
        commit_id: CommitId::parse("save-review-source").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-review-source").unwrap(),
        request_digest: "review-digest".into(),
        content_digest: "review-digest".into(),
        content_json: json!({"nodes":[{"id":"book","kind":"book","title":"全书"}]}).to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("saved").unwrap(),
        next_snapshot_id: SnapshotId::parse("review-snapshot").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    let digest = repo.project_source_watermarks(&project).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let author_intent = "评审现有大纲，只保存报告供我核对。";
    let run = if conversation {
        let root = repo
            .create_budgeted_root_task(
                CreateTaskRequest {
                    task_id: "native-mints-review-root".into(),
                    project_id: project.clone(),
                    thread_id: "root".into(),
                    operation: "advisory".into(),
                    input_revision: 0,
                    model: None,
                    target_json: json!({"type":"project"}).to_string(),
                    input_manifest_json: json!({"authorIntent":author_intent}).to_string(),
                    capability_hash: "a".repeat(64),
                    capability_expires_at: i64::MAX,
                },
                &InitialWorkBudget {
                    authorization_id: "review-conversation-authority".into(),
                    author_message_id: "review-result-author".into(),
                    expected_base_snapshot_id: "review-snapshot".into(),
                    expected_source_digest: digest.clone(),
                    max_requests: (!unlimited).then_some(10),
                    max_tokens: (!unlimited).then_some(1000),
                    deadline_at: None,
                },
                1,
            )
            .unwrap();
        let root_grant = repo
            .task_provider_budget(&project, &root.task_id, 1)
            .unwrap();
        let plan: UpdateModelWorkPlan = serde_json::from_value(json!({"expectedPlanVersion":0,
            "idempotencyKey":"saved-review-goal","plan":{"intentMode":"executeGoals","goals":[{
                "id":"review","title":"核对持久评审报告","acceptance":"根据真实报告完成作者要求",
                "kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"已有评审报告是什么",
                "checkScope":"当前大纲","approach":"按需评审与回读","assessment":"","evidence":[],
                "sourceIds":[],"sourceChecks":[],"continuation":null}]}})).unwrap();
        repo.update_model_work_plan(&root, &plan).unwrap();
        let run = repo
            .request_task_planning_review(
                &project,
                &root.task_id,
                &TaskPlanningReviewRequest {
                    thread_id: "root".into(),
                    expected_revision: 0,
                    expected_intent_revision: 0,
                    artifact_kind: "outline".into(),
                    revision_id: "review-source".into(),
                    target_node_id: None,
                    roles: vec!["structure".into()],
                    focus: "核对结构".into(),
                    idempotency_key: "saved-result-review".into(),
                },
                2,
            )
            .unwrap();
        repo.complete_task(
            &project,
            &root.task_id,
            "root",
            "已登记评审，等待保存报告核对".into(),
            None,
            3,
        )
        .unwrap();
        repo.charge_work_provider_usage(&project,&root_grant.work_task_id,&root.task_id,
            &json!({"runId":root.task_id,"budgetId":root_grant.budget_id,"status":"completed",
                "limits":{"maxRequests":root_grant.max_requests,"maxTokens":root_grant.max_tokens,"maxDurationMs":null},
                "usage":{"requests":0,"tokens":0,"unknownTokens":0,"overrun":false}})).unwrap();
        run
    } else {
        let run = repo
            .create_planning_review(CreatePlanningReview {
                run_id: "planning-review-saved-result",
                project_id: &project,
                artifact_kind: "outline",
                revision_id: "review-source",
                target_node_id: None,
                author_intent,
                author_message_id: "review-result-author",
                roles: &["structure".into()],
                now: 1,
            })
            .unwrap();
        let work_id = run["workTaskId"].as_str().unwrap();
        let work = repo.work_task(&project, work_id).unwrap();
        repo.authorize_work_budget(
            &project,
            work_id,
            &WorkBudgetAuthorization {
                expected_version: work.version,
                expected_project_revision: 0,
                expected_source_digest: digest,
                max_requests: (!unlimited).then_some(10),
                max_tokens: (!unlimited).then_some(1000),
                deadline_at: None,
            },
            1,
        )
        .unwrap();
        run
    };
    let run_id = run["id"].as_str().unwrap();
    let work_id = run["workTaskId"].as_str().unwrap();
    let task = repo
        .create_planning_review_seat_task(
            &project,
            run_id,
            "structure",
            CreateTaskRequest {
                task_id: "saved-review-seat".into(),
                project_id: project.clone(),
                thread_id: "review-thread".into(),
                operation: "reviewPlanningSeat".into(),
                input_revision: 0,
                model: None,
                target_json:
                    json!({"type":"artifact","artifactKind":"outline","revisionId":"review-source",
            "reviewRunId":run_id,"role":"structure"})
                    .to_string(),
                input_manifest_json: "{}".into(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            },
        )
        .unwrap();
    let grant = repo
        .task_provider_budget(&project, &task.task_id, 1)
        .unwrap();
    let coverage = if partial {
        json!([])
    } else {
        json!([{"checkId":"structure:book","status":"checked"}])
    };
    repo.save_planning_review_report(SavePlanningReviewReport {
        project_id: &project,
        run_id,
        task_id: &task.task_id,
        thread_id: "review-thread",
        expected_revision: 0,
        report: &json!({"summary":"已保存的结构核对","strengths":[],
            "findings":[{"note":"保留这条既有分析"}],"coverage":coverage}),
        now: 2,
    })
    .unwrap();
    repo.complete_task(
        &project,
        &task.task_id,
        "review-thread",
        "报告已保存".into(),
        None,
        3,
    )
    .unwrap();
    // Core consumes a trusted terminal provider receipt here. The cross-layer
    // HTTP tests exercise the real Engine ledger and public receipt reader.
    repo.charge_work_provider_usage(&project, work_id, &task.task_id,
        &json!({"runId":task.task_id,"budgetId":grant.budget_id,"status":"completed",
            "limits":{"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"maxDurationMs":null},
            "usage":{"requests":1,"tokens":20,"unknownTokens":10,"overrun":false}})).unwrap();
    let work = repo.work_task(&project, work_id).unwrap();
    (repo, project, run_id.into(), work)
}

#[test]
fn saved_terminal_review_recovers_while_unknown_usage_and_author_pause_remain_safe() {
    for unlimited in [false, true] {
        for partial in [false, true] {
            for paused in [false, true] {
                let (mut repo, project, run_id, mut work) =
                    saved_review_with_unknown_usage(unlimited, partial);
                assert_eq!(work.resume_mode, Some(WorkResumeMode::RecoverSavedResult));
                assert_eq!(work.resume_block_reason.as_deref(), if unlimited { None } else { Some("unknownUsage") });
                if paused {
                    work = repo
                        .control_work_task(&project, &work.id, "pause", work.control_revision, 4)
                        .unwrap();
                    assert!(matches!(
                        repo.finish_planning_review(&project, &run_id, false, 5),
                        Err(RepositoryError::TaskNotWritable)
                    ));
                }
                let before = repo.planning_review(&project, &run_id).unwrap();
                let facts = execution_facts(&repo, &work.id);
                if !unlimited { assert!(matches!(
                    repo.control_work_task(&project, &work.id, "resume", work.control_revision, 5),
                    Err(RepositoryError::WorkUsageUnknown)
                )); }
                assert!(matches!(
                    repo.control_work_task(
                        &project,
                        &work.id,
                        "recoverSavedResult",
                        work.control_revision - 1,
                        5
                    ),
                    Err(RepositoryError::WorkTaskStale)
                ));
                let recovered = repo
                    .control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 5)
                    .unwrap();
                assert_eq!(recovered.status, "completed");
                let after = repo.planning_review(&project, &run_id).unwrap();
                assert_eq!(after["reports"], before["reports"]);
                assert_eq!(
                    after["status"],
                    if partial { "partial" } else { "completed" }
                );
                assert_eq!(
                    after["synthesis"]["priorities"].as_array().unwrap().len(),
                    1
                );
                assert_eq!(
                    after["synthesis"]["incompleteRoles"],
                    if partial {
                        json!(["structure"])
                    } else {
                        json!([])
                    }
                );
                assert_eq!(execution_facts(&repo, &work.id), facts);
                assert!(
                    repo.control_work_task(
                        &project,
                        &work.id,
                        "recoverSavedResult",
                        recovered.control_revision,
                        6
                    )
                    .is_err()
                );
                assert_eq!(repo.planning_review(&project, &run_id).unwrap(), after);
            }
        }
    }
}

#[test]
fn saved_review_recovery_uses_frozen_reports_after_the_live_outline_head_changes() {
    let (mut repo, project, run_id, work) = saved_review_with_unknown_usage(false, false);
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("artifact-outline").unwrap(),
        object_kind: "outline".into(),
        revision_id: RevisionId::parse("later-outline").unwrap(),
        commit_id: CommitId::parse("later-outline-save").unwrap(),
        idempotency_key: IdempotencyKey::parse("later-outline-save").unwrap(),
        request_digest: "later".into(),
        content_digest: "later".into(),
        content_json: json!({"nodes":[{"id":"book","kind":"book","title":"新大纲"}]}).to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("review-snapshot").unwrap(),
        next_snapshot_id: SnapshotId::parse("later-outline-snapshot").unwrap(),
        expected_candidate_revision_id: Some(RevisionId::parse("review-source").unwrap()),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    let seat = repo.task(&project, "saved-review-seat").unwrap().unwrap();
    assert!(matches!(
        repo.assert_task_inputs_current(&seat),
        Err(RepositoryError::TaskInputStale)
    ));
    let before = repo.planning_review(&project, &run_id).unwrap();
    let facts = execution_facts(&repo, &work.id);
    let recovered = repo
        .control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 5)
        .unwrap();
    assert_eq!(recovered.status, "completed");
    assert_eq!(
        repo.planning_review(&project, &run_id).unwrap()["reports"],
        before["reports"]
    );
    assert_eq!(execution_facts(&repo, &work.id), facts);
}

#[test]
fn saved_review_recovery_rejects_unfinished_or_unbound_publications_without_writes() {
    for fault in [
        "reportMissing",
        "wrongReportRole",
        "wrongCoverageStatus",
        "wrongRefs",
        "wrongOutput",
        "queuedStep",
        "wrongCurrentAttempt",
        "activeTask",
        "unsettledGrant",
        "acceptedWrite",
        "sourceHash",
        "reader",
    ] {
        let (mut repo, project, run_id, work) = saved_review_with_unknown_usage(false, false);
        // Fault injection only creates negative broken bindings in isolated
        // fixtures. Every successful Task/report/grant was created above.
        let sql = match fault {
            "reportMissing" => "DELETE FROM story_planning_review_reports_v2",
            "wrongReportRole" => "UPDATE story_planning_review_reports_v2 SET role='character'",
            "wrongCoverageStatus" => "UPDATE story_planning_review_reports_v2 SET status='partial'",
            "wrongRefs" => {
                "UPDATE story_tasks_v2 SET output_refs_json='[]' WHERE task_id='saved-review-seat'"
            }
            "wrongOutput" => "UPDATE story_task_outputs_v2 SET completeness='partial'",
            "queuedStep" => "UPDATE story_work_steps_v1 SET status='queued'",
            "wrongCurrentAttempt" => "UPDATE story_work_steps_v1 SET current_attempt_id=NULL",
            "activeTask" => {
                "UPDATE story_tasks_v2 SET status='running' WHERE task_id='saved-review-seat'"
            }
            "unsettledGrant" => "UPDATE story_task_provider_budgets_v1 SET settled=0",
            "acceptedWrite" => {
                "INSERT INTO story_tool_operations_v1 VALUES('review-unknown-write','model-recovery','saved-review-seat','review-thread','turn','call','submit_planning_review_report','hash',NULL,'accepted',NULL,1,1)"
            }
            "sourceHash" => "UPDATE story_planning_review_runs_v2 SET source_packet_hash='wrong'",
            "reader" => "UPDATE story_work_steps_v1 SET operation='reviewQuality'",
            _ => unreachable!(),
        };
        repo.connection.execute_batch(sql).unwrap();
        let facts = execution_facts(&repo, &work.id);
        let raw_run: (String, Option<String>) = repo
            .connection
            .query_row(
                "SELECT status,synthesis_json FROM story_planning_review_runs_v2 WHERE run_id=?1",
                [&run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(
            repo.control_work_task(&project, &work.id, "recoverSavedResult", work.control_revision, 5)
                .is_err(),
            "{fault}"
        );
        assert_eq!(execution_facts(&repo, &work.id), facts, "{fault}");
        assert_eq!(repo.connection.query_row(
            "SELECT status,synthesis_json FROM story_planning_review_runs_v2 WHERE run_id=?1",[&run_id],
            |row|Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?))).unwrap(),raw_run,"{fault}");
    }
    let (mut repo, project, run_id, work) = saved_review_with_unknown_usage(false, false);
    let cancelled = repo
        .control_work_task(&project, &work.id, "cancel", work.control_revision, 5)
        .unwrap();
    let facts = execution_facts(&repo, &work.id);
    assert!(
        repo.control_work_task(
            &project,
            &work.id,
            "recoverSavedResult",
            cancelled.control_revision,
            6
        )
        .is_err()
    );
    assert_eq!(
        repo.planning_review(&project, &run_id).unwrap()["status"],
        "cancelled"
    );
    assert_eq!(execution_facts(&repo, &work.id), facts);
}

#[test]
fn recovering_conversation_reports_does_not_complete_or_resume_its_pending_author_decision() {
    for unlimited in [false, true] {
        let (mut repo, project, run_id, work) =
            saved_review_with_unknown_usage_kind(unlimited, false, true);
        let paused = repo
            .control_work_task(&project, &work.id, "pause", work.control_revision, 4)
            .unwrap();
        assert_eq!(paused.resume_mode, Some(WorkResumeMode::RecoverSavedResult));
        assert_eq!(paused.resume_block_reason.as_deref(), if unlimited { None } else { Some("unknownUsage") });
        let facts = execution_facts(&repo, &work.id);
        let before = repo.planning_review(&project, &run_id).unwrap();
        assert_eq!(
            repo.connection
                .query_row(
                    "SELECT finished FROM story_work_decisions_v1 WHERE work_task_id=?1",
                    [&work.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        let recovered = repo
            .control_work_task(&project, &work.id, "recoverSavedResult", paused.control_revision, 5)
            .unwrap();
        assert_eq!(recovered.status, "paused");
        assert_eq!(
            recovered.resume_block_reason.as_deref(),
            if unlimited { None } else { Some("unknownUsage") }
        );
        assert_eq!(recovered.resume_mode, if unlimited { Some(WorkResumeMode::ContinueExecution) } else { None });
        assert_eq!(
            repo.planning_review(&project, &run_id).unwrap()["status"],
            "completed"
        );
        assert_eq!(
            repo.planning_review(&project, &run_id).unwrap()["reports"],
            before["reports"]
        );
        assert_eq!(
            repo.connection
                .query_row(
                    "SELECT finished FROM story_work_decisions_v1 WHERE work_task_id=?1",
                    [&work.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(execution_facts(&repo, &work.id), facts);
        let resume = repo.control_work_task(&project, &work.id, "resume", recovered.control_revision, 6);
        if unlimited { assert_eq!(resume.unwrap().status, "running"); }
        else { assert!(matches!(resume, Err(RepositoryError::WorkUsageUnknown))); }
        assert!(
            repo.control_work_task(
                &project,
                &work.id,
                "recoverSavedResult",
                recovered.control_revision,
                6
            )
            .is_err()
        );
        assert_eq!(execution_facts(&repo, &work.id), facts);
    }
}
