use crate::{
    BranchId, CommitId, CommitRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, RevisionRecord, SearchQuery, SearchScope, SnapshotId,
    SqliteStoryRepository,
};

fn project(repository: &mut SqliteStoryRepository, name: &str) -> ProjectId {
    let project_id = ProjectId::parse(name).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: name.into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("snapshot-{name}-0")).unwrap(),
        )
        .unwrap();
    project_id
}

fn add_revision(repository: &mut SqliteStoryRepository, project_id: &ProjectId, body: &str) {
    let name = project_id.as_str();
    repository
        .apply_commit(CommitRequest {
            project_id: project_id.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            commit_id: CommitId::parse(format!("commit-{name}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("request-{name}")).unwrap(),
            request_digest: format!("digest-{name}"),
            base_snapshot_id: SnapshotId::parse(format!("snapshot-{name}-0")).unwrap(),
            next_snapshot_id: SnapshotId::parse(format!("snapshot-{name}-1")).unwrap(),
            revisions: vec![RevisionRecord {
                project_id: project_id.clone(),
                object_id: ObjectId::parse(format!("draft-{name}")).unwrap(),
                object_kind: "draft".into(),
                revision_id: RevisionId::parse(format!("revision-{name}")).unwrap(),
                parent_revision_id: None,
                content_digest: format!("content-{name}"),
                content_json: serde_json::json!({"body":body,"title":format!("密函 {name}")})
                    .to_string(),
            }],
        })
        .unwrap();
}

#[test]
fn document_gram_audit_uses_a_bounded_covering_lookup() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let first = project(&mut repository, "audit-lookup-first");
    let second = project(&mut repository, "audit-lookup-second");
    add_revision(&mut repository, &first, "林夕找到密函");
    add_revision(&mut repository, &second, "另一部作品的密函");
    let doc_id: i64 = repository.connection.query_row(
        "SELECT doc_id FROM story_search_documents_v2 WHERE project_id = ?1 AND field_path = '/body'",
        [first.as_str()], |row| row.get(0),
    ).unwrap();
    let mut statement = repository.connection.prepare(
        "EXPLAIN QUERY PLAN SELECT gram FROM story_search_grams_v2 WHERE project_id = ?1 AND doc_id = ?2 ORDER BY gram",
    ).unwrap();
    let plan = statement
        .query_map(rusqlite::params![first.as_str(), doc_id], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("SEARCH")
            && step.contains("COVERING INDEX")
            && step.contains("doc_id=?")
            && step.contains("project_id=?")),
        "{plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "{plan:?}"
    );
    assert!(repository.audit_search_index(&first).unwrap().complete());
    assert!(repository.audit_search_index(&second).unwrap().complete());
}

#[test]
fn full_rebuild_restores_corrupt_derived_index_without_changing_story_or_other_project() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let first = project(&mut repository, "project-one");
    let second = project(&mut repository, "project-two");
    add_revision(&mut repository, &first, "林夕找到密函");
    add_revision(&mut repository, &second, "另一部作品的密函");
    let baseline = repository.audit_search_index(&first).unwrap();
    assert!(baseline.complete());
    assert_eq!(baseline.revision_count, 1);
    let other = repository.audit_search_index(&second).unwrap();
    let first_snapshot = SnapshotId::parse("snapshot-project-one-1").unwrap();
    let first_page = repository
        .search_documents(SearchQuery {
            project_id: &first,
            snapshot_id: Some(&first_snapshot),
            text: "密函",
            kind: None,
            scope: SearchScope::All,
            limit: 1,
            cursor: None,
        })
        .unwrap();
    assert_eq!(
        first_page["results"][0]["sourceRevisionId"],
        "revision-project-one"
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_search_grams_v2 WHERE project_id = ?1 AND gram = ?2",
            rusqlite::params![first.as_str(), "密函"],
        )
        .unwrap();
    let damaged = repository.audit_search_index(&first).unwrap();
    assert!(!damaged.complete());
    assert_eq!(
        damaged.mismatched_revision_ids,
        vec!["revision-project-one"]
    );
    let restored = repository
        .rebuild_search_index(&first, baseline.index_generation)
        .unwrap();
    assert!(restored.complete());
    assert_eq!(restored.index_generation, baseline.index_generation + 1);
    assert_eq!(repository.audit_search_index(&second).unwrap(), other);
    assert_eq!(repository.project_revision(&first).unwrap(), 0);
    assert_eq!(
        repository
            .revision_content(&RevisionId::parse("revision-project-one").unwrap())
            .unwrap(),
        Some(serde_json::json!({"body":"林夕找到密函","title":"密函 project-one"}).to_string())
    );
    let found = repository
        .search_documents(SearchQuery {
            project_id: &first,
            snapshot_id: Some(&first_snapshot),
            text: "密函",
            kind: None,
            scope: SearchScope::All,
            limit: 1,
            cursor: None,
        })
        .unwrap();
    assert_eq!(found["results"][0]["evidence"]["quote"], "密函");
    assert!(matches!(
        repository.rebuild_search_index(&first, baseline.index_generation),
        Err(RepositoryError::InvalidContent(_))
    ));
}

