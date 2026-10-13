use std::collections::BTreeMap;

use rusqlite::params;
use serde_json::json;

use super::{ApplyProposalSet, UndoProposalSet};
use crate::{
    BranchId, CommitId, CreateProposalSet, CreateReviewRequest, EvolutionDimension,
    EvolutionTraceQuery, IdempotencyKey, ObjectId, ProjectId, ProjectRecord, ProposedReview,
    RepositoryError, RevisionId, SnapshotId, SqliteStoryRepository, UndoAcceptanceRequest,
};

fn fixture() -> (SqliteStoryRepository, ProjectId, SnapshotId, String) {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("application-project").unwrap();
    let snapshot = SnapshotId::parse("application-snapshot").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "成组采用".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            snapshot.clone(),
        )
        .unwrap();
    for (object, kind, revision, review) in [
        ("artifact-brief", "brief", "brief-revision", "brief-review"),
        (
            "artifact-outline",
            "outline",
            "outline-revision",
            "outline-review",
        ),
    ] {
        repository.connection.execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, ?4, 'content', '{}')",
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
                object_id: ObjectId::parse(object).unwrap(),
                object_kind: kind.into(),
                revision_id: RevisionId::parse(revision).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("review-key-{review}")).unwrap(),
                request_digest: review.into(),
                checks_json: "[]".into(),
                proposals_json: "[]".into(),
                modeling_batch_id: None,
                selections_json: "{}".into(),
            })
            .unwrap();
    }
    let proposal = repository
        .create_proposal_set(CreateProposalSet {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "修改两处",
            operations: &[
                ProposedReview {
                    operation_id: "brief".into(),
                    review_id: "brief-review".into(),
                    depends_on: vec![],
                },
                ProposedReview {
                    operation_id: "outline".into(),
                    review_id: "outline-review".into(),
                    depends_on: vec!["brief".into()],
                },
            ],
            idempotency_key: "proposal-key",
            now: 2,
        })
        .unwrap();
    (
        repository,
        project,
        snapshot,
        proposal["id"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn group_application_is_atomic_idempotent_and_dependency_closed() {
    let (mut repository, project, base_snapshot, proposal_set_id) = fixture();
    let selections = BTreeMap::from([
        ("brief".to_owned(), json!({})),
        ("outline".to_owned(), json!({})),
    ]);
    let missing_dependency = repository.apply_proposal_set(ApplyProposalSet {
        project_id: &project,
        proposal_set_id: &proposal_set_id,
        selected_operation_ids: &["outline".into()],
        selections_by_operation_id: &BTreeMap::from([("outline".to_owned(), json!({}))]),
        idempotency_key: "incomplete",
        now: 3,
    });
    assert!(matches!(
        missing_dependency,
        Err(RepositoryError::ReviewChecksFailed)
    ));
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        base_snapshot
    );
    let selected = ["outline".to_owned(), "brief".to_owned()];
    let receipt = repository
        .apply_proposal_set(ApplyProposalSet {
            project_id: &project,
            proposal_set_id: &proposal_set_id,
            selected_operation_ids: &selected,
            selections_by_operation_id: &selections,
            idempotency_key: "apply-key",
            now: 4,
        })
        .unwrap();
    assert_eq!(receipt["resultRevision"], 1);
    assert_eq!(receipt["replayed"], false);
    let replay = repository
        .apply_proposal_set(ApplyProposalSet {
            project_id: &project,
            proposal_set_id: &proposal_set_id,
            selected_operation_ids: &selected,
            selections_by_operation_id: &selections,
            idempotency_key: "apply-key",
            now: 99,
        })
        .unwrap();
    assert_eq!(replay["applicationId"], receipt["applicationId"]);
    assert_eq!(replay["replayed"], true);
    assert_eq!(repository.project_revision(&project).unwrap(), 1);
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_some())
    );
    let count: i64 = repository.connection.query_row(
        "SELECT COUNT(*) FROM story_acceptances_v2 WHERE project_id = ?1 AND operation = 'accept' AND result_revision = 1",
        [project.as_str()], |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 2);
    assert_ne!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        base_snapshot
    );
}

