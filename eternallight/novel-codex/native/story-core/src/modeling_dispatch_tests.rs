use crate::*;
use pretty_assertions::assert_eq;
use serde_json::json;

pub(crate) fn source_repository() -> (SqliteStoryRepository, ProjectId) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("dispatch-rollback").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "rollback".into(),
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

#[test]
fn a_late_dispatch_write_failure_rolls_back_batch_step_and_task() {
    let (mut repo, project) = source_repository();
    repo.connection.execute("UPDATE story_modeling_outbox_v1 SET status='pending',generation=generation+1,first_dirty_ms=0,last_dirty_ms=0",[]).unwrap();
    let counts = || {
        repo.connection.query_row("SELECT (SELECT COUNT(*) FROM story_modeling_batches_v2),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_modeling_dispatch_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap()
    };
    let before = counts();
    let branch = repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap();
    repo.connection.execute_batch("CREATE TRIGGER fail_dispatch BEFORE INSERT ON story_modeling_dispatch_v1 BEGIN SELECT RAISE(ABORT,'dispatch write failed'); END;").unwrap();
    assert!(
        repo.dispatch_modeling_intent(&project, /*now_ms*/ 1000)
            .is_err()
    );
    let after=repo.connection.query_row("SELECT (SELECT COUNT(*) FROM story_modeling_batches_v2),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_modeling_dispatch_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap();
    assert_eq!(after, before);
    assert_eq!(
        repo.branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap(),
        branch
    );
    assert_eq!(
        repo.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 20,
            now_ms: 1000
        })
        .unwrap()["pendingCount"],
        json!(1)
    );
}

#[test]
fn an_invalidated_batch_does_not_poison_live_source_and_algorithm_deduplication() {
    let (mut repo, project) = source_repository();
    let object = ObjectId::parse("draft-chapter-1").unwrap();
    let source = RevisionId::parse("source").unwrap();
    let first = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &object,
            source_revision_id: &source,
            expected_revision: 0,
            extractor_version: "novel-knowledge-v1",
            now: 1,
        })
        .unwrap();
    crate::modeling::invalidate_source_modeling(&repo.connection, &project, &object).unwrap();
    let second = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &object,
            source_revision_id: &source,
            expected_revision: 0,
            extractor_version: "novel-knowledge-v1",
            now: 2,
        })
        .unwrap();
    let repeated = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &object,
            source_revision_id: &source,
            expected_revision: 0,
            extractor_version: "novel-knowledge-v1",
            now: 3,
        })
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(second.as_json(), repeated.as_json());
    assert_eq!(
        repo.modeling_batch(&project, &first.id).unwrap().status,
        "stale"
    );
}

#[test]
fn a_running_automatic_attempt_prevents_dispatching_another_chapter() {
    let (mut repo, project) = source_repository();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-2").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source-second").unwrap(),
        commit_id: CommitId::parse("save-second").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-second").unwrap(),
        request_digest: "second".into(),
        content_digest: "second".into(),
        content_json: json!({"chapterNumber":2,"title":"第二章","body":"另一封信。"}).to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("saved").unwrap(),
        next_snapshot_id: SnapshotId::parse("saved-second").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-2".into(),
            number: 2,
            title: "第二章".into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    repo.connection
        .execute(
            "UPDATE story_modeling_outbox_v1 SET first_dirty_ms=0,last_dirty_ms=0",
            [],
        )
        .unwrap();
    let action = repo
        .dispatch_modeling_intent(&project, /*now_ms*/ 1000)
        .unwrap()
        .unwrap();
    let work = repo
        .work_task(&project, action["workTaskId"].as_str().unwrap())
        .unwrap();
    let mut target = work.steps[0].target.clone();
    target["resumeStepId"] = json!(work.steps[0].id);
    let task = repo
        .create_task(CreateTaskRequest {
            task_id: "automatic-attempt".into(),
            project_id: project.clone(),
            thread_id: "automatic-thread".into(),
            operation: "modelKnowledge".into(),
            input_revision: 0,
            model: None,
            target_json: target.to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    assert_eq!(
        repo.dispatch_modeling_intent(&project, /*now_ms*/ 1000)
            .unwrap(),
        None
    );
    let batch = target["batchId"].as_str().unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: batch,
        task_id: &task.task_id,
        thread_id: &task.thread_id,
        entities: &[],
        assertions: &[],
        now: 2,
    })
    .unwrap();
    repo.complete_task(
        &project,
        &task.task_id,
        &task.thread_id,
        "published".into(),
        /*now*/ 3,
    )
    .unwrap();
    let next = repo
        .dispatch_modeling_intent(&project, /*now_ms*/ 1000)
        .unwrap()
        .unwrap();
    let next_work = repo
        .work_task(&project, next["workTaskId"].as_str().unwrap())
        .unwrap();
    assert_ne!(work.id, next_work.id);
    assert_eq!(next_work.steps[0].target["chapterId"], "chapter-2");
}

#[test]
fn removing_an_intent_does_not_leave_an_unclaimed_work_task_running() {
    let (mut repo, project) = source_repository();
    let action = repo
        .dispatch_modeling_intent(&project, i64::MAX)
        .unwrap()
        .unwrap();
    let id = action["workTaskId"].as_str().unwrap();
    repo.connection
        .execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    assert_eq!(
        repo.dispatch_modeling_intent(&project, i64::MAX).unwrap(),
        None
    );
    let cancelled = repo.work_task(&project, id).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(cancelled.steps[0].status, "cancelled");
    repo.connection
        .execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id='source' WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    let next = repo
        .dispatch_modeling_intent(&project, i64::MAX)
        .unwrap()
        .unwrap();
    assert_ne!(next["workTaskId"], action["workTaskId"]);
}
