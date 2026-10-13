use rusqlite::params;
use serde_json::{Value, json};

use super::{CreateHypothesis, HypothesisAssumption};
use crate::{
    BranchId, KnowledgePerspective, KnowledgeStateQuery, ProjectId, ProjectRecord, RepositoryError,
    SnapshotId, SqliteStoryRepository,
};

fn add_source(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    snapshot: &SnapshotId,
    object_id: &str,
    revision_id: &str,
    batch_id: &str,
    quote: &str,
) {
    let assertion = json!({"kind":"assertion","subjectId":"master","predicate":"alive",
        "value":true,"storyTime":1,"evidence":{"quote":quote,"start":0,"end":quote.chars().count()}});
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', ?4)",
        params![revision_id, project.as_str(), object_id, json!({"chapterNumber":1,"body":quote}).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
        params![snapshot.as_str(), object_id, revision_id],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, 'draft', ?3)",
        params![project.as_str(), object_id, revision_id],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, entities_json, assertions_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'ready', '[]', ?5, 1, 2)",
        params![batch_id, project.as_str(), object_id, revision_id, json!([assertion]).to_string()],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_snapshot_knowledge_v2 SET batch_id = ?3 WHERE snapshot_id = ?1 AND object_id = ?2",
        params![snapshot.as_str(), object_id, batch_id],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, ?2, 'master', 'character', '师父', '[]', ?3)",
        params![project.as_str(), batch_id, json!({"quote":quote,"start":0,"end":quote.chars().count()}).to_string()],
    ).unwrap();
}

fn author_state(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    snapshot: &SnapshotId,
) -> Value {
    repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: project,
            snapshot_id: Some(snapshot),
            perspective: KnowledgePerspective::Author,
            world_time: None,
            epistemic_time: None,
            reading_chapter: None,
            owner_id: None,
            entity_id: None,
            predicate: None,
        })
        .unwrap()
}

#[test]
fn historical_hypothesis_suppresses_only_selected_evidence_and_never_changes_canon() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("hypothesis-story").unwrap();
    let snapshot = SnapshotId::parse("hypothesis-snapshot").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "白塔".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            snapshot.clone(),
        )
        .unwrap();
    add_source(
        &repository,
        &project,
        &snapshot,
        "draft-one",
        "revision-one",
        "batch-one",
        "师父活着",
    );
    add_source(
        &repository,
        &project,
        &snapshot,
        "draft-two",
        "revision-two",
        "batch-two",
        "师父现身",
    );
    let before = author_state(&repository, &project, &snapshot);
    assert_eq!(before["entries"].as_array().unwrap().len(), 2);
    let assumptions = vec![
        HypothesisAssumption::SuppressClaim {
            modeling_batch_id: "batch-one".into(),
            source_revision_id: "revision-one".into(),
            assertion_index: 0,
        },
        HypothesisAssumption::AssumeClaim {
            subject_id: "master".into(),
            predicate: "alive".into(),
            value: json!(false),
            story_time: Some(1),
        },
    ];
    let created = repository
        .create_hypothesis(CreateHypothesis {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "如果师父已经死去",
            assumptions: &assumptions,
            idempotency_key: "hypothesis-request-1",
            now: 4,
        })
        .unwrap();
    assert_eq!(created["replayed"], false);
    let id = created["id"].as_str().unwrap();
    let replayed = repository
        .create_hypothesis(CreateHypothesis {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "如果师父已经死去",
            assumptions: &assumptions,
            idempotency_key: "hypothesis-request-1",
            now: 99,
        })
        .unwrap();
    assert_eq!(replayed["id"], id);
    assert_eq!(replayed["replayed"], true);
    let report = repository.read_hypothesis(&project, id).unwrap();
    assert_eq!(report["authority"], "hypothesisOnly");
    assert_eq!(report["baseline"]["entryCount"], 2);
    assert_eq!(report["simulation"]["entries"].as_array().unwrap().len(), 2);
    assert_eq!(
        report["simulation"]["entries"][0]["modelingBatchId"],
        "batch-two"
    );
    assert_eq!(
        report["simulation"]["entries"][1]["confirmationState"],
        "assumed"
    );
    assert_eq!(
        report["simulation"]["findings"][0]["confirmationState"],
        "hypothesisConflict"
    );
    assert_eq!(
        report["simulation"]["coverage"]["globalImpactComplete"],
        false
    );
    assert_eq!(report["simulation"]["coverage"]["projectionComplete"], true);
    assert_eq!(author_state(&repository, &project, &snapshot), before);
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        snapshot
    );

    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET status = 'stale' WHERE batch_id = 'batch-one'",
            [],
        )
        .unwrap();
    assert_eq!(
        repository.read_hypothesis(&project, id).unwrap()["simulation"],
        report["simulation"]
    );
    let another = ProjectId::parse("another-hypothesis-story").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: another.clone(),
                title: "另一作品".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("another-hypothesis-snapshot").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        repository.read_hypothesis(&another, id),
        Err(RepositoryError::CandidateNotFound)
    ));
}

#[test]
fn hypothesis_rejects_unbound_sources_unknown_entities_and_changed_idempotent_payload() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("hypothesis-validation").unwrap();
    let snapshot = SnapshotId::parse("hypothesis-validation-snapshot").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "白塔".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            snapshot.clone(),
        )
        .unwrap();
    add_source(
        &repository,
        &project,
        &snapshot,
        "draft-one",
        "revision-one",
        "batch-one",
        "师父活着",
    );
    let good = [HypothesisAssumption::AssumeClaim {
        subject_id: "master".into(),
        predicate: "alive".into(),
        value: json!(false),
        story_time: Some(1),
    }];
    repository
        .create_hypothesis(CreateHypothesis {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "师父已死",
            assumptions: &good,
            idempotency_key: "same-key",
            now: 2,
        })
        .unwrap();
    let changed = [HypothesisAssumption::AssumeClaim {
        subject_id: "master".into(),
        predicate: "alive".into(),
        value: json!(true),
        story_time: Some(1),
    }];
    assert!(matches!(
        repository.create_hypothesis(CreateHypothesis {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "师父已死",
            assumptions: &changed,
            idempotency_key: "same-key",
            now: 2,
        }),
        Err(RepositoryError::IdempotencyConflict)
    ));
    let foreign_claim = [HypothesisAssumption::SuppressClaim {
        modeling_batch_id: "batch-one".into(),
        source_revision_id: "wrong-revision".into(),
        assertion_index: 0,
    }];
    assert!(matches!(
        repository.create_hypothesis(CreateHypothesis {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "错源",
            assumptions: &foreign_claim,
            idempotency_key: "foreign-source",
            now: 2,
        }),
        Err(RepositoryError::CandidateNotFound)
    ));
    let unknown_entity = [HypothesisAssumption::AssumeClaim {
        subject_id: "unknown".into(),
        predicate: "alive".into(),
        value: json!(true),
        story_time: Some(1),
    }];
    assert!(matches!(
        repository.create_hypothesis(CreateHypothesis {
            project_id: &project,
            base_snapshot_id: &snapshot,
            intent: "未知人物",
            assumptions: &unknown_entity,
            idempotency_key: "unknown-entity",
            now: 2,
        }),
        Err(RepositoryError::CandidateNotFound)
    ));
    let count: i64 = repository
        .connection
        .query_row("SELECT COUNT(*) FROM story_hypotheses_v1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

use crate::{HypothesesRepository, KnowledgeProjectionRepository, ProjectRevisionRepository};
