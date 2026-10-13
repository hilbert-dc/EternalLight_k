use rusqlite::params;
use serde_json::json;

use super::{CreateProposalSet, ProposedReview};
use crate::{
    BranchId, CreateReviewRequest, IdempotencyKey, ProjectId, ProjectRecord, RepositoryError,
    SnapshotId, SqliteStoryRepository,
};

fn fixture() -> (SqliteStoryRepository, ProjectId, SnapshotId) {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("proposal-project").unwrap();
    let snapshot = SnapshotId::parse("proposal-snapshot").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "提案作品".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            snapshot.clone(),
        )
        .unwrap();
    for (object, kind, revision, review) in [
        (
            "artifact-outline",
            "outline",
            "outline-candidate",
            "review-outline",
        ),
        ("draft-first", "draft", "draft-candidate", "review-draft"),
    ] {
        repository.connection.execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, ?4, 'fixture', '{}')",
            params![revision, project.as_str(), object, kind],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, ?3, ?4)",
            params![project.as_str(), object, kind, revision],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
            params![snapshot.as_str(), object, revision],
        ).unwrap();
        repository
            .create_review(CreateReviewRequest {
                review_id: review.into(),
                project_id: project.clone(),
                object_id: crate::ObjectId::parse(object).unwrap(),
                object_kind: kind.into(),
                revision_id: crate::RevisionId::parse(revision).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("create-{review}")).unwrap(),
                request_digest: review.into(),
                checks_json: json!([{"id":"source","blocking":true,"passed":true}]).to_string(),
                proposals_json: "[]".into(),
                modeling_batch_id: None,
                selections_json: "{}".into(),
            })
            .unwrap();
    }
    (repository, project, snapshot)
}

fn operations() -> Vec<ProposedReview> {
    vec![
        ProposedReview {
            operation_id: "outline".into(),
            review_id: "review-outline".into(),
            depends_on: vec![],
        },
        ProposedReview {
            operation_id: "draft".into(),
            review_id: "review-draft".into(),
            depends_on: vec!["outline".into()],
        },
    ]
}

#[test]
fn proposal_listing_pages_persisted_sets_without_repeating() {
    let (mut repository, project, snapshot) = fixture();
    let ops = operations();
    for ordinal in 0..22 {
        repository
            .create_proposal_set(CreateProposalSet {
                project_id: &project,
                base_snapshot_id: &snapshot,
                intent: &format!("候选提案 {ordinal}"),
                operations: &ops,
                idempotency_key: &format!("proposal-page-{ordinal}"),
                now: ordinal,
            })
            .unwrap();
    }
    let first = repository.proposal_sets_page(&project, None).unwrap();
    let second = repository
        .proposal_sets_page(
            &project,
            Some(first["nextCursor"].as_str().unwrap().parse().unwrap()),
        )
        .unwrap();
    assert_eq!(first["data"].as_array().unwrap().len(), 20);
    assert_eq!(second["data"].as_array().unwrap().len(), 2);
    assert_eq!(second["nextCursor"], serde_json::Value::Null);
    let ids = first["data"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["data"].as_array().unwrap())
        .map(|item| item["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 22);
    assert!(matches!(
        repository.proposal_sets_page(&project, Some(0)),
        Err(RepositoryError::InvalidContent(_))
    ));
}

#[test]
fn proposal_preflight_requires_closure_and_never_moves_formal_state() {
    let (mut repository, project, snapshot) = fixture();
    let ops = operations();
    let created = repository
        .create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "改大纲和第一章",
            operations: &ops,
            idempotency_key: "proposal-key",
            now: 2,
        })
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let replay = repository
        .create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "改大纲和第一章",
            operations: &ops,
            idempotency_key: "proposal-key",
            now: 3,
        })
        .unwrap();
    assert_eq!(replay["id"], id);
    assert_eq!(replay["replayed"], true);

    let partial = repository
        .preview_proposal_set(&project, id, &["draft".into()])
        .unwrap();
    assert_eq!(partial["selection"]["declaredClosureReady"], false);
    assert_eq!(
        partial["selection"]["missingDependencies"],
        json!([{"operationId":"draft","requires":"outline"}])
    );
    let complete = repository
        .preview_proposal_set(&project, id, &["draft".into(), "outline".into()])
        .unwrap();
    assert_eq!(complete["selection"]["declaredClosureReady"], true);
    assert_eq!(complete["selection"]["applyReady"], false);
    assert_eq!(complete["coverage"]["semanticDependenciesComplete"], false);
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        snapshot
    );
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    assert!(matches!(
        repository.preview_proposal_set(&project, id, &["unknown".into()]),
        Err(RepositoryError::InvalidContent(_))
    ));

    repository.connection.execute("UPDATE story_object_heads_v2 SET locked = 1 WHERE project_id = ?1 AND object_id = 'draft-first'", [project.as_str()]).unwrap();
    let stale = repository
        .preview_proposal_set(&project, id, &["draft".into(), "outline".into()])
        .unwrap();
    assert_eq!(stale["status"], "stale");
    assert_eq!(stale["staleOperations"], json!(["draft"]));
    assert_eq!(stale["selection"]["declaredClosureReady"], false);

    let other = ProjectId::parse("another-proposal-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: other.clone(),
                title: "另一作品".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("another-proposal-snapshot").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        repository.preview_proposal_set(&other, id, &[]),
        Err(RepositoryError::CandidateNotFound)
    ));
}

#[test]
fn proposal_rejects_cycles_foreign_reviews_and_changed_idempotent_requests() {
    let (mut repository, project, snapshot) = fixture();
    let ops = operations();
    repository
        .create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "原提案",
            operations: &ops,
            idempotency_key: "same-key",
            now: 2,
        })
        .unwrap();
    assert!(matches!(
        repository.create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "改了意图",
            operations: &ops,
            idempotency_key: "same-key",
            now: 2,
        }),
        Err(RepositoryError::IdempotencyConflict)
    ));
    let cyclic = vec![
        ProposedReview {
            operation_id: "outline".into(),
            review_id: "review-outline".into(),
            depends_on: vec!["draft".into()],
        },
        ProposedReview {
            operation_id: "draft".into(),
            review_id: "review-draft".into(),
            depends_on: vec!["outline".into()],
        },
    ];
    assert!(matches!(
        repository.create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "循环",
            operations: &cyclic,
            idempotency_key: "cycle",
            now: 2,
        }),
        Err(RepositoryError::InvalidContent(_))
    ));
    let mut foreign = ops;
    foreign[1].review_id = "other-project-review".into();
    assert!(matches!(
        repository.create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "外部审阅",
            operations: &foreign,
            idempotency_key: "foreign",
            now: 2,
        }),
        Err(RepositoryError::ReviewNotFound)
    ));
}

use crate::{ProjectRevisionRepository, ProposalSetsRepository, WorkflowRepository};
