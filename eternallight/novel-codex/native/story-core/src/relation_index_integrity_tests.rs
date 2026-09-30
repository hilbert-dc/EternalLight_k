use rusqlite::params;
use serde_json::json;

use crate::{
    BranchId, ProjectId, ProjectRecord, RelationDirection, RelationGraphQuery, RelationGraphScope,
    RepositoryError, SnapshotId, SqliteStoryRepository,
};

fn seed_project(repository: &mut SqliteStoryRepository, suffix: &str) -> ProjectId {
    let project_id = ProjectId::parse(format!("relation-project-{suffix}")).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Relations".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse(format!("relation-snapshot-{suffix}")).unwrap(),
        )
        .unwrap();
    project_id
}

fn publish_fixture(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    suffix: &str,
    status: &str,
) -> String {
    let batch_id = format!("relation-batch-{suffix}");
    let object_id = format!("draft-{suffix}");
    let revision_id = format!("revision-{suffix}");
    let assertion = json!({
        "kind":"relation", "subjectId":"linxi", "targetId":"port",
        "predicate":"locatedAt", "relationType":"locatedAt", "propositionId":format!("location-{suffix}"), "value":true,
        "validFrom":1, "evidence":{"quote":"林夕在港口", "start":0,"end":5}
    });
    let entities = json!([
        {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],
            "evidence":{"quote":"林夕","start":0,"end":2}},
        {"id":"port","kind":"location","displayName":"港口","aliases":[],
            "evidence":{"quote":"港口","start":3,"end":5}},
        {"id":format!("location-{suffix}"),"kind":"proposition","displayName":"林夕在港口","aliases":[],
            "evidence":{"quote":"林夕在港口","start":0,"end":5}}
    ]);
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, ?3, 'draft', 'digest', ?4)",
        params![revision_id, project_id.as_str(), object_id, json!({"body":"林夕在港口"}).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, ?2, 'draft', ?3)",
        params![project_id.as_str(), object_id, revision_id],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, ?3, ?4, 0, 'test', ?5, ?6, ?7, 1, 2)",
        params![batch_id, project_id.as_str(), object_id, revision_id, status, json!([assertion]).to_string(), entities.to_string()],
    ).unwrap();
    for entity in entities.as_array().unwrap() {
        repository.connection.execute(
            "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, ?2, ?3, ?4, ?5, '[]', ?6)",
            params![project_id.as_str(), batch_id, entity["id"].as_str(), entity["kind"].as_str(),
                entity["displayName"].as_str(), entity["evidence"].to_string()],
        ).unwrap();
    }
    repository.connection.execute(
        "INSERT INTO story_relation_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json) VALUES (?1, ?2, 0, 'linxi', 'port', 'locatedAt', ?3)",
        params![project_id.as_str(), batch_id, assertion.to_string()],
    ).unwrap();
    batch_id
}

#[test]
fn relation_index_audit_and_rebuild_include_stale_history_without_touching_other_projects() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "a");
    let other_id = seed_project(&mut repository, "b");
    let old = publish_fixture(&repository, &project_id, "old", "stale");
    let current = publish_fixture(&repository, &project_id, "current", "ready");
    publish_fixture(&repository, &other_id, "other", "ready");
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, 'draft-current', ?2)",
        params![project_id.as_str(), current],
    ).unwrap();
    assert!(
        repository
            .audit_relation_index(&project_id)
            .unwrap()
            .complete()
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_relation_claims_v1 WHERE batch_id = ?1",
            [&old],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_relation_claims_v1 SET target_id = 'elsewhere' WHERE batch_id = ?1",
            [&current],
        )
        .unwrap();
    repository.connection.execute(
        "INSERT INTO story_relation_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json) SELECT project_id, batch_id, 99, subject_id, target_id, relation_type, assertion_json FROM story_relation_claims_v1 WHERE batch_id = ?1",
        [&current],
    ).unwrap();
    let broken = repository.audit_relation_index(&project_id).unwrap();
    assert_eq!(
        (
            broken.missing_claims,
            broken.changed_claims,
            broken.extra_claims
        ),
        (1, 1, 1)
    );
    assert_eq!(broken.affected_batch_ids, vec![current, old]);
    let other_before = repository.audit_relation_index(&other_id).unwrap();
    let revision_before = repository.project_revision(&project_id).unwrap();
    let repaired = repository.rebuild_relation_index(&project_id).unwrap();
    assert!(repaired.complete());
    assert_eq!(repaired.expected_claims, 2);
    assert_eq!(
        repository.audit_relation_index(&other_id).unwrap(),
        other_before
    );
    assert_eq!(
        repository.project_revision(&project_id).unwrap(),
        revision_before
    );
    let graph = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::Current,
            entity_id: "linxi",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: None,
            max_hops: 1,
            limit: 10,
        })
        .unwrap();
    assert_eq!(graph["results"].as_array().unwrap().len(), 1);
    assert_eq!(graph["results"][0]["toId"], "port");
    assert_eq!(graph["results"][0]["propositionId"], "location-current");
}

