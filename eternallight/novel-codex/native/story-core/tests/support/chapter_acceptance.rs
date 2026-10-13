use novel_codex_story_core::*;
use serde_json::{Value, json};

pub fn project() -> ProjectId {
    ProjectId::parse("selected-chapter").unwrap()
}
pub fn branch() -> BranchId {
    BranchId::parse("main").unwrap()
}
pub fn group(id: &str, body: &str) -> Value {
    json!({"id":id,"sceneId":format!("scene-{id}"),"body":body,"locked":false})
}
pub fn chapter(title: &str, groups: Vec<Value>, refs: Value) -> Value {
    let body = groups
        .iter()
        .map(|group| group["body"].as_str().unwrap().trim())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    json!({"chapterId":"chapter-one","chapterNumber":1,"title":title,"body":body,"sceneGroups":groups,"sourcePlanRefs":refs,"version":1,"completeness":"chapterComplete"})
}
pub fn setup(repo: &mut SqliteStoryRepository, base: Value, candidate: Value) {
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "采用契约".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    save(repo, "base", base);
    review(repo, "base");
    repo.accept_review(accept(repo, "base", json!({}))).unwrap();
    save(repo, "candidate", candidate);
    review(repo, "candidate");
}
pub fn save(repo: &mut SqliteStoryRepository, id: &str, content: Value) {
    let head = repo
        .object_heads(&project())
        .unwrap()
        .into_iter()
        .find(|head| head.object_id.as_str() == "draft-chapter-one");
    repo.save_candidate(SaveCandidateRequest {
        project_id: project(),
        branch_id: branch(),
        object_id: ObjectId::parse("draft-chapter-one").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(id).unwrap(),
        commit_id: CommitId::parse(format!("save-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{id}")).unwrap(),
        request_digest: content.to_string(),
        content_digest: content.to_string(),
        content_json: content.to_string(),
        expected_project_revision: repo.project_revision(&project()).unwrap(),
        expected_snapshot_id: repo
            .branch_head(&project(), &branch())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: head.and_then(|head| head.candidate_revision_id),
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-one".into(),
            number: 1,
            title: content["title"].as_str().unwrap().into(),
        }),
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
}
pub fn review(repo: &mut SqliteStoryRepository, id: &str) {
    repo.create_review(CreateReviewRequest {
        review_id: format!("review-{id}"),
        project_id: project(),
        object_id: ObjectId::parse("draft-chapter-one").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(id).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("review-{id}")).unwrap(),
        request_digest: id.into(),
        checks_json: "[]".into(),
        proposals_json: "[]".into(),
        modeling_batch_id: None,
        selections_json: "{}".into(),
    })
    .unwrap();
}
pub fn accept(repo: &SqliteStoryRepository, id: &str, choices: Value) -> AcceptReviewRequest {
    AcceptReviewRequest {
        project_id: project(),
        branch_id: branch(),
        review_id: format!("review-{id}"),
        commit_id: CommitId::parse(format!("accept-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("accept-{id}")).unwrap(),
        request_digest: choices.to_string(),
        expected_project_revision: repo.project_revision(&project()).unwrap(),
        next_snapshot_id: SnapshotId::parse(format!("accepted-{id}")).unwrap(),
        selections_json: choices.to_string(),
    }
}
pub fn accepted(repo: &SqliteStoryRepository) -> Value {
    let id = repo
        .object_heads(&project())
        .unwrap()
        .into_iter()
        .find(|head| head.object_id.as_str() == "draft-chapter-one")
        .unwrap()
        .accepted_revision_id
        .unwrap();
    serde_json::from_str(&repo.revision_content(&id).unwrap().unwrap()).unwrap()
}

use novel_codex_story_core::{ProjectRevisionRepository, WorkflowRepository};
