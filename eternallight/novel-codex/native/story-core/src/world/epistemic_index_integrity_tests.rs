use rusqlite::params;
use serde_json::json;

use crate::{
    BranchId, ProjectId, ProjectRecord, RepositoryError, SnapshotId, SqliteStoryRepository,
};

fn seed(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let id = ProjectId::parse(format!("epistemic-project-{suffix}")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: id.clone(),
                title: "Knowledge".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("epistemic-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    id
}

fn batch(repository: &SqliteStoryRepository, project_id: &ProjectId, suffix: &str) -> String {
    let id = format!("epistemic-batch-{suffix}");
    let assertion = json!({"kind":"epistemic","subjectId":"secret","propositionId":"secret",
        "predicate":"knows","epistemicOwnerId":"linxi","epistemicState":"known",
        "epistemicTime":5,"value":true,"evidence":{"quote":"林夕知道秘密","start":0,"end":6}});
    let entities = json!([{"id":"linxi","kind":"character"},{"id":"secret","kind":"proposition"}]);
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', '{}')",
        params![format!("revision-{suffix}"), project_id.as_str(), format!("draft-{suffix}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'stale', ?5, ?6, 1, 2)",
        params![id, project_id.as_str(), format!("draft-{suffix}"), format!("revision-{suffix}"),
            json!([assertion]).to_string(), entities.to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json) VALUES (?1, ?2, 0, 'linxi', 'secret', ?2, 'known', 5, ?3)",
        params![project_id.as_str(), id, assertion.to_string()],
    ).unwrap();
    id
}

#[test]
fn epistemic_index_audit_and_rebuild_preserve_other_projects_and_authority() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed(&mut repository, "a");
    let other_id = seed(&mut repository, "b");
    let first = batch(&repository, &project_id, "a");
    batch(&repository, &other_id, "b");
    assert!(
        repository
            .audit_epistemic_index(&project_id)
            .unwrap()
            .complete()
    );
    repository.connection.execute(
        "UPDATE story_epistemic_claims_v1 SET proposition_source_batch_id = 'wrong-source' WHERE batch_id = ?1",
        [&first],
    ).unwrap();
    let wrong_source = repository.audit_epistemic_index(&project_id).unwrap();
    assert_eq!(wrong_source.changed_claims, 1);
    assert!(
        repository
            .rebuild_epistemic_index(&project_id)
            .unwrap()
            .complete()
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_epistemic_claims_v1 WHERE batch_id = ?1",
            [&first],
        )
        .unwrap();
    let broken = repository.audit_epistemic_index(&project_id).unwrap();
    assert_eq!(
        (
            broken.missing_claims,
            broken.changed_claims,
            broken.extra_claims
        ),
        (1, 0, 0)
    );
    assert_eq!(broken.affected_batch_ids, vec![first]);
    let other_before = repository.audit_epistemic_index(&other_id).unwrap();
    let revision_before = repository.project_revision(&project_id).unwrap();
    let repaired = repository.rebuild_epistemic_index(&project_id).unwrap();
    assert!(repaired.complete());
    assert_eq!(repaired.expected_claims, 1);
    assert_eq!(
        repository.audit_epistemic_index(&other_id).unwrap(),
        other_before
    );
    assert_eq!(
        repository.project_revision(&project_id).unwrap(),
        revision_before
    );
}

#[test]
fn invalid_epistemic_source_rolls_back_rebuild() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed(&mut repository, "invalid");
    let id = batch(&repository, &project_id, "invalid");
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json = ?1 WHERE batch_id = ?2",
            params![
                json!([{"id":"linxi","kind":"character"},{"id":"secret","kind":"location"}])
                    .to_string(),
                id
            ],
        )
        .unwrap();
    assert!(matches!(
        repository.rebuild_epistemic_index(&project_id),
        Err(RepositoryError::CorruptState)
    ));
    let retained: i64 = repository
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_epistemic_claims_v1 WHERE batch_id = ?1",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, 1);
}

use crate::{EpistemicIndexIntegrityRepository, ProjectRevisionRepository};
