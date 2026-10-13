use rusqlite::params;
use serde_json::json;

use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};

fn project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let project_id = ProjectId::parse(format!("relation-audit-project-{suffix}")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Audit".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("relation-audit-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    project_id
}

fn batch(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    number: usize,
    assertions: &str,
) {
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', '{\"body\":\"林夕在港口\"}')",
        params![format!("relation-audit-revision-{number}"), project_id.as_str(),
            format!("draft-relation-audit-{number}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'stale', ?5, ?6, 1, 2)",
        params![format!("relation-audit-batch-{number}"), project_id.as_str(),
            format!("draft-relation-audit-{number}"), format!("relation-audit-revision-{number}"), assertions,
            json!([{"id":"linxi-at-port","kind":"proposition"}]).to_string()],
    ).unwrap();
}

#[test]
fn relation_audit_checkpoint_survives_restart_and_rebuild_invalidates_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = project(&mut repository, "long");
    for number in 0..205 {
        batch(&repository, &project_id, number, "[]");
    }
    let queued = repository.start_relation_audit(&project_id).unwrap();
    assert_eq!(
        repository.start_relation_audit(&project_id).unwrap(),
        queued
    );
    let first = repository
        .advance_relation_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(first.status, "running");
    assert_eq!(first.audit.published_batches, 25);
    assert_eq!(first.verified_consistent, None);
    let paused = repository
        .control_relation_audit(&project_id, &first.id, "pause", first.version)
        .unwrap();
    drop(repository);
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        reopened
            .relation_audit_job(&project_id, &queued.id)
            .unwrap(),
        paused
    );
    let resumed = reopened
        .control_relation_audit(&project_id, &paused.id, "resume", paused.version)
        .unwrap();
    assert_eq!(resumed.status, "queued");
    let done = loop {
        let next = reopened
            .advance_relation_audit(&project_id, &queued.id)
            .unwrap();
        if next.status == "completed" {
            break next;
        }
    };
    assert_eq!(done.verified_consistent, Some(true));
    assert_eq!(
        done.audit,
        reopened.audit_relation_index(&project_id).unwrap()
    );
    assert_eq!(reopened.next_relation_audit_job().unwrap(), None);
    reopened.rebuild_relation_index(&project_id).unwrap();
    let stale = reopened.relation_audit_job(&project_id, &done.id).unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.verified_consistent, None);
}

#[test]
fn relation_audit_detects_missing_and_orphan_rows_and_stale_source() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository, "missing");
    batch(
        &repository,
        &project_id,
        0,
        &json!([{
            "kind":"relation", "subjectId":"linxi", "targetId":"port",
            "predicate":"locatedAt", "relationType":"locatedAt", "propositionId":"linxi-at-port", "value":true,
            "evidence":{"quote":"林夕在港口", "start":0,"end":5}
        }])
        .to_string(),
    );
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at) VALUES ('relation-unpublished', ?1, 'draft-relation-audit-0', 'relation-audit-revision-0', 0, 'unpublished', 'queued', 1)",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_relation_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json) VALUES (?1, 'relation-unpublished', 0, 'ghost', 'port', 'locatedAt', '{}')",
        [project_id.as_str()],
    ).unwrap();
    let queued = repository.start_relation_audit(&project_id).unwrap();
    let done = repository
        .advance_relation_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(false));
    assert_eq!(done.audit.missing_claims, 1);
    assert_eq!(done.audit.extra_claims, 1);
    assert_eq!(
        done.audit,
        repository.audit_relation_index(&project_id).unwrap()
    );
    repository.rebuild_relation_index(&project_id).unwrap();
    let next = repository.start_relation_audit(&project_id).unwrap();
    batch(&repository, &project_id, 1, "[]");
    let stale = repository
        .advance_relation_audit(&project_id, &next.id)
        .unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.verified_consistent, None);
}

#[test]
fn malformed_relation_source_fails_without_changing_authority() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = project(&mut repository, "bad");
    batch(&repository, &project_id, 0, "{");
    let revision = repository.project_revision(&project_id).unwrap();
    let queued = repository.start_relation_audit(&project_id).unwrap();
    let failed = repository
        .advance_relation_audit(&project_id, &queued.id)
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.verified_consistent, None);
    assert!(failed.error.unwrap().contains("relation-audit-batch-0"));
    assert_eq!(repository.project_revision(&project_id).unwrap(), revision);
}

#[test]
fn relation_audit_queue_has_global_backpressure() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    for index in 0..16 {
        let id = project(&mut repository, &format!("queue-{index}"));
        repository.start_relation_audit(&id).unwrap();
    }
    let overflow = project(&mut repository, "queue-overflow");
    assert!(repository.start_relation_audit(&overflow).is_err());
}

use crate::{ProjectRevisionRepository, RelationAuditJobsRepository, RelationIndexIntegrityRepository};
