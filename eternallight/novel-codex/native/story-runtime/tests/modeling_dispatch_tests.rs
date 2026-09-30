use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

#[path = "modeling_dispatch/domain_source_tests.rs"]
mod domain_source;

fn edit(repo: &mut SqliteStoryRepository, project: &ProjectId, suffix: &str) {
    let object = ObjectId::parse("draft-chapter-1").unwrap();
    let previous = repo
        .object_heads(project)
        .unwrap()
        .into_iter()
        .find(|head| head.object_id == object)
        .unwrap()
        .candidate_revision_id;
    let head = repo
        .branch_head(project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap()
        .snapshot_id;
    let content =
        json!({"chapterNumber":1,"title":"改稿","body":format!("信件被藏起来。{suffix}")})
            .to_string();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: object,
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(format!("revision-{suffix}")).unwrap(),
        commit_id: CommitId::parse(format!("commit-{suffix}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(suffix).unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: 0,
        expected_snapshot_id: head,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}")).unwrap(),
        expected_candidate_revision_id: previous,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: 1,
            title: "改稿".into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
}
fn dispatch(repo: &mut SqliteStoryRepository, project: &ProjectId) -> WorkTask {
    let action = repo
        .dispatch_modeling_intent(project, i64::MAX)
        .unwrap()
        .unwrap();
    repo.work_task(project, action["workTaskId"].as_str().unwrap())
        .unwrap()
}
fn claim(project: &ProjectId, work: &WorkTask, number: usize) -> CreateTaskRequest {
    let mut target = work.steps[0].target.clone();
    target["resumeStepId"] = json!(work.steps[0].id);
    CreateTaskRequest {
        task_id: format!("task-auto-{number}"),
        project_id: project.clone(),
        thread_id: format!("thread-auto-{number}"),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    }
}

#[tokio::test]
async fn dispatch_recovers_one_reservation_and_completes_through_normal_task_publication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dispatch.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "saved");
    assert_eq!(
        repo.dispatch_modeling_intent(&project, /*now_ms*/ 0)
            .unwrap(),
        None
    );
    let work = dispatch(&mut repo, &project);
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(dispatch(&mut repo, &project), work);
    let candidate = "candidate-dispatch-token-123456789012";
    let author = "author-dispatch-token-123456789012345";
    let app = router(RuntimeState::new(path, author.into(), candidate.into()));
    let uri = format!("/api/projects/{project}/modeling-outbox/dispatch");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("authorization", format!("Bearer {author}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["action"]["workTaskId"], work.id);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", format!("Bearer {candidate}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let task = repo
        .create_task(claim(&project, &work, /*number*/ 1))
        .unwrap();
    assert!(
        repo.create_task(claim(&project, &work, /*number*/ 2))
            .is_err()
    );
    let batch = work.steps[0].target["batchId"].as_str().unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: batch,
        task_id: &task.task_id,
        thread_id: &task.thread_id,
        entities: &[],
        assertions: &[],
        now: 1,
    })
    .unwrap();
    repo.complete_task(
        &project,
        &task.task_id,
        &task.thread_id,
        "published".into(),
        /*now*/ 2,
    )
    .unwrap();
    assert_eq!(
        repo.work_task(&project, &work.id).unwrap().status,
        "completed"
    );
    assert_eq!(
        repo.dispatch_modeling_intent(&project, i64::MAX).unwrap(),
        None
    );
    assert_eq!(
        repo.chapter_knowledge(&project, "chapter-1").unwrap()["status"],
        "ready"
    );
    edit(&mut repo, &project, "after-completion");
    let next_work = dispatch(&mut repo, &project);
    assert_ne!(next_work.id, work.id);
    // The replaced dispatch pointer does not destroy the old source proof.
    // Its frozen input is stale, rather than missing its domain identity.
    assert_eq!(
        repo.assert_task_inputs_current(&task),
        Err(RepositoryError::TaskInputStale)
    );
}

#[test]
fn superseding_unclaimed_sources_never_bypasses_pause_or_cancel() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "first");
    let first = dispatch(&mut repo, &project);
    edit(&mut repo, &project, "second");
    let second = dispatch(&mut repo, &project);
    assert_ne!(first.id, second.id);
    assert_eq!(
        repo.work_task(&project, &first.id).unwrap().status,
        "cancelled"
    );
    assert!(
        repo.create_task(claim(&project, &first, /*number*/ 1))
            .is_err()
    );
    let paused = repo
        .control_work_task(
            &project,
            &second.id,
            "pause",
            second.version,
            /*now*/ 1,
        )
        .unwrap();
    edit(&mut repo, &project, "third");
    assert_eq!(
        repo.dispatch_modeling_intent(&project, i64::MAX).unwrap(),
        None
    );
    assert_eq!(
        repo.work_task(&project, &second.id)
            .unwrap()
            .resume_block_reason,
        Some("inputChanged".into())
    );
    assert!(
        repo.control_work_task(
            &project,
            &second.id,
            "resume",
            paused.version,
            /*now*/ 2
        )
        .is_err()
    );
    repo.control_work_task(
        &project,
        &second.id,
        "cancel",
        paused.version,
        /*now*/ 3,
    )
    .unwrap();
    let third = dispatch(&mut repo, &project);
    repo.control_work_task(&project, &third.id, "cancel", third.version, /*now*/ 4)
        .unwrap();
    assert_eq!(
        repo.dispatch_modeling_intent(&project, i64::MAX).unwrap(),
        None
    );
}

