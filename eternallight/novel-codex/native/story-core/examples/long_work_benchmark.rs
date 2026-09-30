//! Deterministic scale fixture built through the production repository boundary.
//! Literal gold anchors measure retrieval correctness, not semantic or literary quality.
use std::time::Instant;

use novel_codex_story_core::*;
use rusqlite::Connection;
use serde_json::{Value, json};
#[path = "support/long_work_fixture.rs"]
mod support;
use support::marker;

const CHAPTERS: usize = 512;
const SAMPLES: usize = 30;

fn main() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("长篇基准.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let build_started = Instant::now();
    let (project, snapshot) = support::build(&mut repository, CHAPTERS);
    let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    let connection = Connection::open(&path).unwrap();
    let (scenes, claims): (i64, i64) = connection.query_row(
        "SELECT (SELECT SUM(json_array_length(content_json, '$.sceneGroups')) FROM story_revisions_v2 WHERE object_kind = 'draft'), \
         (SELECT SUM(json_array_length(assertions_json)) FROM story_modeling_batches_v2 WHERE status = 'ready')",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    let mut statement = connection.prepare("SELECT json_extract(content_json, '$.body') FROM story_revisions_v2 WHERE object_kind = 'draft'").unwrap();
    let han_characters: usize = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| {
            row.unwrap()
                .chars()
                .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
                .count()
        })
        .sum();
    assert!(han_characters >= 1_000_000 && scenes >= 1000 && claims >= 50_000);
    let mut recalled = 0;
    let mut ordinal = 0;
    let literal = measure(|| {
        let chapter = 1 + ordinal * (CHAPTERS - 1) / SAMPLES;
        ordinal += 1;
        let query = marker(chapter);
        let result = repository
            .search_documents(SearchQuery {
                project_id: &project,
                snapshot_id: None,
                text: &query,
                kind: Some("draft"),
                scope: SearchScope::All,
                limit: 20,
                cursor: None,
            })
            .unwrap();
        let expected = format!("draft-chapter-{chapter}");
        let hits = result["results"].as_array().unwrap();
        if hits.iter().any(|hit| {
            hit["id"] == expected && hit["evidence"]["quote"].as_str().unwrap().contains(&query)
        }) {
            recalled += 1;
        }
        for hit in hits {
            let revision = RevisionId::parse(hit["sourceRevisionId"].as_str().unwrap()).unwrap();
            let content: Value =
                serde_json::from_str(&repository.revision_content(&revision).unwrap().unwrap())
                    .unwrap();
            let field = content
                .pointer(hit["sourcePath"].as_str().unwrap())
                .unwrap()
                .as_str()
                .unwrap();
            let start = hit["evidence"]["start"].as_u64().unwrap() as usize;
            let end = hit["evidence"]["end"].as_u64().unwrap() as usize;
            assert_eq!(
                field
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>(),
                hit["evidence"]["quote"].as_str().unwrap()
            );
        }
    });
    assert_eq!(
        recalled,
        SAMPLES + 1,
        "every synthetic gold anchor must be recalled"
    );
    let current = measure(|| {
        let result = repository
            .knowledge_state(KnowledgeStateQuery {
                project_id: &project,
                snapshot_id: None,
                perspective: KnowledgePerspective::Character,
                world_time: None,
                epistemic_time: Some(CHAPTERS as i64),
                reading_chapter: None,
                owner_id: Some("person-0"),
                entity_id: None,
                predicate: None,
            })
            .unwrap();
        assert_eq!(result["entries"].as_array().unwrap().len(), 200);
        assert_eq!(result["coverage"]["truncatedSources"], true);
    });
    let historical = measure(|| {
        let result = repository
            .knowledge_state(KnowledgeStateQuery {
                project_id: &project,
                snapshot_id: Some(&snapshot),
                perspective: KnowledgePerspective::Character,
                world_time: None,
                epistemic_time: Some(CHAPTERS as i64),
                reading_chapter: Some(CHAPTERS as i64),
                owner_id: Some("person-0"),
                entity_id: None,
                predicate: None,
            })
            .unwrap();
        assert_eq!(result["entries"].as_array().unwrap().len(), 200);
        assert_eq!(result["coverage"]["truncatedSources"], true);
    });
    let graph = measure(|| {
        let result = repository
            .query_relation_graph(RelationGraphQuery {
                project_id: &project,
                scope: RelationGraphScope::Current,
                entity_id: "person-0",
                relation_type: Some("locatedAt"),
                direction: RelationDirection::Outgoing,
                world_time: Some(CHAPTERS as i64),
                max_hops: 1,
                limit: 20,
            })
            .unwrap();
        assert_eq!(result["results"].as_array().unwrap().len(), 20);
    });
    let pov_graph = measure(|| {
        let result = repository
            .query_relation_graph(RelationGraphQuery {
                project_id: &project,
                scope: RelationGraphScope::WriterPov {
                    chapter: CHAPTERS as i64 + 1,
                    character_id: "person-0",
                },
                entity_id: "person-0",
                relation_type: Some("locatedAt"),
                direction: RelationDirection::Outgoing,
                world_time: Some(CHAPTERS as i64),
                max_hops: 1,
                limit: 20,
            })
            .unwrap();
        assert_eq!(result["results"].as_array().unwrap().len(), 20);
        assert_eq!(result["coverage"]["complete"], false);
    });
    let baseline = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: None,
            perspective: KnowledgePerspective::Character,
            world_time: None,
            epistemic_time: Some(CHAPTERS as i64),
            reading_chapter: None,
            owner_id: Some("person-0"),
            entity_id: None,
            predicate: None,
        })
        .unwrap();
    let baseline_graph = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project,
            scope: RelationGraphScope::Current,
            entity_id: "person-0",
            relation_type: Some("locatedAt"),
            direction: RelationDirection::Outgoing,
            world_time: Some(CHAPTERS as i64),
            max_hops: 1,
            limit: 20,
        })
        .unwrap();
    let baseline_search = repository
        .search_documents(SearchQuery {
            project_id: &project,
            snapshot_id: None,
            text: &marker(1),
            kind: Some("draft"),
            scope: SearchScope::All,
            limit: 20,
            cursor: None,
        })
        .unwrap();
    eprintln!("Auditing and rebuilding derived indexes");
    assert!(repository.audit_search_index(&project).unwrap().complete());
    assert!(repository.audit_entity_index(&project).unwrap().complete());
    assert!(
        repository
            .audit_assertion_index(&project)
            .unwrap()
            .complete()
    );
    assert!(
        repository
            .audit_epistemic_index(&project)
            .unwrap()
            .complete()
    );
    assert!(
        repository
            .audit_relation_index(&project)
            .unwrap()
            .complete()
    );
    let index_generation = repository.search_index_generation(&project).unwrap();
    let search_rebuild = maintenance(|| {
        let audit = repository
            .rebuild_search_index(&project, index_generation)
            .unwrap();
        assert!(audit.complete());
        audit
    });
    let entity_rebuild = maintenance(|| {
        let audit = repository.rebuild_entity_index(&project).unwrap();
        assert!(audit.complete());
        audit
    });
    let assertion_rebuild = maintenance(|| {
        let audit = repository.rebuild_assertion_index(&project).unwrap();
        assert!(audit.complete());
        audit
    });
    let epistemic_rebuild = maintenance(|| {
        let audit = repository.rebuild_epistemic_index(&project).unwrap();
        assert!(audit.complete());
        audit
    });
    let relation_rebuild = maintenance(|| {
        let audit = repository.rebuild_relation_index(&project).unwrap();
        assert!(audit.complete());
        audit
    });
    let rebuilt = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: None,
            perspective: KnowledgePerspective::Character,
            world_time: None,
            epistemic_time: Some(CHAPTERS as i64),
            reading_chapter: None,
            owner_id: Some("person-0"),
            entity_id: None,
            predicate: None,
        })
        .unwrap();
    assert_eq!(baseline, rebuilt);
    let rebuilt_graph = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project,
            scope: RelationGraphScope::Current,
            entity_id: "person-0",
            relation_type: Some("locatedAt"),
            direction: RelationDirection::Outgoing,
            world_time: Some(CHAPTERS as i64),
            max_hops: 1,
            limit: 20,
        })
        .unwrap();
    let rebuilt_search = repository
        .search_documents(SearchQuery {
            project_id: &project,
            snapshot_id: None,
            text: &marker(1),
            kind: Some("draft"),
            scope: SearchScope::All,
            limit: 20,
            cursor: None,
        })
        .unwrap();
    assert_eq!(baseline_graph, rebuilt_graph);
    assert_eq!(baseline_search["results"], rebuilt_search["results"]);
    println!(
        "{}",
        json!({"datasetVersion":"long-work-literal-v1","platform":std::env::consts::OS,
        "architecture":std::env::consts::ARCH,"profile":"release","samplesPerQuery":SAMPLES,
        "chapters":CHAPTERS,"scenes":scenes,"hanCharacters":han_characters,"claims":claims,
        "buildMs":build_ms,"literalEvidenceRecallAt20":recalled as f64 / (SAMPLES + 1) as f64,
        "queries":{"literalSearch":literal,"currentCharacter":current,"historicalCharacter":historical,"relationGraph":graph,"writerPovRelations":pov_graph},
        "maintenance":{"search":search_rebuild,"entity":entity_rebuild,"assertion":assertion_rebuild,"epistemic":epistemic_rebuild,"relation":relation_rebuild,"characterProjectionEqual":true,"representativeGraphEqual":true,"representativeSearchEqual":true},
        "limitations":["synthetic literal anchors only; not semantic or influence recall", "bounded projections remain incomplete", "no Context Pack, model latency, Windows or literary-quality gate"]})
    );
}

fn maintenance<T: serde::Serialize>(operation: impl FnOnce() -> T) -> Value {
    let start = Instant::now();
    let report = operation();
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    eprintln!("Rebuild completed in {duration_ms:.1} ms");
    json!({"durationMs":duration_ms,"audit":report})
}

fn measure(mut operation: impl FnMut()) -> Value {
    let first = Instant::now();
    operation();
    let first_ms = first.elapsed().as_secs_f64() * 1000.0;
    let mut samples = (0..SAMPLES)
        .map(|_| {
            let start = Instant::now();
            operation();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect::<Vec<_>>();
    samples.sort_by(f64::total_cmp);
    json!({"firstQueryMs":first_ms,"warmP50Ms":samples[SAMPLES / 2],"warmP95Ms":samples[(SAMPLES * 95).div_ceil(100) - 1],"warmMaxMs":samples[SAMPLES - 1]})
}
