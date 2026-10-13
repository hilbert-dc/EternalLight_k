use novel_codex_story_core::AcceptReviewRequest;
use novel_codex_story_core::BranchId;
use novel_codex_story_core::CommitId;
use novel_codex_story_core::CreateReviewRequest;
use novel_codex_story_core::IdError;
use novel_codex_story_core::IdempotencyKey;
use novel_codex_story_core::ObjectId;
use novel_codex_story_core::ProjectId;
use novel_codex_story_core::ProjectRecord;
use novel_codex_story_core::RepositoryError;
use novel_codex_story_core::RevisionId;
use novel_codex_story_core::SaveCandidateRequest;
use novel_codex_story_core::SnapshotId;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_core::UndoAcceptanceRequest;

fn parse<T>(value: &str, parser: impl FnOnce(String) -> Result<T, IdError>) -> T {
    parser(value.to_owned()).unwrap()
}

fn candidate_repository() -> SqliteStoryRepository {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = parse("project-review", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Review".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id,
            branch_id: parse("main", BranchId::parse),
            object_id: parse("artifact-brief", ObjectId::parse),
            object_kind: "brief".into(),
            revision_id: parse("brief-1", RevisionId::parse),
            commit_id: parse("candidate-1", CommitId::parse),
            idempotency_key: parse("save-1", IdempotencyKey::parse),
            request_digest: "save-digest".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("snapshot-0", SnapshotId::parse),
            next_snapshot_id: parse("snapshot-1", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "content".into(),
            content_json: r#"{"version":1,"payload":{"premise":"memory"}}"#.into(),
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    repository
}

#[test]
fn review_accept_and_undo_are_atomic_and_idempotent() {
    let mut repository = candidate_repository();
    let review_request = CreateReviewRequest {
        review_id: "review-1".into(),
        project_id: parse("project-review", ProjectId::parse),
        object_id: parse("artifact-brief", ObjectId::parse),
        object_kind: "brief".into(),
        revision_id: parse("brief-1", RevisionId::parse),
        idempotency_key: parse("review-request-1", IdempotencyKey::parse),
        request_digest: "review-digest".into(),
        checks_json: r#"[{"id":"current-candidate","passed":true,"blocking":true}]"#.into(),
        proposals_json: "[]".into(),
        modeling_batch_id: None,
        selections_json: "{}".into(),
    };
    let review = repository.create_review(review_request.clone()).unwrap();
    assert_eq!(repository.create_review(review_request).unwrap(), review);

    let accept_request = AcceptReviewRequest {
        project_id: parse("project-review", ProjectId::parse),
        branch_id: parse("main", BranchId::parse),
        review_id: review.review_id,
        commit_id: parse("accept-1", CommitId::parse),
        idempotency_key: parse("accept-request-1", IdempotencyKey::parse),
        request_digest: "accept-digest".into(),
        expected_project_revision: 0,
        next_snapshot_id: parse("snapshot-accept-1", SnapshotId::parse),
        selections_json: "{}".into(),
    };
    let accepted = repository.accept_review(accept_request.clone()).unwrap();
    assert_eq!(repository.accept_review(accept_request).unwrap(), accepted);
    assert_eq!(accepted.result_revision, 1);
    assert_eq!(
        repository
            .object_heads(&parse("project-review", ProjectId::parse))
            .unwrap()[0]
            .accepted_revision_id,
        Some(parse("brief-1", RevisionId::parse)),
    );

    let undone = repository
        .undo_acceptance(UndoAcceptanceRequest {
            project_id: parse("project-review", ProjectId::parse),
            branch_id: parse("main", BranchId::parse),
            target_commit_id: accepted.commit_id,
            commit_id: parse("undo-1", CommitId::parse),
            idempotency_key: parse("undo-request-1", IdempotencyKey::parse),
            request_digest: "undo-digest".into(),
            expected_project_revision: 1,
            next_snapshot_id: parse("snapshot-undo-1", SnapshotId::parse),
        })
        .unwrap();
    assert_eq!(undone.result_revision, 2);
    assert_eq!(
        repository
            .object_heads(&parse("project-review", ProjectId::parse))
            .unwrap()[0]
            .accepted_revision_id,
        None,
    );
    assert_eq!(
        repository
            .project_revision(&parse("project-review", ProjectId::parse))
            .unwrap(),
        2,
    );
}

#[test]
fn accepting_a_review_from_another_project_never_changes_the_target_project() {
    let mut repository = candidate_repository();
    let target = parse("project-review", ProjectId::parse);
    let other = parse("project-review-other", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: other.clone(),
                title: "Other".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parse("main", BranchId::parse),
            parse("other-snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: other.clone(),
            branch_id: parse("main", BranchId::parse),
            object_id: parse("artifact-brief", ObjectId::parse),
            object_kind: "brief".into(),
            revision_id: parse("other-brief-1", RevisionId::parse),
            commit_id: parse("other-candidate-1", CommitId::parse),
            idempotency_key: parse("other-save-1", IdempotencyKey::parse),
            request_digest: "other-save".into(),
            expected_project_revision: 0,
            expected_snapshot_id: parse("other-snapshot-0", SnapshotId::parse),
            next_snapshot_id: parse("other-snapshot-1", SnapshotId::parse),
            expected_candidate_revision_id: None,
            content_digest: "other-content".into(),
            content_json: "{}".into(),
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: vec![],
        })
        .unwrap();
    repository
        .create_review(CreateReviewRequest {
            review_id: "foreign-review".into(),
            project_id: other,
            object_id: parse("artifact-brief", ObjectId::parse),
            object_kind: "brief".into(),
            revision_id: parse("other-brief-1", RevisionId::parse),
            idempotency_key: parse("foreign-review-key", IdempotencyKey::parse),
            request_digest: "foreign-review".into(),
            checks_json: "[]".into(),
            proposals_json: "[]".into(),
            modeling_batch_id: None,
            selections_json: "{}".into(),
        })
        .unwrap();
    let before = repository
        .branch_head(&target, &parse("main", BranchId::parse))
        .unwrap()
        .unwrap();
    let result = repository.accept_review(AcceptReviewRequest {
        project_id: target.clone(),
        branch_id: parse("main", BranchId::parse),
        review_id: "foreign-review".into(),
        commit_id: parse("bad-accept", CommitId::parse),
        idempotency_key: parse("bad-accept-key", IdempotencyKey::parse),
        request_digest: "bad".into(),
        expected_project_revision: 0,
        next_snapshot_id: parse("bad-snapshot", SnapshotId::parse),
        selections_json: "{}".into(),
    });
    assert!(matches!(
        result,
        Err(RepositoryError::CrossProjectReference)
    ));
    assert_eq!(
        repository
            .branch_head(&target, &parse("main", BranchId::parse))
            .unwrap()
            .unwrap(),
        before
    );
    assert_eq!(repository.project_revision(&target).unwrap(), 0);
}

use novel_codex_story_core::{ProjectRevisionRepository, WorkflowRepository};