#[test]
fn malformed_source_aborts_relation_rebuild_without_erasing_index() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "malformed");
    let batch = publish_fixture(&repository, &project_id, "bad", "ready");
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET assertions_json = '{' WHERE batch_id = ?1",
            [&batch],
        )
        .unwrap();
    assert!(matches!(
        repository.rebuild_relation_index(&project_id),
        Err(RepositoryError::CorruptState)
    ));
    let retained: i64 = repository
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_relation_claims_v1 WHERE batch_id = ?1",
            [&batch],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, 1);
}

#[test]
fn wrong_type_proposition_source_fails_relation_audit_and_rebuild() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "wrong-proposition");
    let batch = publish_fixture(&repository, &project_id, "wrong", "ready");
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json = ?1 WHERE batch_id = ?2",
            params![
                json!([{"id":"location-wrong","kind":"character"}]).to_string(),
                batch
            ],
        )
        .unwrap();
    assert!(matches!(
        repository.audit_relation_index(&project_id),
        Err(RepositoryError::CorruptState)
    ));
    assert!(matches!(
        repository.rebuild_relation_index(&project_id),
        Err(RepositoryError::CorruptState)
    ));
    let job = repository.start_relation_audit(&project_id).unwrap();
    let failed = repository
        .advance_relation_audit(&project_id, &job.id)
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.verified_consistent, None);
    let retained: i64 = repository
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_relation_claims_v1 WHERE batch_id = ?1",
            [&batch],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, 1);
}

#[test]
fn indexed_relation_expansion_reaches_sources_beyond_the_old_hundred_batch_scan() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "long");
    for number in 0..120 {
        let suffix = format!("long-{number:03}");
        let batch_id = publish_fixture(&repository, &project_id, &suffix, "ready");
        repository.connection.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3)",
            params![project_id.as_str(), format!("draft-{suffix}"), batch_id],
        ).unwrap();
    }
    let graph = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::Current,
            entity_id: "linxi",
            relation_type: Some("locatedAt"),
            direction: RelationDirection::Outgoing,
            world_time: None,
            max_hops: 1,
            limit: 100,
        })
        .unwrap();
    assert_eq!(graph["coverage"]["totalSources"], 120);
    assert_eq!(graph["coverage"]["modeledSources"], 120);
    assert_eq!(graph["coverage"]["scannedEdges"], 120);
    assert_eq!(graph["coverage"]["truncatedEdges"], false);
    assert_eq!(graph["coverage"]["truncatedMatches"], true);
    assert_eq!(graph["results"].as_array().unwrap().len(), 100);
}

