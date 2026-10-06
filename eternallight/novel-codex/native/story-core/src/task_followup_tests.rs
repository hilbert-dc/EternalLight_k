use crate::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn writer_source() -> (SqliteStoryRepository, ProjectId) {
    writer_source_with_completion(true)
}

fn writer_source_with_completion(complete: bool) -> (SqliteStoryRepository, ProjectId) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("followup").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "午夜回信".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.create_task(request(
        &project,
        "writer",
        "generateChapter",
        0,
        json!({"chapterId":"chapter-1"}),
    ))
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source").unwrap(),
        commit_id: CommitId::parse("save-source").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-source").unwrap(),
        request_digest: "save-source".into(),
        content_digest: "source".into(),
        content_json: json!({"chapterId":"chapter-1","chapterNumber":1,"title":"回信",
            "body":"信".repeat(351),"sceneGroups":[{"id":"one","sceneId":"scene-one",
                "body":"信".repeat(351),"locked":false}],"completeness":"chapterComplete"})
        .to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("saved").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: 1,
            title: "回信".into(),
        }),
        task_output: Some(TaskOutputBinding {
            task_id: "writer".into(),
            thread_id: "thread".into(),
            output_kind: "draft".into(),
            completeness: Some("chapterComplete".into()),
        }),
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    if complete {
        repo.complete_task(&project, "writer", "thread", "saved".into(), None, /*now*/ 1)
            .unwrap();
    }
    (repo, project)
}

fn request(
    project: &ProjectId,
    id: &str,
    operation: &str,
    revision: i64,
    target: Value,
) -> CreateTaskRequest {
    CreateTaskRequest {
        task_id: id.into(),
        project_id: project.clone(),
        thread_id: "thread".into(),
        operation: operation.into(),
        input_revision: revision,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: r#"{"authorIntent":"分析当前正文"}"#.into(),
        capability_hash: "f".repeat(64),
        capability_expires_at: i64::MAX,
    }
}

fn followup(source_task_id: Option<&str>) -> Option<TaskFollowup> {
    Some(TaskFollowup::ModelKnowledge {
        chapter_id: "chapter-1".into(),
        source_revision_id: "source".into(),
        source_task_id: source_task_id.map(str::to_owned),
    })
}

fn analysis(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    id: &str,
) -> (String, TaskRecord) {
    let object = ObjectId::parse("draft-chapter-1").unwrap();
    let source = RevisionId::parse("source").unwrap();
    let revision = repo.project_revision(project).unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: project,
            object_id: &object,
            source_revision_id: &source,
            expected_revision: revision,
            extractor_version: "novel-knowledge-v1",
            now: 2,
        })
        .unwrap();
    let target = json!({"chapterId":"chapter-1","batchId":batch.id});
    let task = repo
        .create_task(request(project, id, "modelKnowledge", revision, target))
        .unwrap();
    (batch.id, task)
}

fn publish_body(repo: &mut SqliteStoryRepository, project: &ProjectId) {
    let choices = json!({"sceneGroupIds":["one"],"noConfirmedChanges":true}).to_string();
    repo.create_review(CreateReviewRequest {
        review_id: "body-only".into(),
        project_id: project.clone(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source").unwrap(),
        idempotency_key: IdempotencyKey::parse("body-only").unwrap(),
        request_digest: "body-only".into(),
        checks_json: "[]".into(),
        proposals_json: "[]".into(),
        modeling_batch_id: None,
        selections_json: choices.clone(),
    })
    .unwrap();
    repo.accept_review(AcceptReviewRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        review_id: "body-only".into(),
        commit_id: CommitId::parse("publish").unwrap(),
        idempotency_key: IdempotencyKey::parse("publish").unwrap(),
        request_digest: "publish".into(),
        expected_project_revision: 0,
        next_snapshot_id: SnapshotId::parse("published").unwrap(),
        selections_json: choices,
    })
    .unwrap();
}

#[test]
fn cancelled_writer_after_body_publication_starts_exact_source_independent_analysis() {
    // Cancel after the body is durable but before task completion. A completed
    // Work cannot be cancelled merely to construct this recovery scenario.
    let (mut repo, project) = writer_source_with_completion(false);
    let old = repo.work_task_for_attempt(&project, "writer").unwrap();
    repo.control_work_task(&project, &old.id, "cancel", old.version, /*now*/ 2)
        .unwrap();
    publish_body(&mut repo, &project);
    let old = repo.work_task(&project, &old.id).unwrap();
    assert_eq!(
        repo.chapter_followup(&project, "chapter-1").unwrap(),
        followup(None)
    );
    let (batch, task) = analysis(&mut repo, &project, "fresh-analysis");
    assert_eq!(task.input_revision, 1);
    assert_eq!(
        repo.modeling_batch(&project, &batch)
            .unwrap()
            .source_revision_id,
        "source"
    );
    assert_eq!(repo.work_task(&project, &old.id).unwrap(), old);
    assert_ne!(
        repo.work_task_for_attempt(&project, &task.task_id)
            .unwrap()
            .id,
        old.id
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &repo
                .revision_content(&RevisionId::parse("source").unwrap())
                .unwrap()
                .unwrap()
        )
        .unwrap()["body"],
        "信".repeat(351)
    );
}

#[test]
fn a_ready_analysis_has_no_next_step() {
    let (mut repo, project) = writer_source();
    let (batch, task) = analysis(&mut repo, &project, "successful-analysis");
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch,
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
        "thread",
        "ready".into(), None,
        /*now*/ 3,
    )
    .unwrap();
    assert_eq!(repo.chapter_followup(&project, "chapter-1").unwrap(), None);
}

#[test]
fn failed_authority_reads_are_not_independent_task_permission() {
    let (repo, project) = writer_source();
    repo.connection
        .execute_batch("DROP TABLE story_work_attempts_v1")
        .unwrap();
    assert!(matches!(
        repo.chapter_followup(&project, "chapter-1"),
        Err(RepositoryError::Storage(_))
    ));
}

#[test]
fn pending_writer_output_keeps_exact_recovery_owner() {
    let (repo, project) = writer_source();
    let original = repo.work_task_for_attempt(&project, "writer").unwrap();
    assert_eq!(
        repo.chapter_followup(&project, "chapter-1").unwrap(),
        followup(Some("writer"))
    );
    assert_eq!(
        repo.work_task_for_attempt(&project, "writer").unwrap(),
        original
    );
}
