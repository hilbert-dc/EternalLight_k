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
const AUTHOR: &str = "outbox-author-token-1234567890123456789";
const CANDIDATE: &str = "outbox-candidate-token-1234567890123456";

fn page(
    repo: &SqliteStoryRepository,
    project: &ProjectId,
    cursor: Option<&str>,
    limit: usize,
) -> Value {
    repo.modeling_outbox_page(ModelingOutboxQuery {
        project_id: project,
        cursor,
        limit,
    })
    .unwrap()
}
fn edit_request(
    repo: &SqliteStoryRepository,
    project: &ProjectId,
    chapter: usize,
    suffix: &str,
) -> SaveCandidateRequest {
    let object = ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap();
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
        json!({"chapterNumber":chapter,"title":"改稿","body":"信件被藏起来。"}).to_string();
    SaveCandidateRequest {
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
            chapter_id: format!("chapter-{chapter}"),
            number: chapter as i64,
            title: "改稿".into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    }
}
fn publish(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    chapter: usize,
    suffix: &str,
    entities: &[Value],
    assertions: &[Value],
) -> String {
    let object = ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap();
    let revision = repo
        .object_heads(project)
        .unwrap()
        .into_iter()
        .find(|head| head.object_id == object)
        .unwrap()
        .candidate_revision_id
        .unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: project,
            object_id: &object,
            source_revision_id: &revision,
            expected_revision: 0,
            extractor_version: "outbox-fixture",
            now: 2,
        })
        .unwrap();
    let task = format!("task-{suffix}");
    let thread = format!("thread-{suffix}");
    repo.create_task(CreateTaskRequest {
        task_id: task.clone(),
        project_id: project.clone(),
        thread_id: thread.clone(),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"batchId":batch.id}).to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: project,
        batch_id: &batch.id,
        task_id: &task,
        thread_id: &thread,
        entities,
        assertions,
        now: 3,
    })
    .unwrap();
    repo.complete_task(
        project,
        &task,
        &thread,
        "published".into(),
        None,
        /*now*/ 4,
    )
    .unwrap();
    batch.id
}

#[tokio::test]
async fn saves_coalesce_durably_and_failed_transactions_leave_no_outbox_intent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("outbox.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 3);
    let original_task_count = repo.tasks(&project).unwrap().len();
    assert_eq!(
        page(&repo, &project, /*cursor*/ None, /*limit*/ 20)["pendingCount"],
        0
    );
    let request = edit_request(&repo, &project, /*chapter*/ 1, "first");
    repo.save_candidate(request.clone()).unwrap();
    let first = page(&repo, &project, /*cursor*/ None, /*limit*/ 20);
    repo.save_candidate(request).unwrap();
    assert_eq!(page(&repo, &project, /*cursor*/ None, /*limit*/ 20), first);
    let mut failed = edit_request(&repo, &project, /*chapter*/ 2, "failed");
    failed.task_output = Some(TaskOutputBinding {
        task_id: "unknown-task".into(),
        thread_id: "unknown-thread".into(),
        output_kind: "draft".into(),
        completeness: None,
    });
    assert!(repo.save_candidate(failed).is_err());
    assert_eq!(page(&repo, &project, /*cursor*/ None, /*limit*/ 20), first);
    let request = edit_request(&repo, &project, /*chapter*/ 1, "latest");
    repo.save_candidate(request).unwrap();
    let latest = page(&repo, &project, /*cursor*/ None, /*limit*/ 20);
    assert_eq!(latest["pendingCount"], 1);
    assert_eq!(latest["data"][0]["sourceRevisionId"], "revision-latest");
    assert_eq!(
        latest["data"][0]["firstDirtyMs"],
        first["data"][0]["firstDirtyMs"]
    );
    assert_eq!(
        latest["data"][0]["generation"].as_i64().unwrap(),
        first["data"][0]["generation"].as_i64().unwrap() + 1
    );
    assert_eq!(latest["authority"], "sourceFreshnessOnly");
    assert!(latest["data"][0].get("due").is_none());
    assert!(latest["data"][0].get("dueAtMs").is_none());
    assert_eq!(repo.tasks(&project).unwrap().len(), original_task_count);
    drop(repo);
    let repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(page(&repo, &project, /*cursor*/ None, /*limit*/ 20), latest);
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let uri = format!("/api/projects/{project}/modeling-outbox?limit=20");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header("authorization", format!("Bearer {AUTHOR}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let http: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        http["data"][0]["sourceRevisionId"],
        latest["data"][0]["sourceRevisionId"]
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("authorization", format!("Bearer {CANDIDATE}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn publication_confirms_only_current_intents_and_revokes_queue_cursors() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, 3);
    for chapter in 1..=2 {
        let request = edit_request(&repo, &project, chapter, &format!("edit-{chapter}"));
        repo.save_candidate(request).unwrap();
    }
    let first = page(&repo, &project, /*cursor*/ None, /*limit*/ 1);
    let cursor = first["nextCursor"].as_str().unwrap();
    let next = page(&repo, &project, Some(cursor), /*limit*/ 1);
    assert_eq!(next["data"][0]["objectId"], "draft-chapter-2");
    let object = ObjectId::parse("draft-chapter-1").unwrap();
    let revision = RevisionId::parse("revision-edit-1").unwrap();
    let old = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &object,
            source_revision_id: &revision,
            expected_revision: 0,
            extractor_version: "old",
            now: 1,
        })
        .unwrap();
    let request = edit_request(&repo, &project, /*chapter*/ 1, "newest");
    repo.save_candidate(request).unwrap();
    assert!(
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &old.id,
            task_id: "old",
            thread_id: "old",
            entities: &[],
            assertions: &[],
            now: 3
        })
        .is_err()
    );
    assert_eq!(
        page(&repo, &project, /*cursor*/ None, /*limit*/ 20)["pendingCount"],
        2
    );
    assert!(
        repo.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: Some(cursor),
            limit: 1,
        })
        .is_err()
    );
    let current = publish(&mut repo, &project, /*chapter*/ 1, "current", &[], &[]);
    let remaining = page(&repo, &project, /*cursor*/ None, /*limit*/ 20);
    assert_eq!(remaining["pendingCount"], 1);
    assert_eq!(remaining["data"][0]["objectId"], "draft-chapter-2");
    let other = ProjectId::parse("other-outbox").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: other.clone(),
            title: "other".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("other-initial").unwrap(),
    )
    .unwrap();
    assert_eq!(
        page(&repo, &other, /*cursor*/ None, /*limit*/ 20)["pendingCount"],
        0
    );
    assert!(
        repo.modeling_outbox_page(ModelingOutboxQuery {
            project_id: &other,
            cursor: Some(cursor),
            limit: 1,
        })
        .is_err()
    );
    repo.create_review(CreateReviewRequest {
        review_id: "brief-review".into(),
        project_id: project.clone(),
        object_id: ObjectId::parse("benchmark-brief").unwrap(),
        object_kind: "brief".into(),
        revision_id: RevisionId::parse("benchmark-brief-revision").unwrap(),
        idempotency_key: IdempotencyKey::parse("brief-review").unwrap(),
        request_digest: "brief-review".into(),
        checks_json: "[]".into(),
        proposals_json: "[]".into(),
        modeling_batch_id: None,
        selections_json: "{}".into(),
    })
    .unwrap();
    repo.accept_review(AcceptReviewRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        review_id: "brief-review".into(),
        commit_id: CommitId::parse("accept-brief").unwrap(),
        idempotency_key: IdempotencyKey::parse("accept-brief").unwrap(),
        request_digest: "accept-brief".into(),
        expected_project_revision: 0,
        next_snapshot_id: SnapshotId::parse("snapshot-accepted").unwrap(),
        selections_json: "{}".into(),
    })
    .unwrap();
    assert!(
        repo.publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &current,
            task_id: "task-current",
            thread_id: "thread-current",
            entities: &[],
            assertions: &[],
            now: 4
        })
        .is_err()
    );
    assert_eq!(
        page(&repo, &project, /*cursor*/ None, /*limit*/ 20)["pendingCount"],
        2
    );
}