#[test]
fn writer_relation_scope_excludes_current_future_and_later_identity_merges() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "writer-scope");
    for (number, suffix) in [(1, "prior"), (2, "current"), (3, "future")] {
        repository.connection.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
            params![suffix, project_id.as_str(), number, suffix],
        ).unwrap();
        let batch_id = publish_fixture(&repository, &project_id, suffix, "ready");
        repository.connection.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3)",
            params![project_id.as_str(), format!("draft-{suffix}"), batch_id],
        ).unwrap();
    }
    let future_assertion = json!({
        "kind":"relation", "subjectId":"future", "targetId":"port",
        "predicate":"locatedAt", "relationType":"locatedAt", "propositionId":"location-future", "value":true,
        "validFrom":1, "evidence":{"quote":"未来人物在港口", "start":0,"end":7}
    });
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = ?1 WHERE batch_id = 'relation-batch-future'",
        [json!([future_assertion]).to_string()],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_relation_claims_v1 SET subject_id = 'future', assertion_json = ?1 WHERE batch_id = 'relation-batch-future'",
        [future_assertion.to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, 'relation-batch-future', 'future', 'character', '未来人物', '[]', '{}')",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('identity-later', ?1, 'artifact-identity', 'identity', 'digest', ?2)",
        params![project_id.as_str(), json!({"payload":{"decisions":[{"kind":"same","fromId":"future","toId":"linxi"}]}}).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, accepted_revision_id) VALUES (?1, 'artifact-identity', 'identity', 'identity-later')",
        [project_id.as_str()],
    ).unwrap();

    let writer = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::WriterBeforeChapter(2),
            entity_id: "linxi",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: None,
            max_hops: 1,
            limit: 10,
        })
        .unwrap();
    assert_eq!(writer["coverage"]["totalSources"], 1);
    assert_eq!(writer["coverage"]["modeledSources"], 1);
    assert_eq!(writer["results"].as_array().unwrap().len(), 1);
    assert_eq!(writer["results"][0]["sourceObjectId"], "draft-prior");
    assert_eq!(writer["results"][0]["confirmationState"], "candidate");
    assert_eq!(writer["identityRevisionId"], json!(null));
    let hidden = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::WriterBeforeChapter(2),
            entity_id: "future",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: None,
            max_hops: 1,
            limit: 10,
        })
        .unwrap();
    assert_eq!(hidden["seedFound"], false);
    assert_eq!(hidden["results"], json!([]));
    let author = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::Current,
            entity_id: "linxi",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: None,
            max_hops: 1,
            limit: 10,
        })
        .unwrap();
    assert_eq!(author["results"].as_array().unwrap().len(), 3);
}