#[test]
fn failures_require_author_resume_and_cannot_exceed_the_attempt_budget() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "budget");
    let mut work = dispatch(&mut repo, &project);
    let mut original_source = None;
    for number in 1..=3 {
        let task = repo.create_task(claim(&project, &work, number)).unwrap();
        let manifest: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
        assert!(!manifest.as_object().unwrap().contains_key("authorIntent"));
        if let Some(source) = &original_source {
            assert_eq!(&manifest["domainTaskSource"], source);
        } else {
            original_source = Some(manifest["domainTaskSource"].clone());
        }
        repo.update_task(
            &project,
            &task.task_id,
            &task.thread_id,
            TaskUpdate {
                status: "failed".into(),
                error: Some("model failed".into()),
                result_summary: None,
                now: 2,
            },
        )
        .unwrap();
        work = repo.work_task(&project, &work.id).unwrap();
        assert_eq!(work.status, "blocked");
        assert_eq!(
            repo.dispatch_modeling_intent(&project, i64::MAX).unwrap(),
            None
        );
        if number < 3 {
            work = repo
                .control_work_task(&project, &work.id, "resume", work.version, /*now*/ 3)
                .unwrap();
        }
    }
    assert_eq!(work.resume_block_reason, Some("attemptBudget".into()));
    assert!(
        repo.control_work_task(&project, &work.id, "resume", work.version, /*now*/ 4)
            .is_err()
    );
    assert!(
        repo.create_task(claim(&project, &work, /*number*/ 4))
            .is_err()
    );
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), work);
    assert_eq!(work.steps[0].attempts.len(), 3);
}

#[test]
fn restart_blocks_an_interrupted_attempt_instead_of_automatically_replaying_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interrupted.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "interrupted");
    let work = dispatch(&mut repo, &project);
    let task = repo
        .create_task(claim(&project, &work, /*number*/ 1))
        .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repo.recover_orphaned_tasks(/*now*/ 4).unwrap(),
        vec![task.task_id]
    );
    let blocked = repo.work_task(&project, &work.id).unwrap();
    assert_eq!(blocked.status, "blocked");
    assert_eq!(blocked.steps[0].attempts.len(), 1);
    assert_eq!(
        repo.dispatch_modeling_intent(&project, i64::MAX).unwrap(),
        None
    );
    let resumed = repo
        .control_work_task(
            &project,
            &work.id,
            "resume",
            blocked.version,
            /*now*/ 5,
        )
        .unwrap();
    repo.create_task(claim(&project, &resumed, /*number*/ 2))
        .unwrap();
    assert_eq!(
        repo.work_task(&project, &work.id).unwrap().steps[0]
            .attempts
            .len(),
        2
    );
}
