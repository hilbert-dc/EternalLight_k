use rusqlite::params;
use serde_json::json;

use super::*;
use crate::{
    BranchId, ChapterProjection, CommitId, IdempotencyKey, KnowledgePerspective,
    KnowledgeStateQuery, ProjectRecord, RevisionId, SaveCandidateRequest,
};

fn parsed<T>(value: &str, parse: impl FnOnce(String) -> Result<T, crate::IdError>) -> T {
    parse(value.to_owned()).unwrap()
}

fn save(
    repository: &mut SqliteStoryRepository,
    object: &str,
    kind: &str,
    revision: &str,
    previous: Option<&str>,
    base: &str,
    next: &str,
) {
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: parsed("project-history", ProjectId::parse),
            branch_id: parsed("main", BranchId::parse),
            object_id: parsed(object, ObjectId::parse),
            object_kind: kind.into(),
            revision_id: parsed(revision, RevisionId::parse),
            commit_id: parsed(&format!("commit-{next}"), CommitId::parse),
            idempotency_key: parsed(&format!("request-{next}"), IdempotencyKey::parse),
            request_digest: next.into(),
            expected_project_revision: 0,
            expected_snapshot_id: parsed(base, SnapshotId::parse),
            next_snapshot_id: parsed(next, SnapshotId::parse),
            expected_candidate_revision_id: previous.map(|id| parsed(id, RevisionId::parse)),
            content_digest: revision.into(),
            content_json: json!({"body": revision}).to_string(),
            chapter_projection: (kind == "draft").then(|| ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "Chapter 1".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
}

#[test]
fn snapshot_replays_only_knowledge_visible_at_creation() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = parsed("project-history", ProjectId::parse);
    let draft = parsed("draft-chapter-1", ObjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "History".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parsed("main", BranchId::parse),
            parsed("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    save(
        &mut repository,
        "draft-chapter-1",
        "draft",
        "draft-revision-1",
        None,
        "snapshot-0",
        "snapshot-before-model",
    );

    let before = repository
        .knowledge_at_snapshot(
            &project,
            &parsed("snapshot-before-model", SnapshotId::parse),
            &draft,
        )
        .unwrap();
    assert_eq!(before["status"], "model_pending");
    let reader_state = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: Some(&parsed("snapshot-before-model", SnapshotId::parse)),
            perspective: KnowledgePerspective::Reader,
            world_time: None,
            epistemic_time: None,
            reading_chapter: Some(1),
            owner_id: None,
            entity_id: None,
            predicate: None,
        })
        .unwrap();
    assert_eq!(reader_state["entries"], json!([]));
    assert_eq!(reader_state["coverage"]["unknownChapterSources"], 1);
    assert_eq!(reader_state["coverage"]["truncatedSources"], false);
    assert_eq!(reader_state["coverage"]["complete"], false);

    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'ready', ?5, '[]', 1, 2)",
        params!["batch-1", project.as_str(), draft.as_str(), "draft-revision-1", json!([{"id":"assertion-1"}]).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, 'batch-1')",
        params![project.as_str(), draft.as_str()],
    ).unwrap();

    save(
        &mut repository,
        "artifact-brief",
        "brief",
        "brief-revision-1",
        None,
        "snapshot-before-model",
        "snapshot-with-model",
    );
    save(
        &mut repository,
        "draft-chapter-1",
        "draft",
        "draft-revision-2",
        Some("draft-revision-1"),
        "snapshot-with-model",
        "snapshot-after-edit",
    );

    let historic = repository
        .knowledge_at_snapshot(
            &project,
            &parsed("snapshot-with-model", SnapshotId::parse),
            &draft,
        )
        .unwrap();
    assert_eq!(historic["status"], "ready");
    assert_eq!(historic["sourceRevisionId"], "draft-revision-1");
    assert_eq!(historic["batchId"], "batch-1");
    assert_eq!(historic["assertions"], json!([{"id":"assertion-1"}]));
    assert_eq!(
        repository.chapter_knowledge(&project, "chapter-1").unwrap()["status"],
        "model_pending"
    );
    assert_eq!(
        repository
            .knowledge_at_snapshot(
                &project,
                &parsed("snapshot-after-edit", SnapshotId::parse),
                &draft
            )
            .unwrap()["status"],
        "model_pending"
    );
    assert_eq!(
        repository
            .knowledge_at_snapshot(
                &project,
                &parsed("snapshot-before-model", SnapshotId::parse),
                &draft
            )
            .unwrap(),
        before
    );
}