#[test]
fn search_cursor_is_invalidated_when_derived_index_is_rebuilt() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "project-cursor");
    add_revision(&mut repository, &id, "密函与密函");
    let snapshot = SnapshotId::parse("snapshot-project-cursor-1").unwrap();
    let page = repository
        .search_documents(SearchQuery {
            project_id: &id,
            snapshot_id: Some(&snapshot),
            text: "密函",
            kind: None,
            scope: SearchScope::All,
            limit: 1,
            cursor: None,
        })
        .unwrap();
    let cursor = page["nextCursor"]
        .as_str()
        .expect("expected a cursor before rebuild");
    repository.rebuild_search_index(&id, 0).unwrap();
    assert!(matches!(
        repository.search_documents(SearchQuery {
            project_id: &id,
            snapshot_id: Some(&snapshot),
            text: "密函",
            kind: None,
            scope: SearchScope::All,
            limit: 1,
            cursor: Some(cursor),
        }),
        Err(RepositoryError::InvalidContent(_))
    ));
}

#[test]
fn foreign_project_gram_cannot_route_another_projects_document_into_search() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let first = project(&mut repository, "project-a");
    let second = project(&mut repository, "project-b");
    add_revision(&mut repository, &first, "本作品的线索");
    add_revision(&mut repository, &second, "另一部小说专有词");
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, 'draft', ?3)",
        rusqlite::params![second.as_str(), "draft-project-b", "revision-project-b"],
    ).unwrap();
    let foreign_doc_id: i64 = repository.connection.query_row(
        "SELECT doc_id FROM story_search_documents_v2 WHERE project_id = ?1 AND field_path = '/body'",
        [second.as_str()], |row| row.get(0),
    ).unwrap();
    repository
        .connection
        .execute(
            "INSERT INTO story_search_grams_v2(project_id, gram, doc_id) VALUES (?1, '专有', ?2)",
            rusqlite::params![first.as_str(), foreign_doc_id],
        )
        .unwrap();
    let result = repository
        .search_documents(SearchQuery {
            project_id: &first,
            snapshot_id: None,
            text: "专有",
            kind: None,
            scope: SearchScope::All,
            limit: 10,
            cursor: None,
        })
        .unwrap();
    assert_eq!(result["results"], serde_json::json!([]));
    let audit = repository.audit_search_index(&first).unwrap();
    assert_eq!(audit.orphaned_grams, 1);
    assert!(!audit.complete());
    assert!(
        repository
            .rebuild_search_index(&first, audit.index_generation)
            .unwrap()
            .complete()
    );
    assert!(repository.audit_search_index(&second).unwrap().complete());
}

#[test]
fn failed_rebuild_rolls_back_derived_rows_and_generation() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let id = project(&mut repository, "project-rollback");
    add_revision(&mut repository, &id, "林夕找到密函");
    let before = repository.audit_search_index(&id).unwrap();
    let original_content = repository
        .revision_content(&RevisionId::parse("revision-project-rollback").unwrap())
        .unwrap()
        .unwrap();
    let count_rows = |repository: &SqliteStoryRepository| {
        repository
            .connection
            .query_row(
                "SELECT COUNT(*) FROM story_search_documents_v2 WHERE project_id = ?1",
                [id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    };
    assert!(count_rows(&repository) > 0);
    repository
        .connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json = 'not-json' WHERE project_id = ?1",
            [id.as_str()],
        )
        .unwrap();
    assert!(matches!(
        repository.rebuild_search_index(&id, before.index_generation),
        Err(RepositoryError::CorruptState)
    ));
    assert_eq!(count_rows(&repository), before.document_count as i64);
    assert_eq!(
        repository.search_index_generation(&id).unwrap(),
        before.index_generation
    );
    repository
        .connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json = ?2 WHERE project_id = ?1",
            rusqlite::params![id.as_str(), original_content],
        )
        .unwrap();
    assert_eq!(repository.audit_search_index(&id).unwrap(), before);
}

use crate::{ProjectRevisionRepository, SearchIndexRepository, SearchRebuildRepository};
