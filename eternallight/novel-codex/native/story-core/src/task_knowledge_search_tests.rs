use rusqlite::params;
use serde_json::json;

use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};

#[test]
fn indexed_writer_knowledge_finds_early_evidence_beyond_hundred_chapters() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = ProjectId::parse("long-writer-knowledge").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "长篇".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("long-writer-initial").unwrap(),
        )
        .unwrap();
    for number in 1..=131 {
        let chapter_id = format!("c{number}");
        let object_id = format!("draft-{chapter_id}");
        let revision_id = format!("revision-{number}");
        let batch_id = format!("batch-{number}");
        repository.connection.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
            params![chapter_id, project_id.as_str(), number, chapter_id],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', ?4)",
            params![revision_id, project_id.as_str(), object_id,json!({"body":if matches!(number,2|3) { "秘密身份" } else { "林夕" }}).to_string()],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, 'draft', ?3)",
            params![project_id.as_str(), object_id, revision_id],
        ).unwrap();
        let named = matches!(number, 1..=3 | 131);
        let entity_id = match number {
            1 => "linxi",
            2 => "secret-a",
            3 => "secret-b",
            _ => "future",
        };
        let name = if matches!(number, 2 | 3) {
            "秘密身份"
        } else {
            "林夕"
        };
        let evidence = json!({"quote":name,"start":0,"end":name.chars().count()});
        let assertion = json!({"kind":"assertion","subjectId":entity_id,"predicate":"appeared","value":true,
            "evidence":evidence});
        repository.connection.execute(
            "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', 'ready', ?5, ?6, 1, 2)",
            params![batch_id, project_id.as_str(), object_id, revision_id,
                if named { json!([assertion]).to_string() } else { "[]".to_owned() },
                if named { json!([{"id":entity_id,"kind":"character","displayName":name,"aliases":[],"evidence":evidence}]).to_string() } else { "[]".to_owned() }],
        ).unwrap();
        repository.connection.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3)",
            params![project_id.as_str(), object_id, batch_id],
        ).unwrap();
        if named {
            repository.connection.execute(
                "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, ?2, ?3, 'character', ?4, '[]', ?5)",
                params![project_id.as_str(), batch_id, entity_id, name, evidence.to_string()],
            ).unwrap();
            repository.connection.execute(
                "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, ?2, ?3, ?4)",
                params![project_id.as_str(), batch_id, entity_id, name],
            ).unwrap();
            repository.connection.execute(
                "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, subject_id, assertion_json) VALUES (?1, ?2, 0, ?3, ?4)",
                params![project_id.as_str(), batch_id, entity_id, assertion.to_string()],
            ).unwrap();
        }
    }
    let found = repository
        .query_writer_knowledge_by_name(
            &project_id,
            super::WriterKnowledgeScope::PriorChapters(131),
            "林夕",
            None,
            10,
        )
        .unwrap();
    assert_eq!(found["selectedEntityId"], "linxi");
    assert_eq!(found["results"].as_array().unwrap().len(), 1);
    assert_eq!(found["results"][0]["sourceObjectId"], "draft-c1");
    assert_eq!(found["coverage"]["totalSources"], 130);
    assert_eq!(found["coverage"]["modeledSources"], 130);
    assert_eq!(found["coverage"]["complete"], true);
    assert!(
        repository
            .audit_assertion_index(&project_id)
            .unwrap()
            .complete()
    );
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('revised-1', ?1, 'draft-c1', 'draft', 'new-digest', '{}')",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_object_heads_v2 SET candidate_revision_id = 'revised-1' WHERE project_id = ?1 AND object_id = 'draft-c1'",
        [project_id.as_str()],
    ).unwrap();
    let stale = repository
        .query_writer_knowledge_by_name(
            &project_id,
            super::WriterKnowledgeScope::PriorChapters(131),
            "林夕",
            None,
            10,
        )
        .unwrap();
    assert_eq!(stale["candidates"], json!([]));
    assert_eq!(stale["coverage"]["modeledSources"], 129);
    assert_eq!(stale["coverage"]["complete"], false);
    assert_eq!(stale["coverage"]["pendingSources"], json!(["draft-c1"]));
    assert_eq!(stale["results"], json!([]));
    let scope = super::WriterKnowledgeScope::CharacterPov {
        chapter: 131,
        character_id: "linxi",
        at_story_time: Some(10),
    };
    let hidden = repository
        .query_writer_knowledge_by_name(&project_id, scope, "秘密身份", None, 10)
        .unwrap();
    assert_eq!(hidden["candidates"], json!([]));
    assert_eq!(hidden["results"], json!([]));
    assert_eq!(hidden["coverage"]["requiresDisambiguation"], false);
    let selected_hidden = repository
        .query_writer_knowledge_by_name(&project_id, scope, "秘密身份", Some("secret-a"), 10)
        .unwrap();
    let selected_missing = repository
        .query_writer_knowledge_by_name(&project_id, scope, "秘密身份", Some("not-a-candidate"), 10)
        .unwrap();
    assert_eq!(selected_hidden, selected_missing);
    assert!(
        repository
            .query_writer_knowledge_by_name(
                &project_id,
                super::WriterKnowledgeScope::PriorChapters(131),
                "秘密身份",
                Some("not-a-candidate"),
                10,
            )
            .is_err()
    );
}
