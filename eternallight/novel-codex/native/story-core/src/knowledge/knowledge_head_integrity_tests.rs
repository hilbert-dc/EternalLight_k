use rusqlite::params;

use crate::{
    BranchId, ObjectId, ProjectId, ProjectRecord, RepositoryError, SnapshotId,
    SqliteStoryRepository,
};

fn project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let id = ProjectId::parse(format!("head-audit-{suffix}")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: id.clone(),
                title: "Knowledge heads".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("head-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    id
}

fn ready_batch(repository: &SqliteStoryRepository, project_id: &ProjectId, suffix: &str) {
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', '{\"body\":\"林夕\"}')",
        params![format!("head-revision-{suffix}"), project_id.as_str(), format!("draft-{suffix}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, 'draft', ?3)",
        params![project_id.as_str(), format!("draft-{suffix}"), format!("head-revision-{suffix}")],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'ready', 1, 2)",
        params![format!("head-batch-{suffix}"), project_id.as_str(), format!("draft-{suffix}"), format!("head-revision-{suffix}")],
    ).unwrap();
}

#[test]
fn missing_head_marks_job_inconsistent_and_rebuild_restores_only_derived_head() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "primary");
    let other = project(&mut repository, "other");
    ready_batch(&repository, &id, "primary");
    ready_batch(&repository, &other, "other");
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, 'draft-other', 'head-batch-other')",
        [other.as_str()],
    ).unwrap();
    let source_before: String = repository.connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE revision_id = 'head-revision-primary'",
        [], |row| row.get(0),
    ).unwrap();
    let audit = repository.audit_knowledge_heads(&id).unwrap();
    assert_eq!(audit.missing_heads, 1);
    assert_eq!(audit.affected_object_ids, vec!["draft-primary"]);
    let queued = repository.start_knowledge_audit(&id).unwrap();
    let done = repository.advance_knowledge_audit(&id, &queued.id).unwrap();
    assert_eq!(done.verified_consistent, Some(false));
    assert_eq!(done.audit.heads, audit);

    let rebuilt = repository.rebuild_knowledge_heads(&id).unwrap();
    assert!(rebuilt.complete());
    assert_eq!(repository.audit_knowledge_heads(&id).unwrap(), rebuilt);
    assert_eq!(
        repository
            .knowledge_audit_job(&id, &done.id)
            .unwrap()
            .status,
        "stale"
    );
    assert!(repository.audit_knowledge_heads(&other).unwrap().complete());
    let source_after: String = repository.connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE revision_id = 'head-revision-primary'",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(source_after, source_before);
    let second = repository.start_knowledge_audit(&id).unwrap();
    assert_eq!(
        repository
            .advance_knowledge_audit(&id, &second.id)
            .unwrap()
            .verified_consistent,
        Some(true)
    );
}

#[test]
fn wrong_and_orphan_heads_are_detected_and_replaced() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "wrong");
    ready_batch(&repository, &id, "wrong");
    ready_batch(&repository, &id, "orphan");
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, 'draft-wrong', 'head-batch-orphan')",
        [id.as_str()],
    ).unwrap();
    let audit = repository.audit_knowledge_heads(&id).unwrap();
    assert_eq!(audit.changed_heads, 1);
    assert_eq!(audit.missing_heads, 1);
    assert_eq!(audit.extra_heads, 1);
    assert_eq!(
        audit.affected_object_ids,
        vec!["draft-orphan", "draft-wrong"]
    );
    assert!(repository.rebuild_knowledge_heads(&id).unwrap().complete());
}

#[test]
fn invalid_or_ambiguous_ready_source_cannot_be_rebuilt() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "ambiguous");
    ready_batch(&repository, &id, "ambiguous");
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, 'draft-ambiguous', 'head-batch-ambiguous')",
        [id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, created_at, published_at) VALUES ('head-batch-duplicate', ?1, 'draft-ambiguous', 'head-revision-ambiguous', 1, 'other', 'ready', 1, 2)",
        [id.as_str()],
    ).unwrap();
    assert_eq!(
        repository
            .audit_knowledge_heads(&id)
            .unwrap()
            .ambiguous_ready_batches,
        2
    );
    assert!(matches!(
        repository.rebuild_knowledge_heads(&id),
        Err(RepositoryError::CorruptState)
    ));
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET status = 'stale' WHERE batch_id = 'head-batch-duplicate'",
        [],
    ).unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id = NULL WHERE project_id = ?1",
            [id.as_str()],
        )
        .unwrap();
    assert_eq!(
        repository
            .audit_knowledge_heads(&id)
            .unwrap()
            .invalid_ready_batches,
        1
    );
    assert!(matches!(
        repository.rebuild_knowledge_heads(&id),
        Err(RepositoryError::CorruptState)
    ));
    let retained: String = repository
        .connection
        .query_row(
            "SELECT batch_id FROM story_knowledge_heads_v2 WHERE project_id = ?1",
            [id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, "head-batch-ambiguous");
}

#[test]
fn source_invalidation_stales_in_flight_audit_even_without_new_batch() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "invalidation");
    ready_batch(&repository, &id, "invalidation");
    let queued = repository.start_knowledge_audit(&id).unwrap();
    crate::modeling::invalidate_source_modeling(
        &repository.connection,
        &id,
        &ObjectId::parse("draft-invalidation").unwrap(),
    )
    .unwrap();
    assert_eq!(
        repository
            .advance_knowledge_audit(&id, &queued.id)
            .unwrap()
            .status,
        "stale"
    );
    assert!(repository.audit_knowledge_heads(&id).unwrap().complete());
}

use crate::{KnowledgeAuditJobsRepository, KnowledgeHeadIntegrityRepository, ProjectRevisionRepository};