#[test]
fn failed_member_validation_rolls_back_every_selected_object() {
    let (mut repository, project, base_snapshot, proposal_set_id) = fixture();
    repository.connection.execute(
        "UPDATE story_object_heads_v2 SET locked = 1 WHERE project_id = ?1 AND object_id = 'artifact-outline'",
        [project.as_str()],
    ).unwrap();
    let result = repository.apply_proposal_set(ApplyProposalSet {
        project_id: &project,
        proposal_set_id: &proposal_set_id,
        selected_operation_ids: &["brief".into(), "outline".into()],
        selections_by_operation_id: &BTreeMap::from([
            ("brief".to_owned(), json!({})),
            ("outline".to_owned(), json!({})),
        ]),
        idempotency_key: "failed-apply",
        now: 4,
    });
    assert!(matches!(result, Err(RepositoryError::ObjectLocked)));
    assert_eq!(repository.project_revision(&project).unwrap(), 0);
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        base_snapshot
    );
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
}

#[test]
fn late_member_write_failure_rolls_back_the_whole_application() {
    let (mut repository, project, base_snapshot, proposal_set_id) = fixture();
    repository.connection.execute_batch(
        "CREATE TRIGGER reject_second_proposal_member BEFORE INSERT ON story_proposal_application_members_v1
         WHEN NEW.operation_id = 'outline' BEGIN SELECT RAISE(ABORT, 'late member failure'); END;",
    ).unwrap();
    let selected = ["brief".to_owned(), "outline".to_owned()];
    let selections = BTreeMap::from([
        ("brief".to_owned(), json!({})),
        ("outline".to_owned(), json!({})),
    ]);
    let result = repository.apply_proposal_set(ApplyProposalSet {
        project_id: &project,
        proposal_set_id: &proposal_set_id,
        selected_operation_ids: &selected,
        selections_by_operation_id: &selections,
        idempotency_key: "late-failure",
        now: 7,
    });
    assert!(matches!(result, Err(RepositoryError::Storage(_))));
    assert_eq!(repository.project_revision(&project).unwrap(), 0);
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        base_snapshot
    );
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    let acceptance_count: i64 = repository
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_acceptances_v2 WHERE project_id = ?1",
            [project.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(acceptance_count, 0);
}

#[test]
fn group_undo_restores_every_head_and_cannot_undo_one_member() {
    let (mut repository, project, base_snapshot, proposal_set_id) = fixture();
    let selected = ["brief".to_owned(), "outline".to_owned()];
    let selections = BTreeMap::from([
        ("brief".to_owned(), json!({})),
        ("outline".to_owned(), json!({})),
    ]);
    let applied = repository
        .apply_proposal_set(ApplyProposalSet {
            project_id: &project,
            proposal_set_id: &proposal_set_id,
            selected_operation_ids: &selected,
            selections_by_operation_id: &selections,
            idempotency_key: "apply-undo-key",
            now: 5,
        })
        .unwrap();
    let application_id = applied["applicationId"].as_str().unwrap();
    let member: String = repository.connection.query_row(
        "SELECT acceptance_commit_id FROM story_proposal_application_members_v1 WHERE application_id = ?1 LIMIT 1",
        [application_id], |row| row.get(0),
    ).unwrap();
    let individual = repository.undo_acceptance(UndoAcceptanceRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        target_commit_id: CommitId::parse(member).unwrap(),
        commit_id: CommitId::parse("undo-single-member").unwrap(),
        idempotency_key: IdempotencyKey::parse("undo-single-member-key").unwrap(),
        request_digest: "single".into(),
        expected_project_revision: 1,
        next_snapshot_id: SnapshotId::parse("snapshot-single-member").unwrap(),
    });
    assert!(matches!(
        individual,
        Err(RepositoryError::UndoRequiresLatestCommit)
    ));
    let undone = repository
        .undo_proposal_set(UndoProposalSet {
            project_id: &project,
            application_id,
            expected_project_revision: 1,
            idempotency_key: "undo-group-key",
        })
        .unwrap();
    assert_eq!(undone["resultRevision"], 2);
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    assert_eq!(repository.project_revision(&project).unwrap(), 2);
    assert_ne!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        base_snapshot
    );
    let replay = repository
        .undo_proposal_set(UndoProposalSet {
            project_id: &project,
            application_id,
            expected_project_revision: 1,
            idempotency_key: "undo-group-key",
        })
        .unwrap();
    assert_eq!(replay["undoApplicationId"], undone["undoApplicationId"]);
    assert_eq!(replay["resultSnapshotId"], undone["resultSnapshotId"]);
    assert_eq!(replay["replayed"], true);
}

