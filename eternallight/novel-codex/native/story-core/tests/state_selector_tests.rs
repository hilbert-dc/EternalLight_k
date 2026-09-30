use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use rusqlite::Connection;
use serde_json::json;

#[path = "../examples/support/long_work_fixture.rs"]
mod fixture;

#[test]
fn event_participants_are_selected_before_long_work_source_limits_in_current_and_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("event-selector.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 231);
    let connection = Connection::open(&path).unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET
        entities_json=json_set(json_insert(entities_json,'$[#]',json_object('id','event-'||object_id,'kind','event','displayName','地点变化','aliases',json('[]'),'evidence',json_extract(assertions_json,'$[0].evidence'))),'$[0].id','person-'||object_id),
        assertions_json=json_set(assertions_json,'$[0]',json_object(
            'kind','event','subjectId','event-'||object_id,'predicate','arrival','storyTime',json_extract(assertions_json,'$[0].validFrom'),
            'value',json_object('participants',json_array(json_object('role','actor','entityId','person-'||object_id),
                json_object('role','location','entityId','port')),
                'preconditions',json('[]'),'effects',json('[]'),'untilExclusive',NULL),
            'evidence',json_extract(assertions_json,'$[0].evidence')),'$[1].epistemicOwnerId','person-'||object_id)",[]).unwrap();
    for historical in [false, true] {
        for chapter in [1, 231] {
            let participant = format!("person-draft-chapter-{chapter}");
            let state = repository
                .knowledge_state(KnowledgeStateQuery {
                    project_id: &project,
                    snapshot_id: if historical { Some(&snapshot) } else { None },
                    perspective: KnowledgePerspective::World,
                    world_time: None,
                    epistemic_time: None,
                    reading_chapter: None,
                    owner_id: None,
                    entity_id: Some(&participant),
                    predicate: Some("arrival"),
                })
                .unwrap();
            assert_eq!(state["entries"].as_array().unwrap().len(), 1, "{state}");
            assert_eq!(state["entries"][0]["sourceChapter"], chapter);
            assert_eq!(state["coverage"]["matchingSourcesCount"], 1);
            assert_eq!(state["coverage"]["complete"], true);
        }
    }
}

#[test]
fn property_world_intervals_are_filtered_before_source_limits_and_bad_intervals_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state-intervals.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 231);
    let connection = Connection::open(&path).unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0]',json_object(
        'kind','state','subjectId',json_extract(assertions_json,'$[0].subjectId'),'predicate','position',
        'value',json_object('entityId','port'),'stateSchema',json_object('cardinality','single','valueType',json_object('type','entity','kind','location')),
        'validFrom',json_extract(assertions_json,'$[0].validFrom'),'validUntilExclusive',json_extract(assertions_json,'$[0].validFrom')+1,
        'evidence',json_extract(assertions_json,'$[0].evidence'),'validUntilEvidence',json_extract(assertions_json,'$[0].evidence')))",[]).unwrap();
    for time in [1, 231] {
        let query = KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: Some(&snapshot),
            perspective: KnowledgePerspective::World,
            world_time: Some(time),
            epistemic_time: None,
            reading_chapter: None,
            owner_id: None,
            entity_id: Some("port"),
            predicate: Some("position"),
        };
        let state = repository.knowledge_state(query).unwrap();
        assert_eq!(state["entries"].as_array().unwrap().len(), 1, "{state}");
        assert_eq!(state["entries"][0]["sourceChapter"], time);
        assert_eq!(state["coverage"]["complete"], true, "{state}");
    }
    connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=json_set(assertions_json,'$[0].validFrom','bad') WHERE object_id='draft-chapter-231'",[]).unwrap();
    let invalid = repository.knowledge_state(KnowledgeStateQuery {
        project_id: &project,
        snapshot_id: Some(&snapshot),
        perspective: KnowledgePerspective::World,
        world_time: Some(0),
        epistemic_time: None,
        reading_chapter: None,
        owner_id: None,
        entity_id: Some("port"),
        predicate: Some("position"),
    });
    assert!(matches!(invalid, Err(RepositoryError::CorruptState)));
}

