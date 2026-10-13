use rusqlite::params;
use serde_json::json;

use crate::{
    BranchId, CommitId, CommitRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RevisionId, RevisionRecord, SnapshotId, SqliteStoryRepository,
};

fn project(repository: &mut SqliteStoryRepository) -> ProjectId {
    let project_id = ProjectId::parse("entity-audit-project").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let start = SnapshotId::parse("entity-audit-snapshot-0").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Audit".into(),
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
            commit_id: CommitId::parse("entity-audit-commit").unwrap(),
            idempotency_key: IdempotencyKey::parse("entity-audit-request").unwrap(),
            request_digest: "entity-audit".into(),
            base_snapshot_id: start,
            next_snapshot_id: SnapshotId::parse("entity-audit-snapshot-1").unwrap(),
            revisions: vec![RevisionRecord {
                project_id: project_id.clone(),
                object_id: ObjectId::parse("draft-audit").unwrap(),
                object_kind: "draft".into(),
                revision_id: RevisionId::parse("revision-audit").unwrap(),
                parent_revision_id: None,
                content_digest: "digest".into(),
                content_json: json!({"body":"林夕"}).to_string(),
            }],
        })
        .unwrap();
    project_id
}

fn batch(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    number: usize,
    entities: &str,
) {
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, entities_json, created_at, published_at) VALUES (?1, ?2, 'draft-audit', 'revision-audit', 0, ?3, 'stale', ?4, 1, 2)",
        params![format!("entity-batch-{number}"), project_id.as_str(), format!("extractor-{number}"), entities],
    ).unwrap();
}

#[test]
fn entity_audit_checkpoint_survives_restart_and_rebuild_invalidates_finished_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = project(&mut repository);
    for number in 0..205 {
        batch(&repository, &project_id, number, "[]");
    }
    let queued = repository.start_entity_audit(&project_id).unwrap();
    assert_eq!(repository.start_entity_audit(&project_id).unwrap(), queued);
    let first = repository
        .advance_entity_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(first.status, "running");
    assert_eq!(first.audit.published_batches, 25);
    let paused = repository
        .control_entity_audit(&project_id, &first.id, "pause", first.version)
        .unwrap();
    drop(repository);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened.entity_audit_job(&project_id, &queued.id).unwrap(),
        paused
    );
    let resumed = reopened
        .control_entity_audit(&project_id, &paused.id, "resume", paused.version)
        .unwrap();
    assert_eq!(resumed.status, "queued");
    assert_eq!(
        reopened
            .advance_entity_audit(&project_id, &queued.id)
            .unwrap()
            .audit
            .published_batches,
        50
    );
    let done = loop {
        let next = reopened
            .advance_entity_audit(&project_id, &queued.id)
            .unwrap();
        if next.status == "completed" {
            break next;
        }
    };
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    assert_eq!(
        done.audit,
        reopened.audit_entity_index(&project_id).unwrap()
    );
    assert_eq!(reopened.next_entity_audit_job().unwrap(), None);
    reopened.rebuild_entity_index(&project_id).unwrap();
    let stale = reopened.entity_audit_job(&project_id, &done.id).unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.verified_consistent, None);
    assert_ne!(
        reopened.start_entity_audit(&project_id).unwrap().id,
        done.id
    );
}

#[test]
fn entity_audit_detects_missing_rows_and_stale_source_without_claiming_success() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository);
    batch(
        &repository,
        &project_id,
        0,
        &json!([{
            "id":"linxi", "kind":"character", "displayName":"林夕", "aliases":[],
            "evidence":{"quote":"林夕","start":0,"end":2}
        }])
        .to_string(),
    );
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, entities_json, created_at) VALUES ('unpublished-batch', ?1, 'draft-audit', 'revision-audit', 0, 'unpublished', 'queued', '[]', 1)",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, 'unpublished-batch', 'ghost', '幽灵')",
        [project_id.as_str()],
    ).unwrap();
    let first = repository.start_entity_audit(&project_id).unwrap();
    let done = repository
        .advance_entity_audit(&project_id, &first.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(false));
    assert_eq!(done.audit.missing_claims, 1);
    assert_eq!(done.audit.missing_names, 1);
    assert_eq!(done.audit.extra_names, 1);
    assert_eq!(
        done.audit,
        repository.audit_entity_index(&project_id).unwrap()
    );
    let repaired = repository.rebuild_entity_index(&project_id).unwrap();
    assert!(repaired.complete());
    let started = repository.start_entity_audit(&project_id).unwrap();
    batch(&repository, &project_id, 1, "[]");
    let stale = repository
        .advance_entity_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.verified_consistent, None);
}

#[test]
fn malformed_published_payload_fails_audit_without_changing_authority() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository);
    batch(&repository, &project_id, 0, "{");
    let revision_before = repository.project_revision(&project_id).unwrap();
    let started = repository.start_entity_audit(&project_id).unwrap();
    let failed = repository
        .advance_entity_audit(&project_id, &started.id)
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.verified_consistent, None);
    assert!(failed.error.unwrap().contains("entity-batch-0"));
    assert_eq!(
        repository.project_revision(&project_id).unwrap(),
        revision_before
    );
}

use crate::{EntityAuditJobsRepository, EntityIndexIntegrityRepository, ProjectRevisionRepository};
