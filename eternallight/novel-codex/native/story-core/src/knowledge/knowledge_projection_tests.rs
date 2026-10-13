use rusqlite::params;
use serde_json::json;

use super::*;
use crate::{BranchId, ProjectRecord};

#[test]
fn character_knowledge_reaches_early_chapters_and_reports_stale_sources() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("character-long-work").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "Long work".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("character-long-initial").unwrap(),
        )
        .unwrap();
    let claim = json!({
        "kind":"epistemic", "subjectId":"secret", "predicate":"knows",
        "propositionId":"secret", "epistemicOwnerId":"linxi",
        "epistemicState":"known", "epistemicTime":5, "value":true,
        "evidence":{"quote":"林夕知道秘密","start":0,"end":6}
    });
    for chapter in 1..=231 {
        let chapter_id = format!("chapter-{chapter}");
        let object_id = format!("draft-{chapter_id}");
        let revision_id = format!("revision-{chapter}");
        let batch_id = format!("batch-{chapter}");
        repository.connection.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
            params![chapter_id, project.as_str(), chapter, chapter_id],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', ?4)",
            params![revision_id, project.as_str(), object_id, json!({"chapterNumber":chapter}).to_string()],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, 'draft', ?3)",
            params![project.as_str(), object_id, revision_id],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'ready', ?5, '[]', 1, 2)",
            params![batch_id, project.as_str(), object_id, revision_id,
                if chapter == 1 { json!([claim]).to_string() } else { "[]".into() }],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3)",
            params![project.as_str(), object_id, batch_id],
        ).unwrap();
    }
    let historical_snapshot = SnapshotId::parse("character-long-history").unwrap();
    repository.connection.execute(
        "INSERT INTO story_snapshots_v2(snapshot_id, project_id, branch_id, parent_snapshot_id) VALUES (?1, ?2, 'main', 'character-long-initial')",
        params![historical_snapshot.as_str(), project.as_str()],
    ).unwrap();
    for chapter in 1..=231 {
        repository.connection.execute(
            "INSERT INTO story_snapshot_members_v2(snapshot_id, object_id, revision_id) VALUES (?1, ?2, ?3)",
            params![historical_snapshot.as_str(), format!("draft-chapter-{chapter}"), format!("revision-{chapter}")],
        ).unwrap();
    }
    let at = |time| {
        repository
            .knowledge_state(KnowledgeStateQuery {
                project_id: &project,
                snapshot_id: None,
                perspective: KnowledgePerspective::Character,
                world_time: None,
                epistemic_time: Some(time),
                reading_chapter: None,
                owner_id: Some("linxi"),
                entity_id: None,
                predicate: None,
            })
            .unwrap()
    };
    assert_eq!(at(4)["entries"], json!([]));
    assert_eq!(at(4)["coverage"]["complete"], true);
    let known = at(6);
    assert_eq!(known["entries"].as_array().unwrap().len(), 1);
    assert_eq!(known["entries"][0]["sourceChapter"], 1);
    assert_eq!(known["entries"][0]["assertionIndex"], 0);
    assert_eq!(known["coverage"]["complete"], true);
    let historical = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: Some(&historical_snapshot),
            perspective: KnowledgePerspective::Character,
            world_time: None,
            epistemic_time: Some(6),
            reading_chapter: Some(231),
            owner_id: Some("linxi"),
            entity_id: None,
            predicate: None,
        })
        .unwrap();
    assert_eq!(historical["entries"].as_array().unwrap().len(), 1);
    assert_eq!(historical["entries"][0]["sourceChapter"], 1);
    assert_eq!(historical["coverage"]["complete"], true);
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = ?1 WHERE batch_id = 'batch-131'",
        [json!([claim]).to_string()],
    ).unwrap();
    let prior_chapter_only = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: None,
            perspective: KnowledgePerspective::Character,
            world_time: None,
            epistemic_time: Some(6),
            reading_chapter: Some(1),
            owner_id: Some("linxi"),
            entity_id: None,
            predicate: None,
        })
        .unwrap();
    assert_eq!(prior_chapter_only["entries"].as_array().unwrap().len(), 1);
    assert_eq!(prior_chapter_only["entries"][0]["sourceChapter"], 1);
    assert_eq!(prior_chapter_only["coverage"]["complete"], true);
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = '[]' WHERE batch_id = 'batch-131'",
        [],
    ).unwrap();

    for chapter in 2..=201 {
        let batch_id = format!("batch-{chapter}");
        repository
            .connection
            .execute(
                "UPDATE story_modeling_batches_v2 SET assertions_json = ?1 WHERE batch_id = ?2",
                params![json!([claim]).to_string(), batch_id],
            )
            .unwrap();
    }
    let many_relevant = at(6);
    assert_eq!(many_relevant["entries"].as_array().unwrap().len(), 200);
    assert_eq!(many_relevant["coverage"]["truncatedSources"], true);
    assert_eq!(many_relevant["coverage"]["complete"], false);
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = '[]' WHERE batch_id != 'batch-1'",
        [],
    ).unwrap();

    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('revision-new', ?1, 'draft-chapter-1', 'draft', 'new', '{}')",
        [project.as_str()],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_object_heads_v2 SET candidate_revision_id = 'revision-new' WHERE project_id = ?1 AND object_id = 'draft-chapter-1'",
        [project.as_str()],
    ).unwrap();
    let stale = at(6);
    assert_eq!(stale["entries"], json!([]));
    assert_eq!(
        stale["coverage"]["pendingSources"],
        json!(["draft-chapter-1"])
    );
    assert_eq!(stale["coverage"]["complete"], false);
    repository.connection.execute(
        "DELETE FROM story_knowledge_heads_v2 WHERE project_id = ?1 AND object_id IN (SELECT 'draft-' || chapter_id FROM story_chapters_v2 WHERE project_id = ?1 AND number BETWEEN 2 AND 101)",
        [project.as_str()],
    ).unwrap();
    let many_pending = at(6);
    assert_eq!(
        many_pending["coverage"]["pendingSources"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    assert_eq!(many_pending["coverage"]["truncatedSources"], true);
    assert_eq!(many_pending["coverage"]["complete"], false);
    repository.connection.execute(
        "DELETE FROM story_snapshot_knowledge_v2 WHERE snapshot_id = ?1 AND object_id = 'draft-chapter-200'",
        [historical_snapshot.as_str()],
    ).unwrap();
    assert!(matches!(
        repository.knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: Some(&historical_snapshot),
            perspective: KnowledgePerspective::Character,
            world_time: None,
            epistemic_time: Some(6),
            reading_chapter: Some(231),
            owner_id: Some("linxi"),
            entity_id: None,
            predicate: None,
        }),
        Err(RepositoryError::CorruptState)
    ));
}

use crate::{KnowledgeProjectionRepository, ProjectRevisionRepository};