#[test]
fn writer_pov_relation_requires_prior_source_bound_known_proposition() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "writer-pov");
    for (number, suffix) in [(1, "one"), (2, "two"), (3, "three")] {
        repository.connection.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
            params![suffix, project_id.as_str(), number, suffix],
        ).unwrap();
        let batch_id = publish_fixture(&repository, &project_id, suffix, "ready");
        repository.connection.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3)",
            params![project_id.as_str(), format!("draft-{suffix}"), batch_id],
        ).unwrap();
    }
    repository.connection.execute(
        "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, 'relation-batch-one', 'linxi', '林夕')",
        [project_id.as_str()],
    ).unwrap();
    let relation_payload: String = repository.connection.query_row(
        "SELECT json_extract(assertions_json, '$[0]') FROM story_modeling_batches_v2 WHERE batch_id = 'relation-batch-one'",
        [],
        |row| row.get(0),
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, assertion_json) VALUES (?1, 'relation-batch-one', 0, 'linxi', 'port', ?2)",
        params![project_id.as_str(), relation_payload],
    ).unwrap();
    let name_search = |repository: &SqliteStoryRepository, at_story_time| {
        repository
            .query_writer_knowledge_by_name(
                &project_id,
                crate::WriterKnowledgeScope::CharacterPov {
                    chapter: 3,
                    character_id: "linxi",
                    at_story_time,
                },
                "林夕",
                None,
                10,
            )
            .unwrap()
    };
    let hidden_name = name_search(&repository, Some(6));
    assert_eq!(hidden_name["results"], json!([]));
    assert_eq!(hidden_name["candidates"], json!([]));
    assert_eq!(hidden_name["selectedEntityId"], json!(null));
    assert_eq!(hidden_name["coverage"]["povEvidenceUnverified"], true);
    let query = |repository: &SqliteStoryRepository, at| {
        repository
            .query_relation_graph(RelationGraphQuery {
                project_id: &project_id,
                scope: RelationGraphScope::WriterPov {
                    chapter: 3,
                    character_id: "linxi",
                },
                entity_id: "linxi",
                relation_type: None,
                direction: RelationDirection::Outgoing,
                world_time: at,
                max_hops: 1,
                limit: 10,
            })
            .unwrap()
    };
    let unknown = query(&repository, Some(6));
    assert_eq!(unknown["results"], json!([]));
    assert_eq!(unknown["coverage"]["complete"], false);
    assert_eq!(unknown["seedFound"], json!(null));

    let claim = json!({"kind":"epistemic","subjectId":"location-one", "propositionId":"location-one",
        "predicate":"knows", "epistemicOwnerId":"linxi", "epistemicState":"known",
        "epistemicTime":5, "value":true, "evidence":{"quote":"林夕知道","start":0,"end":4}});
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = json_insert(assertions_json, '$[#]', json(?1)) WHERE batch_id = 'relation-batch-one'",
        [claim.to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json) VALUES (?1, 'relation-batch-one', 1, 'linxi', 'location-one', 'relation-batch-one', 'known', 5, ?2)",
        params![project_id.as_str(), claim.to_string()],
    ).unwrap();
    let visible = query(&repository, Some(6));
    assert_eq!(visible["results"].as_array().unwrap().len(), 1);
    assert_eq!(visible["results"][0]["propositionId"], "location-one");
    assert_eq!(visible["coverage"]["complete"], false);
    let unknown_time = query(&repository, None);
    assert_eq!(unknown_time["results"], json!([]));
    assert_eq!(unknown_time["coverage"]["complete"], false);
    let visible_name = name_search(&repository, Some(6));
    assert_eq!(visible_name["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        visible_name["results"][0]["assertion"]["propositionId"],
        "location-one"
    );
    assert_eq!(visible_name["candidates"], json!([]));
    assert_eq!(visible_name["coverage"]["complete"], false);
    assert_eq!(name_search(&repository, Some(4))["results"], json!([]));
    assert_eq!(name_search(&repository, None)["results"], json!([]));
    assert_eq!(query(&repository, Some(4))["results"], json!([]));
    let former_suspicion = json!({"kind":"epistemic","subjectId":"location-one",
        "propositionId":"location-one","predicate":"suspects","epistemicOwnerId":"linxi",
        "epistemicState":"suspected","epistemicTime":1,"epistemicUntilExclusive":5,
        "epistemicUntilEvidence":{"quote":"林夕知道","start":0,"end":4},
        "value":true,"evidence":{"quote":"林夕怀疑","start":0,"end":4}});
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = json_insert(assertions_json, '$[#]', json(?1)) WHERE batch_id = 'relation-batch-one'",
        [former_suspicion.to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json) VALUES (?1, 'relation-batch-one', 2, 'linxi', 'location-one', 'relation-batch-one', 'suspected', 1, ?2)",
        params![project_id.as_str(), former_suspicion.to_string()],
    ).unwrap();
    assert_eq!(
        query(&repository, Some(5))["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    repository.connection.execute(
        "UPDATE story_epistemic_claims_v1 SET assertion_json = json_set(assertion_json, '$.epistemicUntilExclusive', 7) WHERE batch_id = 'relation-batch-one' AND assertion_index = 2",
        [],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = json_set(assertions_json, '$[2].epistemicUntilExclusive', 7) WHERE batch_id = 'relation-batch-one'",
        [],
    ).unwrap();
    assert_eq!(query(&repository, Some(6))["results"], json!([]));
    assert_eq!(
        query(&repository, Some(7))["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    repository.connection.execute(
        "UPDATE story_epistemic_claims_v1 SET epistemic_state = 'suspected', assertion_json = json_set(assertion_json, '$.epistemicState', 'suspected') WHERE batch_id = 'relation-batch-one'",
        [],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = json_set(assertions_json, '$[1].epistemicState', 'suspected') WHERE batch_id = 'relation-batch-one'",
        [],
    ).unwrap();
    assert_eq!(query(&repository, Some(6))["results"], json!([]));
    assert_eq!(name_search(&repository, Some(6))["results"], json!([]));
    repository.connection.execute(
        "UPDATE story_epistemic_claims_v1 SET epistemic_state = 'known', assertion_json = json_set(assertion_json, '$.epistemicState', 'known') WHERE batch_id = 'relation-batch-one'",
        [],
    ).unwrap();
    assert!(matches!(
        repository.query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::WriterPov {
                chapter: 3,
                character_id: "linxi"
            },
            entity_id: "linxi",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: Some(6),
            max_hops: 1,
            limit: 10,
        }),
        Err(RepositoryError::CorruptState)
    ));
}