#[test]
fn creative_history_pages_each_group_member_without_skipping_shared_snapshots() {
    let (mut repository, project, _, proposal_set_id) = fixture();
    let selected = ["brief".to_owned(), "outline".to_owned()];
    let selections = BTreeMap::from([
        ("brief".to_owned(), json!({})),
        ("outline".to_owned(), json!({})),
    ]);
    let applied = repository
        .apply_proposal_set(ApplyProposalSet {
            project_id: &project,
            proposal_set_id: &proposal_set_id,
            selected_operation_ids: &selected,
            selections_by_operation_id: &selections,
            idempotency_key: "history-apply",
            now: 8,
        })
        .unwrap();
    let application_id = applied["applicationId"].as_str().unwrap();
    let applied_snapshot =
        SnapshotId::parse(applied["resultSnapshotId"].as_str().unwrap()).unwrap();
    let historical = repository
        .evolution_trace(EvolutionTraceQuery {
            project_id: &project,
            expected_revision: None,
            snapshot_id: Some(&applied_snapshot),
            dimension: EvolutionDimension::Creative,
            limit: 10,
            cursor: None,
        })
        .unwrap();
    assert_eq!(historical["entries"].as_array().unwrap().len(), 2);
    let historical_first = repository
        .evolution_trace(EvolutionTraceQuery {
            project_id: &project,
            expected_revision: None,
            snapshot_id: Some(&applied_snapshot),
            dimension: EvolutionDimension::Creative,
            limit: 1,
            cursor: None,
        })
        .unwrap();
    let historical_cursor = historical_first["nextCursor"].as_str().unwrap().to_owned();
    repository
        .undo_proposal_set(UndoProposalSet {
            project_id: &project,
            application_id,
            expected_project_revision: 1,
            idempotency_key: "history-undo",
        })
        .unwrap();
    let historical_second = repository
        .evolution_trace(EvolutionTraceQuery {
            project_id: &project,
            expected_revision: None,
            snapshot_id: Some(&applied_snapshot),
            dimension: EvolutionDimension::Creative,
            limit: 1,
            cursor: Some(&historical_cursor),
        })
        .unwrap();
    assert_eq!(historical_second["entries"].as_array().unwrap().len(), 1);
    assert_eq!(historical_second["entries"][0]["change"], "accepted");
    let mut cursor = None::<String>;
    let mut entries = Vec::new();
    for _ in 0..5 {
        let page = repository
            .evolution_trace(EvolutionTraceQuery {
                project_id: &project,
                expected_revision: Some(2),
                snapshot_id: None,
                dimension: EvolutionDimension::Creative,
                limit: 1,
                cursor: cursor.as_deref(),
            })
            .unwrap();
        entries.extend(page["entries"].as_array().unwrap().iter().cloned());
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry["change"] == "accepted")
            .count(),
        2
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry["change"] == "undone")
            .count(),
        2
    );
    assert!(
        entries
            .iter()
            .all(|entry| entry["proposalApplicationId"] == application_id)
    );
    assert!(cursor.is_none());
}

use crate::{EvolutionRepository, ProjectRevisionRepository, ProposalApplicationRepository, ProposalSetsRepository, WorkflowRepository};