#[test]
fn confirmed_fact_history_follows_acceptance_and_undo_ancestry() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = parsed("project-history", ProjectId::parse);
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "History".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            parsed("main", BranchId::parse),
            parsed("snapshot-0", SnapshotId::parse),
        )
        .unwrap();
    save(
        &mut repository,
        "artifact-brief",
        "brief",
        "brief-revision-1",
        None,
        "snapshot-0",
        "snapshot-before-accept",
    );
    save(
        &mut repository,
        "artifact-brief",
        "brief",
        "brief-revision-2",
        Some("brief-revision-1"),
        "snapshot-before-accept",
        "snapshot-accepted",
    );
    save(
        &mut repository,
        "artifact-brief",
        "brief",
        "brief-revision-3",
        Some("brief-revision-2"),
        "snapshot-accepted",
        "snapshot-undone",
    );

    repository.connection.execute(
        "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, object_id, object_kind, accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation, undone_by_commit_id) VALUES ('accept-1', ?1, 'main', 'artifact-brief', 'brief', 'brief-revision-2', 0, 1, 'snapshot-before-accept', 'snapshot-accepted', 'accept', 'undo-1')",
        [project.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_acceptances_v2(commit_id, project_id, branch_id, object_id, object_kind, accepted_revision_id, base_revision, result_revision, base_snapshot_id, result_snapshot_id, operation, target_commit_id) VALUES ('undo-1', ?1, 'main', 'artifact-brief', 'brief', 'brief-revision-2', 1, 2, 'snapshot-accepted', 'snapshot-undone', 'undo', 'accept-1')",
        [project.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_facts_v2(fact_id, project_id, payload_json, accepted_by_commit_id, project_revision, reverted_by_commit_id) VALUES ('fact-1', ?1, ?2, 'accept-1', 1, 'undo-1')",
        params![project.as_str(), json!({"id":"fact-1","modelingBatchId":"batch-1","modelingAssertionIndex":0}).to_string()],
    ).unwrap();

    let confirmed = |snapshot: &str| {
        repository
            .trace_confirmed_facts(&project, Some(&parsed(snapshot, SnapshotId::parse)))
            .unwrap()
    };
    assert!(confirmed("snapshot-before-accept").is_empty());
    assert_eq!(
        confirmed("snapshot-accepted")
            .get(&("batch-1".into(), 0))
            .map(String::as_str),
        Some("fact-1")
    );
    assert!(confirmed("snapshot-undone").is_empty());

    repository
        .connection
        .execute(
            "UPDATE story_facts_v2 SET reverted_by_commit_id = NULL WHERE fact_id = 'fact-1'",
            [],
        )
        .unwrap();
    assert_eq!(
        repository.facts(&project),
        Err(RepositoryError::CorruptState)
    );
    assert_eq!(
        repository.trace_confirmed_facts(
            &project,
            Some(&parsed("snapshot-undone", SnapshotId::parse))
        ),
        Err(RepositoryError::CorruptState)
    );
    repository
        .connection
        .execute(
            "UPDATE story_facts_v2 SET reverted_by_commit_id = 'undo-1' WHERE fact_id = 'fact-1'",
            [],
        )
        .unwrap();

    repository.connection.execute(
        "UPDATE story_acceptances_v2 SET target_commit_id = 'wrong-acceptance' WHERE commit_id = 'undo-1'",
        [],
    ).unwrap();
    assert_eq!(
        repository.facts(&project),
        Err(RepositoryError::CorruptState)
    );
    repository.connection.execute(
        "UPDATE story_acceptances_v2 SET target_commit_id = 'accept-1' WHERE commit_id = 'undo-1'",
        [],
    ).unwrap();

    repository.connection.execute(
        "UPDATE story_acceptances_v2 SET project_id = 'other-project' WHERE commit_id = 'accept-1'",
        [],
    ).unwrap();
    assert_eq!(
        repository.facts(&project),
        Err(RepositoryError::CorruptState)
    );
    assert_eq!(
        repository.trace_confirmed_facts(
            &project,
            Some(&parsed("snapshot-accepted", SnapshotId::parse))
        ),
        Err(RepositoryError::CorruptState)
    );
}

use crate::{KnowledgeProjectionRepository, ModelingRepository, ProjectRevisionRepository, SnapshotKnowledgeRepository, WorkflowRepository};