#[test]
fn transitive_knowledge_invalidation_enqueues_each_current_dependent_once() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, 3);
    let source = repo.chapter_knowledge(&project, "chapter-1").unwrap();
    let prior = repo.chapter_knowledge(&project, "chapter-2").unwrap();
    let owner = prior["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == "person-0")
        .unwrap()
        .clone();
    let evidence = prior["assertions"][0]["evidence"].clone();
    let relay = json!({"id":"relay","kind":"proposition","displayName":"转述","aliases":[],"evidence":evidence});
    let assertion = |batch: Value, proposition: Value| json!({"kind":"epistemic","subjectId":proposition,"propositionId":proposition,"propositionSourceBatchId":batch,"epistemicOwnerId":"person-0","epistemicState":"known","epistemicTime":2,"predicate":"knows","value":true,"evidence":evidence});
    let batch = publish(
        &mut repo,
        &project,
        /*chapter*/ 2,
        "relay",
        &[owner.clone(), relay],
        &[assertion(
            source["batchId"].clone(),
            source["assertions"][1]["propositionId"].clone(),
        )],
    );
    publish(
        &mut repo,
        &project,
        /*chapter*/ 3,
        "dependent",
        &[owner],
        &[assertion(json!(batch), json!("relay"))],
    );
    assert_eq!(
        page(&repo, &project, /*cursor*/ None, /*limit*/ 20)["pendingCount"],
        0
    );
    let request = edit_request(&repo, &project, /*chapter*/ 1, "root-change");
    repo.save_candidate(request).unwrap();
    let pending = page(&repo, &project, /*cursor*/ None, /*limit*/ 20);
    assert_eq!(
        pending["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["objectId"].clone(), item["sourceRevisionId"].clone()))
            .collect::<Vec<_>>(),
        vec![
            (json!("draft-chapter-1"), json!("revision-root-change")),
            (json!("draft-chapter-2"), json!("revision-2")),
            (json!("draft-chapter-3"), json!("revision-3"))
        ]
    );
    assert_eq!(pending["pendingCount"], 3);
    assert_eq!(
        repo.chapter_knowledge(&project, "chapter-3").unwrap()["status"],
        "model_pending"
    );
}

use novel_codex_story_core::{ModelingOperationRepository, ModelingOutboxRepository, ModelingRepository, ProjectRevisionRepository, TasksRepository, WorkflowRepository};