#[test]
fn writer_pov_does_not_join_knowledge_from_another_batch_with_the_same_local_proposition_id() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = seed_project(&mut repository, "pov-local-proposition");
    for (number, suffix) in [(1, "one"), (2, "two")] {
        repository.connection.execute(
            "INSERT INTO story_chapters_v2(chapter_id, project_id, number, title, status) VALUES (?1, ?2, ?3, ?4, 'draft')",
            params![suffix, project_id.as_str(), number, suffix],
        ).unwrap();
        let batch_id = publish_fixture(&repository, &project_id, suffix, "ready");
        repository.connection.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, ?2, ?3)",
            params![project_id.as_str(), format!("draft-{suffix}"), batch_id],
        ).unwrap();
    }
    repository.connection.execute(
        "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, 'relation-batch-one', 'linxi', '林夕')",
        [project_id.as_str()],
    ).unwrap();
    let relation_payload: String = repository.connection.query_row(
        "SELECT json_extract(assertions_json, '$[0]') FROM story_modeling_batches_v2 WHERE batch_id = 'relation-batch-one'",
        [],
        |row| row.get(0),
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_assertion_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, assertion_json) VALUES (?1, 'relation-batch-one', 0, 'linxi', 'port', ?2)",
        params![project_id.as_str(), relation_payload],
    ).unwrap();
    let borrowed_id = json!({"id":"location-one","kind":"proposition", "displayName":"另一命题", "aliases":[],
        "evidence":{"quote":"林夕在港口","start":0,"end":5}});
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET entities_json = json_insert(entities_json, '$[#]', json(?1)) WHERE batch_id = 'relation-batch-two'",
        [borrowed_id.to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, 'relation-batch-two', 'location-one', 'proposition', '另一命题', '[]', ?2)",
        params![project_id.as_str(), borrowed_id["evidence"].to_string()],
    ).unwrap();
    let claim = json!({"kind":"epistemic","subjectId":"location-one", "propositionId":"location-one",
        "predicate":"knows", "epistemicOwnerId":"linxi", "epistemicState":"known", "epistemicTime":5,
        "value":true, "evidence":{"quote":"林夕在港口","start":0,"end":5}});
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET assertions_json = json_insert(assertions_json, '$[#]', json(?1)) WHERE batch_id = 'relation-batch-two'",
        [claim.to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_epistemic_claims_v1(project_id, batch_id, assertion_index, owner_id, proposition_id, proposition_source_batch_id, epistemic_state, epistemic_time, assertion_json) VALUES (?1, 'relation-batch-two', 1, 'linxi', 'location-one', 'relation-batch-two', 'known', 5, ?2)",
        params![project_id.as_str(), claim.to_string()],
    ).unwrap();

    let graph = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::WriterPov {
                chapter: 3,
                character_id: "linxi",
            },
            entity_id: "linxi",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: Some(6),
            max_hops: 1,
            limit: 10,
        })
        .unwrap();
    assert_eq!(graph["results"], json!([]));
    assert_eq!(graph["coverage"]["complete"], false);
    let by_name = repository
        .query_writer_knowledge_by_name(
            &project_id,
            crate::WriterKnowledgeScope::CharacterPov {
                chapter: 3,
                character_id: "linxi",
                at_story_time: Some(6),
            },
            "林夕",
            None,
            10,
        )
        .unwrap();
    assert!(
        by_name["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| result["assertion"]["kind"] != "relation")
    );
    repository.connection.execute(
        "DELETE FROM story_entity_claims_v2 WHERE batch_id = 'relation-batch-two' AND entity_id = 'location-one'",
        [],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_modeling_batches_v2 SET entities_json = json_remove(entities_json, '$[3]'), assertions_json = json_set(assertions_json, '$[1].propositionSourceBatchId', 'relation-batch-one') WHERE batch_id = 'relation-batch-two'",
        [],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_epistemic_claims_v1 SET proposition_source_batch_id = 'relation-batch-one', assertion_json = json_set(assertion_json, '$.propositionSourceBatchId', 'relation-batch-one') WHERE batch_id = 'relation-batch-two'",
        [],
    ).unwrap();
    assert!(
        repository
            .audit_epistemic_index(&project_id)
            .unwrap()
            .complete()
    );
    let linked = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project_id,
            scope: RelationGraphScope::WriterPov {
                chapter: 3,
                character_id: "linxi",
            },
            entity_id: "linxi",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: Some(6),
            max_hops: 1,
            limit: 10,
        })
        .unwrap();
    assert_eq!(linked["results"].as_array().unwrap().len(), 1);
    assert_eq!(linked["results"][0]["propositionId"], "location-one");
}
