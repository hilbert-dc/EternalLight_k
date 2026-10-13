use super::*;

#[tokio::test]
async fn later_chapter_can_publish_source_bound_knowledge_of_an_earlier_proposition() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cross-chapter-knowledge.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"跨章节认知"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = created["project"]["id"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let batch_uri = format!("/api/projects/{project_id}/modeling-batches");
    let task_uri = format!("/api/projects/{project_id}/tasks");
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let first_capability = "cross-chapter-first-secret-123456789012345";
    let second_capability = "cross-chapter-second-secret-12345678901234";

    let (status, first_draft) = call(application(&path), "POST", &draft_uri, Some(json!({
        "payload":{"chapterNumber":1,"title":"港口","body":"林夕在港口。"},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"cross-chapter-first"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{first_draft}");
    let (status, first_batch) = call(application(&path), "POST", &batch_uri, Some(json!({
        "objectId":format!("draft-{}",first_draft["chapterId"].as_str().unwrap()),
        "sourceRevisionId":first_draft["revisionId"],"expectedRevision":0,"extractorVersion":"test-v1"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{first_batch}");
    let first_batch_id = first_batch["id"].as_str().unwrap();
    let (status, first_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation":"modelKnowledge","inputRevision":0,"threadId":"cross-chapter-first-thread",
            "target":{"batchId":first_batch_id},
            "capabilityHash":format!("{:x}",Sha256::digest(first_capability.as_bytes())),
            "capabilityExpiresAt":expires_at,"authorIntent":"建模第一章"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first_task}");
    let (status, first_published) = call_as(application(&path), "POST",
        &format!("{batch_uri}/{first_batch_id}/publish"), Some(json!({
            "taskId":first_task["id"],"threadId":"cross-chapter-first-thread","expectedRevision":0,
            "entities":[
                {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":{"quote":"林夕","start":0,"end":2}},
                {"id":"port","kind":"location","displayName":"港口","aliases":[],"evidence":{"quote":"港口","start":3,"end":5}},
                {"id":"linxi-at-port","kind":"proposition","displayName":"林夕在港口","aliases":[],"evidence":{"quote":"林夕在港口","start":0,"end":5}}
            ],
            "assertions":[{"kind":"relation","subjectId":"linxi","targetId":"port",
                "predicate":"locatedAt","relationType":"locatedAt","propositionId":"linxi-at-port",
                "value":true,"validFrom":1,"evidence":{"quote":"林夕在港口","start":0,"end":5}}]
        })), CANDIDATE_TOKEN, Some(first_capability)).await;
    assert_eq!(status, StatusCode::OK, "{first_published}");

    let (status, second_draft) = call(application(&path), "POST", &draft_uri, Some(json!({
        "payload":{"chapterNumber":2,"title":"得知","body":"林夕终于知道自己在港口。随后忘了这件事。"},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"cross-chapter-second"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{second_draft}");
    let (status, second_batch) = call(application(&path), "POST", &batch_uri, Some(json!({
        "objectId":format!("draft-{}",second_draft["chapterId"].as_str().unwrap()),
        "sourceRevisionId":second_draft["revisionId"],"expectedRevision":0,"extractorVersion":"test-v1"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{second_batch}");
    let second_batch_id = second_batch["id"].as_str().unwrap();
    let (status, second_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation":"modelKnowledge","inputRevision":0,"threadId":"cross-chapter-second-thread",
            "target":{"batchId":second_batch_id},
            "capabilityHash":format!("{:x}",Sha256::digest(second_capability.as_bytes())),
            "capabilityExpiresAt":expires_at,"authorIntent":"建模第二章"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second_task}");
    let entities = json!([
        {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],
            "evidence":{"quote":"林夕","start":0,"end":2}},
        {"id":"linxi-knows-port","kind":"proposition","displayName":"林夕终于知道自己在港口","aliases":[],
            "evidence":{"quote":"林夕终于知道自己在港口","start":0,"end":11}}
    ]);
    let mut claim = json!({"kind":"epistemic","subjectId":"linxi-at-port",
        "predicate":"knows","propositionId":"linxi-at-port","epistemicOwnerId":"linxi",
        "epistemicState":"known","epistemicTime":5,"value":true,
        "evidence":{"quote":"知道自己在港口","start":4,"end":11}});
    let second_publish_uri = format!("{batch_uri}/{second_batch_id}/publish");
    claim["propositionSourceBatchId"] = json!("nonexistent-batch");
    let (status, invalid) = call_as(
        application(&path),
        "POST",
        &second_publish_uri,
        Some(
            json!({"taskId":second_task["id"],"threadId":"cross-chapter-second-thread",
            "expectedRevision":0,"entities":entities,"assertions":[claim]}),
        ),
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
    claim["propositionSourceBatchId"] = json!(first_batch_id);
    claim["epistemicUntilExclusive"] = json!(8);
    let (status, missing_end_evidence) = call_as(
        application(&path),
        "POST",
        &second_publish_uri,
        Some(
            json!({"taskId":second_task["id"],"threadId":"cross-chapter-second-thread",
            "expectedRevision":0,"entities":entities,"assertions":[claim]}),
        ),
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{missing_end_evidence}");
    claim["epistemicUntilEvidence"] = json!({"quote":"随后忘了这件事","start":12,"end":19});
    for (end, quote) in [(5, "随后忘了这件事"), (8, "并没有发生遗忘")] {
        let mut invalid_interval = claim.clone();
        invalid_interval["epistemicUntilExclusive"] = json!(end);
        invalid_interval["epistemicUntilEvidence"]["quote"] = json!(quote);
        let (status, rejected) = call_as(
            application(&path),
            "POST",
            &second_publish_uri,
            Some(
                json!({"taskId":second_task["id"],"threadId":"cross-chapter-second-thread",
                "expectedRevision":0,"entities":entities,"assertions":[invalid_interval]}),
            ),
            CANDIDATE_TOKEN,
            Some(second_capability),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    }
    // Keep the derived proposition index intact while damaging authoritative source data.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let original_entities: String = connection
        .query_row(
            "SELECT entities_json FROM story_modeling_batches_v2 WHERE batch_id=?1",
            [first_batch_id],
            |row| row.get(0),
        )
        .unwrap();
    connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=json_remove(entities_json,'$[2]') WHERE batch_id=?1",[first_batch_id]).unwrap();
    let (status, damaged_source) = call_as(application(&path),"POST",&second_publish_uri,
        Some(json!({"taskId":second_task["id"],"threadId":"cross-chapter-second-thread","expectedRevision":0,"entities":entities,"assertions":[claim]})),
        CANDIDATE_TOKEN,Some(second_capability)).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "{damaged_source}"
    );
    let retained: (String,String,String) = connection.query_row(
        "SELECT status,entities_json,assertions_json FROM story_modeling_batches_v2 WHERE batch_id=?1",[second_batch_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(retained, ("queued".into(), "[]".into(), "[]".into()));
    connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id=?2",
            rusqlite::params![original_entities, first_batch_id],
        )
        .unwrap();
    // A missing derived index must not remove a valid proposition from source authority.
    connection
        .execute(
            "DELETE FROM story_entity_claims_v2 WHERE batch_id=?1 AND entity_id='linxi-at-port'",
            [first_batch_id],
        )
        .unwrap();
    let (status, second_published) = call_as(
        application(&path),
        "POST",
        &second_publish_uri,
        Some(
            json!({"taskId":second_task["id"],"threadId":"cross-chapter-second-thread",
            "expectedRevision":0,"entities":entities,"assertions":[claim]}),
        ),
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second_published}");
    // Restore only the fault-injected fixture row before exercising unrelated read paths.
    connection.execute("INSERT INTO story_entity_claims_v2(project_id,batch_id,entity_id,kind,display_name,aliases_json,evidence_json) VALUES(?1,?2,'linxi-at-port','proposition','林夕在港口','[]',?3)",
        rusqlite::params![project_id,first_batch_id,json!({"quote":"林夕在港口","start":0,"end":5}).to_string()]).unwrap();

    let writer_capability = "cross-chapter-writer-secret-123456789012345";
    let writer_thread = "cross-chapter-writer-thread";
    let (status, writer_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation":"generateChapter","inputRevision":0,"threadId":writer_thread,
            "target":{"type":"chapter","chapterId":second_draft["chapterId"],
                "chapterNumber":2,"readingChapter":2,"sceneOrder":1,
                "storyTime":6,"povCharacterId":"linxi"},
            "capabilityHash":format!("{:x}",Sha256::digest(writer_capability.as_bytes())),
            "capabilityExpiresAt":expires_at,"authorIntent":"从林夕视角写第二章"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{writer_task}");
    let (status, writer_context) = call_as(
        application(&path),
        "GET",
        &format!(
            "/api/projects/{project_id}/context?taskId={}&threadId={writer_thread}&projectRevision=0&purpose=writer&readingChapter=2&sceneOrder=1&storyTime=6&povCharacterId=linxi&budget=24&byteBudget=60000",
            writer_task["id"].as_str().unwrap()
        ),
        None,
        CANDIDATE_TOKEN,
        Some(writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{writer_context}");
    let writer_knowledge = read_frozen_context_source(
        &path, project_id, &writer_task, writer_capability, &writer_context, "/operationContext/workingKnowledge",
    ).await;
    assert_eq!(
        writer_knowledge["perspective"],
        "character"
    );
    assert_eq!(
        writer_knowledge["entries"],
        json!([]),
        "第二章尚未发生的认知不得进入第二章 Writer POV"
    );

    let (status, third_draft) = call(application(&path), "POST", &draft_uri, Some(json!({
        "payload":{"chapterNumber":3,"title":"传闻","body":"张衡得知林夕已经知道。"},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"cross-chapter-third"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{third_draft}");
    let (status, third_batch) = call(application(&path), "POST", &batch_uri, Some(json!({
        "objectId":format!("draft-{}",third_draft["chapterId"].as_str().unwrap()),
        "sourceRevisionId":third_draft["revisionId"],"expectedRevision":0,"extractorVersion":"test-v1"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{third_batch}");
    let third_batch_id = third_batch["id"].as_str().unwrap();
    let third_capability = "cross-chapter-third-secret-123456789012345";
    let (status, third_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation":"modelKnowledge","inputRevision":0,"threadId":"cross-chapter-third-thread",
            "target":{"batchId":third_batch_id},
            "capabilityHash":format!("{:x}",Sha256::digest(third_capability.as_bytes())),
            "capabilityExpiresAt":expires_at,"authorIntent":"建模第三章"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{third_task}");
    let (status, third_published) = call_as(
        application(&path),
        "POST",
        &format!("{batch_uri}/{third_batch_id}/publish"),
        Some(json!({
            "taskId":third_task["id"],"threadId":"cross-chapter-third-thread","expectedRevision":0,
            "entities":[{"id":"zhangheng","kind":"character","displayName":"张衡","aliases":[],
                "evidence":{"quote":"张衡","start":0,"end":2}}],
            "assertions":[{"kind":"epistemic","subjectId":"linxi-knows-port","predicate":"knows",
                "propositionId":"linxi-knows-port","propositionSourceBatchId":second_batch_id,
                "epistemicOwnerId":"zhangheng","epistemicState":"known","epistemicTime":7,
                "value":true,"evidence":{"quote":"张衡得知林夕已经知道","start":0,"end":10}}]
        })),
        CANDIDATE_TOKEN,
        Some(third_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{third_published}");

    let project = ProjectId::parse(project_id.to_owned()).unwrap();
    let repository = SqliteStoryRepository::open(&path).unwrap();
    let historical_snapshot = repository
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap()
        .snapshot_id;
    assert!(
        repository
            .audit_epistemic_index(&project)
            .unwrap()
            .complete()
    );
    let graph_at = |chapter, world_time| {
        repository
            .query_relation_graph(RelationGraphQuery {
                project_id: &project,
                scope: RelationGraphScope::WriterPov {
                    chapter,
                    character_id: "linxi",
                },
                entity_id: "linxi",
                relation_type: None,
                direction: RelationDirection::Outgoing,
                world_time: Some(world_time),
                max_hops: 1,
                limit: 10,
            })
            .unwrap()
    };
    assert_eq!(graph_at(2, 6)["results"], json!([]));
    assert_eq!(graph_at(3, 4)["results"], json!([]));
    assert_eq!(graph_at(3, 6)["results"].as_array().unwrap().len(), 1);
    assert_eq!(graph_at(3, 8)["results"], json!([]));
    let forgotten = repository
        .knowledge_state(KnowledgeStateQuery {
            project_id: &project,
            snapshot_id: None,
            perspective: KnowledgePerspective::Character,
            world_time: None,
            epistemic_time: Some(8),
            reading_chapter: None,
            owner_id: Some("linxi"),
            entity_id: None,
            predicate: None,
        })
        .unwrap();
    assert_eq!(forgotten["entries"], json!([]));
    let character_knowledge = || {
        repository
            .knowledge_state(KnowledgeStateQuery {
                project_id: &project,
                snapshot_id: None,
                perspective: KnowledgePerspective::Character,
                world_time: None,
                epistemic_time: Some(6),
                reading_chapter: None,
                owner_id: Some("linxi"),
                entity_id: None,
                predicate: None,
            })
            .unwrap()
    };
    let searched_knowledge = || {
        repository
            .query_writer_knowledge_by_name(
                &project,
                WriterKnowledgeScope::PriorChapters(3),
                "林夕",
                None,
                10,
            )
            .unwrap()
    };
    assert_eq!(
        character_knowledge()["entries"].as_array().unwrap().len(),
        1
    );
    assert_eq!(searched_knowledge()["results"].as_array().unwrap().len(), 2);
    let (status, revised) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload":{"chapterId":first_draft["chapterId"],"chapterNumber":1,
                "title":"港口","body":"林夕离开港口。"},
            "expectedRevision":0,"expectedCandidateRevisionId":first_draft["revisionId"],
            "idempotencyKey":"cross-chapter-first-revised"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    assert_eq!(graph_at(3, 6)["results"], json!([]));
    assert_eq!(
        repository
            .modeling_batch(&project, second_batch_id)
            .unwrap()
            .status,
        "stale"
    );
    assert_eq!(
        repository
            .modeling_batch(&project, third_batch_id)
            .unwrap()
            .status,
        "stale"
    );
    for (at, expected_entries) in [(6, 1), (8, 0)] {
        assert_eq!(
            repository
                .knowledge_state(KnowledgeStateQuery {
                    project_id: &project,
                    snapshot_id: Some(&historical_snapshot),
                    perspective: KnowledgePerspective::Character,
                    world_time: None,
                    epistemic_time: Some(at),
                    reading_chapter: None,
                    owner_id: Some("linxi"),
                    entity_id: None,
                    predicate: None,
                })
                .unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            expected_entries
        );
    }
    assert_eq!(
        repository
            .chapter_knowledge(&project, second_draft["chapterId"].as_str().unwrap())
            .unwrap()["status"],
        "model_pending"
    );
    let knowledge = character_knowledge();
    assert_eq!(knowledge["entries"], json!([]));
    assert_eq!(knowledge["coverage"]["complete"], false);
    assert!(
        knowledge["coverage"]["pendingSources"]
            .as_array()
            .unwrap()
            .contains(&json!(format!(
                "draft-{}",
                second_draft["chapterId"].as_str().unwrap()
            )))
    );
    let search = searched_knowledge();
    assert_eq!(search["results"], json!([]));
    assert_eq!(search["coverage"]["complete"], false);
}

#[tokio::test]
async fn confirmed_fact_is_visible_in_context_and_undo_removes_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Facts"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let (_, candidate) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "The lamp remembers names."},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "fact-candidate-1"
        })),
    )
    .await;
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "brief",
            "revisionId": candidate["revisionId"],
            "proposals": [{
                "factKind": "author-secret",
                "readingChapter": 1,
                "subjectId": "white-tower",
                "predicate": "lampDurationSeconds",
                "value": 47,
                "evidence": "The harbor ledger"
            }, {
                "factKind": "world",
                "readingChapter": 1,
                "subjectId": "harbor",
                "predicate": "hasWhiteTower",
                "value": true,
                "evidence": "The harbor ledger"
            }],
            "selections": {},
            "idempotencyKey": "fact-review-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review["id"].as_str().unwrap();
    let fact_id = review["factProposals"][0]["id"].as_str().unwrap();
    let world_fact_id = review["factProposals"][1]["id"].as_str().unwrap();
    let (status, _) = call(
        application(&path),
        "POST",
        &format!("/api/projects/{project_id}/artifacts/plan"),
        Some(json!({
            "payload": {"chapters": [
                {"chapterPlanId": "chapter-one", "chapterNumber": 1, "goal": "Past event"},
                {"chapterPlanId": "chapter-two", "chapterNumber": 2, "goal": "Current chapter"},
                {"chapterPlanId": "chapter-three", "chapterNumber": 3, "goal": "Future secret".repeat(900)}
            ]},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "fact-plan-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let accept_uri = format!("/api/projects/{project_id}/reviews/{review_id}/accept");
    let (status, accepted) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 0,
            "idempotencyKey": "fact-accept-1",
            "selections": {"factDecisions": {fact_id: "confirmed", world_fact_id: "confirmed"}}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let context_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=1&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context) = call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["stateFacts"][0]["id"], fact_id);
    assert_eq!(context["stateFacts"][0]["reviewDecision"], "confirmed");
    assert_eq!(context["stateFacts"][0]["projectRevision"], 1);
    let (status, limited) = call(
        application(&path),
        "GET",
        &context_uri.replace("byteBudget=60000", "byteBudget=4000"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(limited["error"]["code"], "context_budget_exceeded");
    let writer_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=1&purpose=writer&readingChapter=2&sceneOrder=1&budget=24&byteBudget=60000"
    );
    let (status, writer_context) = call(application(&path), "GET", &writer_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{writer_context}");
    assert_eq!(writer_context["stateFacts"].as_array().unwrap().len(), 1);
    assert_eq!(writer_context["stateFacts"][0]["id"], world_fact_id);
    let (status, pov_writer_context) = call(
        application(&path),
        "GET",
        &format!("{writer_uri}&storyTime=10&povCharacterId=linxi"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pov_writer_context}");
    assert_eq!(pov_writer_context["stateFacts"], json!([]));
    assert!(
        pov_writer_context["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .iter()
            .any(|scope| scope == "povEvidence")
    );
    assert!(writer_context["narrativeGoals"]["brief"].is_null());
    assert!(writer_context["narrativeGoals"]["outline"].is_null());
    // This future chapter has no binding. A same-number plan is not its execution authority.
    assert_eq!(writer_context["plannedEffects"], json!([]));
    assert_eq!(
        writer_context["budgetsUsed"]["byteSize"],
        writer_context.to_string().len()
    );
    let creative_uri = format!(
        "/api/projects/{project_id}/knowledge/evolution?dimension=creative&projectRevision=1"
    );
    let (status, creative_trace) = call(application(&path), "GET", &creative_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{creative_trace}");
    let changes = creative_trace["entries"].as_array().unwrap();
    assert!(
        changes
            .iter()
            .any(|entry| entry["change"] == "candidateSaved")
    );
    assert!(
        changes.iter().any(|entry| entry["change"] == "accepted"
            && entry["afterRevisionId"] == candidate["revisionId"]),
        "{creative_trace}"
    );
    let (status, first_page) = call(
        application(&path),
        "GET",
        &format!("{creative_uri}&limit=2"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first_page["entries"].as_array().unwrap().len(), 2);
    let cursor = first_page["nextCursor"].as_str().unwrap();
    let (status, next_page) = call(
        application(&path),
        "GET",
        &format!("{creative_uri}&limit=2&cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(next_page["entries"].as_array().unwrap().len(), 1);
    assert!(next_page["nextCursor"].is_null());

    let commit_id = accepted["commitId"].as_str().unwrap();
    let undo_uri = format!("/api/projects/{project_id}/commits/{commit_id}/undo");
    let (status, _) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedRevision": 1,
            "idempotencyKey": "fact-undo-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let context_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=2&purpose=planner&budget=24&byteBudget=60000"
    );
    let (_, context) = call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(context["stateFacts"], json!([]));
    let (_, after_undo) = call(
        application(&path),
        "GET",
        &creative_uri.replace("projectRevision=1", "projectRevision=2"),
        None,
    )
    .await;
    let undo_entry = after_undo["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["change"] == "undone")
        .unwrap();
    assert_eq!(undo_entry["beforeRevisionId"], candidate["revisionId"]);
    assert!(undo_entry["afterRevisionId"].is_null());
    let (status, _) = call(
        application(&path),
        "GET",
        &format!(
            "{}&cursor={cursor}",
            creative_uri.replace("projectRevision=1", "projectRevision=2")
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn confirmed_fact_creates_and_resolves_planning_impact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Impacts"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let outline_uri = format!("/api/projects/{project_id}/artifacts/outline");
    let (_, outline) = call(
        application(&path),
        "POST",
        &outline_uri,
        Some(json!({
            "payload": {"nodes": [{
                "id": "volume-1", "kind": "volume", "position": 1,
                "title": "The Tower", "purpose": "Reveal the ledger", "summary": "",
                "dependencyKeys": ["lampDurationSeconds"], "certainty": "settled",
                "protection": "preserve-intent"
            }]},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "impact-outline-1"
        })),
    )
    .await;
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (_, outline_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": outline["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "impact-outline-review-1"
        })),
    )
    .await;
    let outline_accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        outline_review["id"].as_str().unwrap()
    );
    let (_, _) = call(
        application(&path),
        "POST",
        &outline_accept_uri,
        Some(json!({"expectedRevision": 0, "idempotencyKey": "impact-outline-accept-1", "selections": {}})),
    )
    .await;
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (_, draft) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "Opening", "body": "Forty-seven seconds."},
            "expectedRevision": 1, "expectedCandidateRevisionId": null,
            "idempotencyKey": "impact-draft-1"
        })),
    )
    .await;
    let (_, draft_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "draft", "revisionId": draft["revisionId"],
            "proposals": [{"sceneGroupId":draft["sceneGroups"][0]["id"],"predicate": "lampDurationSeconds", "value": 47, "evidence": "Forty-seven seconds."}],
            "selections": {}, "idempotencyKey": "impact-draft-review-1"
        })),
    )
    .await;
    let fact_id = draft_review["factProposals"][0]["id"].as_str().unwrap();
    let accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        draft_review["id"].as_str().unwrap()
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 1, "idempotencyKey": "impact-draft-accept-1",
            "selections": {"sceneGroupIds":[draft["sceneGroups"][0]["id"]],"factDecisions": {fact_id: "confirmed"}}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let impacts_uri = format!("/api/projects/{project_id}/impacts?limit=25");
    let (status, impacts) = call(application(&path), "GET", &impacts_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(impacts["counts"]["open"], 1);
    assert_eq!(impacts["data"][0]["targetNodeId"], "volume-1");
    assert_eq!(impacts["data"][0]["severity"], "locked-conflict");
    let original_impact_id = impacts["data"][0]["id"].as_str().unwrap();

    let (_, revised_outline) = call(
        application(&path),
        "POST",
        &outline_uri,
        Some(json!({
            "payload": {"nodes": [{
                "id": "volume-1", "kind": "volume", "position": 1,
                "title": "The Tower", "purpose": "Reveal the ledger", "summary": "The reveal moves later.",
                "dependencyKeys": ["lampDurationSeconds"], "certainty": "settled",
                "protection": "free"
            }]},
            "expectedRevision": 2,
            "expectedCandidateRevisionId": outline["revisionId"],
            "idempotencyKey": "impact-outline-2"
        })),
    )
    .await;
    let (_, revised_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": revised_outline["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "impact-outline-review-2"
        })),
    )
    .await;
    let revised_accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        revised_review["id"].as_str().unwrap()
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &revised_accept_uri,
        Some(json!({"expectedRevision": 2, "idempotencyKey": "impact-outline-accept-2", "selections": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, impacts) = call(application(&path), "GET", &impacts_uri, None).await;
    assert_eq!(impacts["counts"]["open"], 1);
    assert_eq!(
        impacts["data"][0]["targetRevisionId"],
        revised_outline["revisionId"]
    );
    assert_eq!(impacts["data"][0]["severity"], "affected");
    assert_ne!(impacts["data"][0]["id"], original_impact_id);

    let impact_id = impacts["data"][0]["id"].as_str().unwrap();
    let resolve_uri = format!("/api/projects/{project_id}/impacts/{impact_id}/resolve");
    let (status, resolved) = call(
        application(&path),
        "POST",
        &resolve_uri,
        Some(json!({
            "expectedRevision": 3, "resolution": "kept", "note": "Keep the intended reveal",
            "idempotencyKey": "impact-resolve-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resolved["resultRevision"], 4);
    let (_, impacts) = call(application(&path), "GET", &impacts_uri, None).await;
    assert_eq!(impacts["counts"]["open"], 0);
    assert_eq!(impacts["data"][0]["state"], "resolved-kept");
}

use novel_codex_story_core::{EpistemicIndexIntegrityRepository, KnowledgeProjectionRepository, ModelingRepository, ProjectRevisionRepository, RelationGraphRepository, TaskKnowledgeSearchRepository};
