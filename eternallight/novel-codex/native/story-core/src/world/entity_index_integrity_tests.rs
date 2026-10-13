use rusqlite::params;
use serde_json::json;

use crate::{
    BranchId, CommitId, CommitRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, RevisionRecord, SnapshotId, SqliteStoryRepository,
};

fn seed_project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let project_id = ProjectId::parse(format!("index-project-{suffix}")).unwrap();
    let branch = BranchId::parse("main").unwrap();
    let start = SnapshotId::parse(format!("snapshot-{suffix}-0")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Identity".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            branch.clone(),
            start.clone(),
        )
        .unwrap();
    repository
        .apply_commit(CommitRequest {
            project_id: project_id.clone(),
            branch_id: branch,
            commit_id: CommitId::parse(format!("commit-{suffix}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("request-{suffix}")).unwrap(),
            request_digest: "identity-test".into(),
            base_snapshot_id: start,
            next_snapshot_id: SnapshotId::parse(format!("snapshot-{suffix}-1")).unwrap(),
            revisions: vec![RevisionRecord {
                project_id: project_id.clone(),
                object_id: ObjectId::parse(format!("draft-{suffix}")).unwrap(),
                object_kind: "draft".into(),
                revision_id: RevisionId::parse(format!("revision-{suffix}")).unwrap(),
                parent_revision_id: None,
                content_digest: "test-digest".into(),
                content_json: json!({"body":"林夕与灯塔"}).to_string(),
            }],
        })
        .unwrap();
    project_id
}

fn publish_fixture(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    suffix: &str,
    batch_suffix: &str,
    status: &str,
) -> String {
    let batch_id = format!("batch-{suffix}-{batch_suffix}");
    let entity = json!({
        "id":format!("linxi-{suffix}"), "kind":"character", "displayName":"林夕",
        "aliases":["阿夕", "林夕"], "evidence":{"quote":"林夕", "start":0, "end":2}
    });
    repository
        .connection
        .execute(
            "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, 1, 2)",
            params![batch_id, project_id.as_str(), format!("draft-{suffix}"),
                format!("revision-{suffix}"), batch_suffix, status, json!([entity]).to_string()],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, ?2, ?3, 'character', '林夕', ?4, ?5)",
            params![project_id.as_str(), batch_id, format!("linxi-{suffix}"),
                entity["aliases"].to_string(), entity["evidence"].to_string()],
        )
        .unwrap();
    for name in ["林夕", "阿夕"] {
        repository
            .connection
            .execute(
                "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, ?2, ?3, ?4)",
                params![project_id.as_str(), batch_id, format!("linxi-{suffix}"), name],
            )
            .unwrap();
    }
    batch_id
}

#[test]
fn entity_index_rebuild_restores_current_and_historical_claims_without_touching_sources() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "a");
    let other_id = seed_project(&mut repository, "b");
    let old = publish_fixture(&repository, &project_id, "a", "old", "stale");
    let current = publish_fixture(&repository, &project_id, "a", "current", "ready");
    publish_fixture(&repository, &other_id, "b", "current", "ready");
    repository
        .connection
        .execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, 'draft-a', ?2)",
            params![project_id.as_str(), current],
        )
        .unwrap();
    assert!(
        repository
            .audit_entity_index(&project_id)
            .unwrap()
            .complete()
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_entity_claims_v2 WHERE batch_id = ?1",
            [&old],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "DELETE FROM story_entity_names_v2 WHERE batch_id = ?1 AND name = '阿夕'",
            [&current],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_entity_claims_v2 SET kind = 'location' WHERE batch_id = ?1",
            [&current],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_entity_claims_v2 SET aliases_json = '{' WHERE batch_id = ?1",
            [&current],
        )
        .unwrap();
    let broken = repository.audit_entity_index(&project_id).unwrap();
    assert_eq!(broken.missing_claims, 1);
    assert_eq!(broken.changed_claims, 1);
    assert_eq!(broken.missing_names, 1);
    assert_eq!(broken.affected_batch_ids, vec![current.clone(), old]);
    let other_before = repository.audit_entity_index(&other_id).unwrap();
    let revision_before = repository.project_revision(&project_id).unwrap();
    let repaired = repository.rebuild_entity_index(&project_id).unwrap();
    assert!(repaired.complete());
    assert_eq!(repaired.published_batches, 2);
    assert_eq!(repaired.expected_claims, 2);
    assert_eq!(repaired.expected_names, 4);
    assert_eq!(
        repository.audit_entity_index(&other_id).unwrap(),
        other_before
    );
    assert_eq!(
        repository.project_revision(&project_id).unwrap(),
        revision_before
    );
    assert_eq!(
        repository
            .resolve_entity_name(&project_id, "阿夕", 10_000)
            .unwrap()["entries"][0]["kind"],
        "character"
    );
}

#[test]
fn malformed_published_source_aborts_rebuild_without_deleting_other_index_rows() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "bad");
    let batch = publish_fixture(&repository, &project_id, "bad", "current", "ready");
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json = '{' WHERE batch_id = ?1",
            [&batch],
        )
        .unwrap();
    assert!(matches!(
        repository.rebuild_entity_index(&project_id),
        Err(RepositoryError::CorruptState)
    ));
    let retained: i64 = repository
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_entity_claims_v2 WHERE batch_id = ?1",
            [&batch],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, 1);
}

use crate::{EntityIdentityRepository, EntityIndexIntegrityRepository, ProjectRevisionRepository};
