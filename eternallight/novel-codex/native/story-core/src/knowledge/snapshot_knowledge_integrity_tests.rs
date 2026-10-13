use rusqlite::params;

use crate::{
    BranchId, ObjectId, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository,
};

fn project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let project_id = ProjectId::parse(format!("capture-project-{suffix}")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "History".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("capture-initial-{suffix}")).unwrap(),
        )
        .unwrap();
    project_id
}

fn member(repository: &SqliteStoryRepository, project_id: &ProjectId, suffix: &str) {
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', '{\"body\":\"林夕\"}')",
        params![format!("capture-revision-{suffix}"), project_id.as_str(), format!("capture-draft-{suffix}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, 'main', ?3)",
        params![format!("capture-before-{suffix}"), project_id.as_str(), format!("capture-initial-{suffix}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
        params![format!("capture-before-{suffix}"), format!("capture-draft-{suffix}"), format!("capture-revision-{suffix}")],
    ).unwrap();
}

#[test]
fn explicit_pending_capture_does_not_inherit_later_modeling() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "late");
    member(&repository, &id, "late");
    let before = SnapshotId::parse("capture-before-late").unwrap();
    let object = ObjectId::parse("capture-draft-late").unwrap();
    let captured: Option<String> = repository.connection.query_row(
        "SELECT batch_id FROM story_snapshot_knowledge_v2 WHERE snapshot_id = ?1 AND object_id = ?2",
        params![before.as_str(), object.as_str()], |row| row.get(0),
    ).unwrap();
    assert_eq!(captured, None);
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at, published_at) VALUES ('capture-batch-late', ?1, ?2, 'capture-revision-late', 0, 'test', 'ready', 1, 2)",
        params![id.as_str(), object.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, 'capture-batch-late')",
        params![id.as_str(), object.as_str()],
    ).unwrap();
    assert_eq!(
        repository
            .knowledge_at_snapshot(&id, &before, &object)
            .unwrap()["status"],
        "model_pending"
    );
    repository.connection.execute(
        "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES ('capture-after-late', ?1, 'main', ?2)",
        params![id.as_str(), before.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES ('capture-after-late', ?1, 'capture-revision-late')",
        [object.as_str()],
    ).unwrap();
    let after = SnapshotId::parse("capture-after-late").unwrap();
    assert_eq!(
        repository
            .knowledge_at_snapshot(&id, &after, &object)
            .unwrap()["batchId"],
        "capture-batch-late"
    );
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET status = 'stale' WHERE batch_id = 'capture-batch-late'",
        [],
    ).unwrap();
    assert_eq!(
        repository
            .knowledge_at_snapshot(&id, &after, &object)
            .unwrap()["status"],
        "ready"
    );
    let audit = repository.audit_snapshot_knowledge(&id).unwrap();
    assert!(audit.complete());
    assert_eq!(
        (audit.members, audit.ready_captures, audit.pending_captures),
        (2, 1, 1)
    );
}

#[test]
fn missing_and_cross_project_captures_fail_closed_without_repairing_history() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "target");
    let foreign = project(&mut repository, "foreign");
    member(&repository, &id, "target");
    member(&repository, &foreign, "foreign");
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at, published_at) VALUES ('capture-foreign-batch', ?1, 'capture-draft-foreign', 'capture-revision-foreign', 0, 'test', 'ready', 1, 2)",
        [foreign.as_str()],
    ).unwrap();
    repository
        .connection
        .execute(
            "DELETE FROM story_snapshot_knowledge_v2 WHERE snapshot_id = 'capture-before-target'",
            [],
        )
        .unwrap();
    let before = SnapshotId::parse("capture-before-target").unwrap();
    let object = ObjectId::parse("capture-draft-target").unwrap();
    assert!(matches!(
        repository.knowledge_at_snapshot(&id, &before, &object),
        Err(RepositoryError::CorruptState)
    ));
    let audit = repository.audit_snapshot_knowledge(&id).unwrap();
    assert_eq!((audit.missing_captures, audit.invalid_captures), (1, 0));
    assert!(
        repository
            .audit_snapshot_knowledge(&foreign)
            .unwrap()
            .complete()
    );
    repository.connection.execute(
        "INSERT INTO story_snapshot_knowledge_v2(snapshot_id, object_id, batch_id) VALUES ('capture-before-target', 'capture-draft-target', 'capture-foreign-batch')",
        [],
    ).unwrap();
    assert!(matches!(
        repository.knowledge_at_snapshot(&id, &before, &object),
        Err(RepositoryError::CorruptState)
    ));
    assert_eq!(
        repository
            .audit_snapshot_knowledge(&id)
            .unwrap()
            .invalid_captures,
        1
    );
}

#[test]
fn foreign_member_revision_is_not_treated_as_pending_knowledge() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "bad-member");
    let other = project(&mut repository, "owner");
    member(&repository, &id, "bad-member");
    member(&repository, &other, "owner");
    repository.connection.execute(
        "UPDATE story_snapshot_members_v2 SET revision_id = 'capture-revision-owner' WHERE snapshot_id = 'capture-before-bad-member'",
        [],
    ).unwrap();
    let snapshot = SnapshotId::parse("capture-before-bad-member").unwrap();
    let object = ObjectId::parse("capture-draft-bad-member").unwrap();
    assert!(matches!(
        repository.knowledge_at_snapshot(&id, &snapshot, &object),
        Err(RepositoryError::CorruptState)
    ));
    assert_eq!(
        repository
            .audit_snapshot_knowledge(&id)
            .unwrap()
            .invalid_members,
        1
    );
    assert!(
        repository
            .audit_snapshot_knowledge(&other)
            .unwrap()
            .complete()
    );
}

use crate::{ProjectRevisionRepository, SnapshotKnowledgeIntegrityRepository, SnapshotKnowledgeRepository};