#[test]
fn state_selectors_reach_both_ends_of_long_work_and_keep_frozen_sources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("全局投影.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 231);
    let focused = |repository: &SqliteStoryRepository,
                   perspective,
                   historical,
                   chapter,
                   entity: &str,
                   predicate: &str| {
        repository
            .knowledge_state(KnowledgeStateQuery {
                project_id: &project,
                snapshot_id: if historical { Some(&snapshot) } else { None },
                perspective,
                world_time: None,
                epistemic_time: None,
                reading_chapter: chapter,
                owner_id: if perspective == KnowledgePerspective::Character {
                    Some("person-0")
                } else {
                    None
                },
                entity_id: Some(entity),
                predicate: Some(predicate),
            })
            .unwrap()
    };
    for historical in [false, true] {
        for (perspective, chapter, entity, expected_chapter) in [
            (KnowledgePerspective::Reader, Some(231), "location-1-0", 1),
            (KnowledgePerspective::Author, None, "location-231-0", 231),
            (KnowledgePerspective::Character, None, "location-231-0", 231),
        ] {
            let state = focused(
                &repository,
                perspective,
                historical,
                chapter,
                entity,
                "knows",
            );
            assert_eq!(state["entries"].as_array().unwrap().len(), 1, "{state}");
            assert_eq!(state["entries"][0]["sourceChapter"], expected_chapter);
            assert_eq!(state["entries"][0]["assertion"]["subjectId"], entity);
            assert_eq!(state["coverage"]["complete"], true, "{}", state["coverage"]);
            let source = state["entries"][0]["assertion"]["evidence"]["quote"]
                .as_str()
                .unwrap();
            assert!(source.contains("知晓"));
        }
        let future = focused(
            &repository,
            KnowledgePerspective::Reader,
            historical,
            Some(230),
            "location-231-0",
            "knows",
        );
        assert_eq!(future["entries"], json!([]));
        let world = repository
            .knowledge_state(KnowledgeStateQuery {
                project_id: &project,
                snapshot_id: if historical { Some(&snapshot) } else { None },
                perspective: KnowledgePerspective::World,
                world_time: Some(1),
                epistemic_time: None,
                reading_chapter: None,
                owner_id: None,
                entity_id: Some("person-0"),
                predicate: Some("locatedAt"),
            })
            .unwrap();
        assert_eq!(world["entries"].as_array().unwrap().len(), 1);
        assert_eq!(world["entries"][0]["sourceChapter"], 1);
        assert_eq!(world["coverage"]["complete"], true);
    }
    let bounded = focused(
        &repository,
        KnowledgePerspective::Reader,
        false,
        Some(231),
        "person-0",
        "knows",
    );
    assert_eq!(bounded["entries"].as_array().unwrap().len(), 200);
    assert_eq!(bounded["coverage"]["matchingSourcesCount"], 231);
    assert_eq!(bounded["coverage"]["truncatedSources"], true);
    assert_eq!(bounded["coverage"]["complete"], false);
    let content =
        json!({"chapterNumber":231,"title":"新结尾","body":"这次林夕并未知晓。"}).to_string();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("draft-chapter-231").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("edited-last").unwrap(),
            commit_id: CommitId::parse("edited-last").unwrap(),
            idempotency_key: IdempotencyKey::parse("edited-last").unwrap(),
            request_digest: "edit".into(),
            content_digest: "edit".into(),
            content_json: content,
            expected_project_revision: 0,
            expected_snapshot_id: snapshot.clone(),
            next_snapshot_id: SnapshotId::parse("edited-last-snapshot").unwrap(),
            expected_candidate_revision_id: Some(RevisionId::parse("revision-231").unwrap()),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-231".into(),
                number: 231,
                title: "新结尾".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    let stale = focused(
        &repository,
        KnowledgePerspective::Author,
        false,
        None,
        "location-231-0",
        "knows",
    );
    assert_eq!(stale["entries"], json!([]));
    assert_eq!(stale["coverage"]["complete"], false);
    let frozen = focused(
        &repository,
        KnowledgePerspective::Author,
        true,
        None,
        "location-231-0",
        "knows",
    );
    assert_eq!(frozen["entries"][0]["sourceRevisionId"], "revision-231");
    let connection = Connection::open(&path).unwrap();
    let original: String = connection.query_row("SELECT assertions_json FROM story_modeling_batches_v2 WHERE object_id = 'draft-chapter-1'", [], |row| row.get(0)).unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json = json_set(assertions_json, '$[1].epistemicTime', 2, '$[1].epistemicUntilExclusive', 1, '$[1].epistemicUntilEvidence', json('{}')) WHERE object_id = 'draft-chapter-1'", []).unwrap();
    let invalid_interval = repository.knowledge_state(KnowledgeStateQuery {
        project_id: &project,
        snapshot_id: None,
        perspective: KnowledgePerspective::Character,
        world_time: None,
        epistemic_time: Some(3),
        reading_chapter: None,
        owner_id: Some("person-0"),
        entity_id: Some("location-1-0"),
        predicate: Some("knows"),
    });
    assert_eq!(invalid_interval, Err(RepositoryError::CorruptState));
    connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json = ?1 WHERE object_id = 'draft-chapter-1'", [original]).unwrap();
    connection.execute("DELETE FROM story_snapshot_knowledge_v2 WHERE snapshot_id = ?1 AND object_id = 'draft-chapter-70'", [snapshot.as_str()]).unwrap();
    let corrupted = repository.knowledge_state(KnowledgeStateQuery {
        project_id: &project,
        snapshot_id: Some(&snapshot),
        perspective: KnowledgePerspective::Reader,
        world_time: None,
        epistemic_time: None,
        reading_chapter: Some(231),
        owner_id: None,
        entity_id: Some("location-1-0"),
        predicate: Some("knows"),
    });
    assert_eq!(corrupted, Err(RepositoryError::CorruptState));
}

#[test]
fn mismatched_ready_head_cannot_reenter_the_projection_as_a_ready_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mismatched-head.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repository, 2);
    let connection = Connection::open(&path).unwrap();
    connection.execute("UPDATE story_knowledge_heads_v2 SET batch_id = (SELECT batch_id FROM story_modeling_batches_v2 WHERE object_id = 'draft-chapter-2') WHERE object_id = 'draft-chapter-1'", []).unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET source_revision_id = 'revision-1' WHERE object_id = 'draft-chapter-2'", []).unwrap();
    let state = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: None,
            perspective: KnowledgePerspective::Author,
            world_time: None,
            epistemic_time: None,
            reading_chapter: None,
            owner_id: None,
            entity_id: Some("location-2-0"),
            predicate: Some("knows"),
        })
        .unwrap();
    assert_eq!(state["entries"], json!([]));
    assert_eq!(state["coverage"]["pendingSourcesCount"], 2);
    assert_eq!(state["coverage"]["complete"], false);
}
