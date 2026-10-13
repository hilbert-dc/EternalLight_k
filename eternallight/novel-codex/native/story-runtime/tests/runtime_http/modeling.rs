use super::*;

#[tokio::test]
async fn modeling_candidates_follow_exact_draft_revision_and_reject_late_publication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Knowledge"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (_, draft) = call(application(&path), "POST", &draft_uri, Some(json!({
        "payload": {"chapterNumber": 1, "title": "信", "body": "林夕烧掉信件。张衡是林夕的父亲。林夕知道密函已送出。二十日后，监护关系解除。"},
        "expectedRevision": 0, "expectedCandidateRevisionId": null, "idempotencyKey": "knowledge-draft-1"
    }))).await;
    let object_id = format!("draft-{}", draft["chapterId"].as_str().unwrap());
    let batch_uri = format!("/api/projects/{project_id}/modeling-batches");
    let (status, batch) = call(
        application(&path),
        "POST",
        &batch_uri,
        Some(json!({
            "objectId": object_id, "sourceRevisionId": draft["revisionId"],
            "expectedRevision": 0, "extractorVersion": "test-v1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let batch_id = batch["id"].as_str().unwrap();
    let capability = "model-secret-123456789012345678901234567890";
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let task_uri = format!("/api/projects/{project_id}/tasks");
    let (status, task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "modelKnowledge", "inputRevision": 0, "threadId": "thread-model",
            "target": {"batchId": batch_id}, "capabilityHash": capability_hash,
            "capabilityExpiresAt": expires_at, "authorIntent": "Extract grounded knowledge"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context_before_modeling) =
        call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{context_before_modeling}");
    assert_eq!(
        context_before_modeling["sourceWatermarks"]["knowledgeBatchCount"],
        0
    );
    let complete_uri = format!("/api/projects/{project_id}/tasks/{task_id}/complete");
    let (status, _) = call(
        application(&path),
        "POST",
        &complete_uri,
        Some(json!({
            "threadId": "thread-model", "summary": "not yet published"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let publish_uri = format!("/api/projects/{project_id}/modeling-batches/{batch_id}/publish");
    let assertion = json!({
        "kind": "event", "subjectId": "burn-letter", "predicate": "burnedLetter",
        "value": {"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null}, "storyTime": 10,
        "evidence": {"quote": "烧掉信件", "start": 2, "end": 6}
    });
    let relation = json!({
        "kind": "relation", "subjectId": "zhangheng", "predicate": "parentOf",
        "relationType": "parentOf", "targetId": "linxi", "propositionId": "father-linxi", "value": true,
        "validFrom": 1, "validUntilExclusive": 20,
        "evidence": {"quote": "张衡是林夕的父亲", "start": 7, "end": 15},
        "validUntilEvidence": {"quote": "监护关系解除", "start": 31, "end": 37}
    });
    let epistemic = json!({
        "kind": "epistemic", "subjectId": "letter-sent", "predicate": "deliveryState",
        "propositionId": "letter-sent", "epistemicOwnerId": "linxi",
        "epistemicState": "known", "epistemicTime": 12, "value": true,
        "evidence": {"quote": "知道密函已送出", "start": 18, "end": 25}
    });
    let entities = json!([
        {"id":"burn-letter","kind":"event","displayName":"烧掉信件","aliases":[],"evidence":{"quote":"烧掉信件","start":2,"end":6}},
        {"id": "linxi", "kind": "character", "displayName": "林夕", "aliases": ["小夕"],
            "evidence": {"quote": "林夕", "start": 0, "end": 2}},
        {"id": "zhangheng", "kind": "character", "displayName": "张衡", "aliases": [],
            "evidence": {"quote": "张衡", "start": 7, "end": 9}},
        {"id": "letter-sent", "kind": "proposition", "displayName": "密函已送出", "aliases": [],
            "evidence": {"quote": "密函已送出", "start": 20, "end": 25}},
        {"id": "father-linxi", "kind": "proposition", "displayName": "张衡是林夕的父亲", "aliases": [],
            "evidence": {"quote": "张衡是林夕的父亲", "start": 7, "end": 15}}
    ]);
    let mut mismatched_event = assertion.clone();
    mismatched_event["evidence"] = json!({"quote":"藏起信件","start":2,"end":6});
    let (status, rejected) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities,
            "assertions": [mismatched_event]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities,
            "assertions": [{"kind": "relation", "subjectId": "linxi", "predicate": "knows",
                "relationType": "knows", "targetId": "letter-sent", "propositionId": "letter-sent", "value": true,
                "evidence": {"quote": "知道密函已送出", "start": 18, "end": 25}}]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities,
            "assertions": [{"kind": "relation", "subjectId": "zhangheng", "predicate": "parentOf",
                "relationType": "parentOf", "targetId": "linxi", "propositionId": "father-linxi", "value": true,
                "validFrom": 20, "validUntilExclusive": 1,
                "evidence": {"quote": "张衡是林夕的父亲", "start": 7, "end": 15},
                "validUntilEvidence": {"quote": "监护关系解除", "start": 31, "end": 37}}]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities,
            "assertions": [{"kind": "relation", "subjectId": "zhangheng", "predicate": "parentOf",
                "relationType": "parentOf", "targetId": "undeclared-character", "propositionId": "father-linxi", "value": true,
                "evidence": {"quote": "张衡是林夕的父亲", "start": 7, "end": 15}}]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities,
            "assertions": [{"kind": "relation", "subjectId": "zhangheng", "predicate": "parentOf",
                "relationType": "parentOf", "targetId": "letter-sent", "propositionId": "father-linxi", "value": true,
                "evidence": {"quote": "张衡是林夕的父亲", "start": 7, "end": 15}}]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut unlinked_relation = relation.clone();
    unlinked_relation
        .as_object_mut()
        .unwrap()
        .remove("propositionId");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities, "assertions": [unlinked_relation]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut wrong_proposition = relation.clone();
    wrong_proposition["propositionId"] = json!("linxi");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities, "assertions": [wrong_proposition]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, published) = call_as(
        application(&path),
        "POST",
        &publish_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-model", "expectedRevision": 0,
            "entities": entities,
            "assertions": [assertion, relation, epistemic]
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let epistemic_audit = SqliteStoryRepository::open(&path)
        .unwrap()
        .audit_epistemic_index(&ProjectId::parse(project_id.to_owned()).unwrap())
        .unwrap();
    assert!(epistemic_audit.complete());
    assert_eq!(epistemic_audit.expected_claims, 1);
    let assertion_audit = SqliteStoryRepository::open(&path)
        .unwrap()
        .audit_assertion_index(&ProjectId::parse(project_id.to_owned()).unwrap())
        .unwrap();
    assert!(assertion_audit.complete());
    assert_eq!(assertion_audit.expected_claims, 3);
    let (status, context_after_modeling) =
        call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{context_after_modeling}");
    assert_eq!(
        context_after_modeling["snapshotId"],
        context_before_modeling["snapshotId"]
    );
    assert_eq!(
        context_after_modeling["sourceWatermarks"]["knowledgeBatchCount"],
        1
    );
    assert_ne!(
        context_after_modeling["sourceWatermarks"]["digest"],
        context_before_modeling["sourceWatermarks"]["digest"]
    );
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, modeling_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "draft", "revisionId": draft["revisionId"],
            "modelingBatchId": batch_id, "proposals": [], "selections": {},
            "idempotencyKey": "review-modeling-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{modeling_review}");
    assert_eq!(
        modeling_review["factProposals"].as_array().unwrap().len(),
        3
    );
    assert_eq!(
        modeling_review["factProposals"][1]["modelingBatchId"],
        batch_id
    );
    assert_eq!(
        modeling_review["factProposals"][1]["evidence"],
        "张衡是林夕的父亲"
    );
    assert_eq!(
        modeling_review["factProposals"][1]["sourceRevisionId"],
        draft["revisionId"]
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "draft", "revisionId": draft["revisionId"],
            "modelingBatchId": batch_id,
            "proposals": [{"predicate": "fabricated"}],
            "idempotencyKey": "review-modeling-forged"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        application(&path),
        "POST",
        &complete_uri,
        Some(json!({
            "threadId": "thread-model", "summary": "grounded knowledge published"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let knowledge_uri = format!("/api/projects/{project_id}/knowledge");
    let (_, knowledge) = call(application(&path), "GET", &knowledge_uri, None).await;
    assert_eq!(knowledge["sources"][0]["status"], "ready");
    assert_eq!(knowledge["sources"][0]["confirmationState"], "candidate");
    assert_eq!(knowledge["sources"][0]["entities"], entities);
    let snapshot_knowledge_uri = format!(
        "/api/projects/{project_id}/knowledge/snapshots/{}/objects/{object_id}",
        context_before_modeling["snapshotId"].as_str().unwrap()
    );
    let (status, historic) = call(application(&path), "GET", &snapshot_knowledge_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historic}");
    assert_eq!(historic["status"], "model_pending");
    assert_eq!(historic["sourceRevisionId"], draft["revisionId"]);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &snapshot_knowledge_uri,
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let state_uri = format!("/api/projects/{project_id}/knowledge/state");
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author"),
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, world) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=world&worldTime=10"),
        None,
    )
    .await;
    assert_eq!(world["entries"].as_array().unwrap().len(), 2);
    assert_eq!(
        world["entries"][0]["assertion"]["predicate"],
        "burnedLetter"
    );
    assert_eq!(world["coverage"]["unknownTimeSkipped"], 0);
    let (_, world_before_relation) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=world&worldTime=0"),
        None,
    )
    .await;
    assert_eq!(world_before_relation["entries"], json!([]));
    let (_, world_after_relation) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=world&worldTime=20"),
        None,
    )
    .await;
    assert_eq!(world_after_relation["entries"].as_array().unwrap().len(), 1);
    let (_, character_before) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=character&ownerId=linxi&epistemicTime=11"),
        None,
    )
    .await;
    assert_eq!(character_before["entries"], json!([]));
    let (_, character_after) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=character&ownerId=linxi&epistemicTime=12"),
        None,
    )
    .await;
    assert_eq!(character_after["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        character_after["entries"][0]["assertion"]["epistemicState"],
        "known"
    );
    let (status, wrong_coordinate) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=character&ownerId=linxi&worldTime=12"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wrong_coordinate}");
    let (status, wrong_world_coordinate) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=world&epistemicTime=12"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wrong_world_coordinate}");
    let (_, author_before_awareness) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author&worldTime=10&epistemicTime=11"),
        None,
    )
    .await;
    assert_eq!(
        author_before_awareness["entries"].as_array().unwrap().len(),
        2
    );
    let (_, author_after_awareness) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author&worldTime=10&epistemicTime=12"),
        None,
    )
    .await;
    assert_eq!(
        author_after_awareness["entries"].as_array().unwrap().len(),
        3
    );
    assert_eq!(author_after_awareness["worldTime"], 10);
    assert_eq!(author_after_awareness["epistemicTime"], 12);
    let (_, reader_before) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=reader&readingChapter=0"),
        None,
    )
    .await;
    assert_eq!(reader_before["entries"], json!([]));
    let (_, reader_after) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=reader&readingChapter=1"),
        None,
    )
    .await;
    assert_eq!(reader_after["entries"].as_array().unwrap().len(), 3);
    let (_, author) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author"),
        None,
    )
    .await;
    assert_eq!(
        author["entries"][1]["relation"]["direction"],
        "sourceToTarget"
    );
    assert_eq!(author["entries"][1]["relation"]["transitive"], false);

    let evolution_uri = format!("/api/projects/{project_id}/knowledge/evolution");
    let (status, world_trace) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&projectRevision=0"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(world_trace["entries"].as_array().unwrap().len(), 3);
    assert_eq!(world_trace["entries"][0]["worldTime"], 1);
    assert_eq!(world_trace["entries"][1]["worldTime"], 10);
    assert_eq!(world_trace["entries"][2]["change"], "relationEnded");
    assert_eq!(world_trace["entries"][2]["after"], Value::Null);
    assert_eq!(world_trace["coverage"]["undatedEventCount"], 0);
    assert_eq!(world_trace["coverage"]["chronologyComplete"], true);
    assert_eq!(world_trace["coverage"]["complete"], true);
    assert_eq!(world_trace["coverage"]["semanticCompleteness"], "unknown");
    assert_eq!(
        world_trace["entries"][2]["evidence"]["quote"],
        "监护关系解除"
    );
    let (_, first_world_page) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&projectRevision=0&limit=2"),
        None,
    )
    .await;
    let world_cursor = first_world_page["nextCursor"].as_str().unwrap();
    let (_, second_world_page) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&projectRevision=0&limit=2&cursor={world_cursor}"),
        None,
    )
    .await;
    assert_eq!(second_world_page["entries"][0]["change"], "relationEnded");
    assert_eq!(second_world_page["entries"].as_array().unwrap().len(), 1);
    assert!(second_world_page["nextCursor"].is_null());
    let (_, narrative_trace) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=narrative&projectRevision=0"),
        None,
    )
    .await;
    assert_eq!(narrative_trace["entries"].as_array().unwrap().len(), 3);
    assert_eq!(narrative_trace["entries"][0]["readingChapter"], 1);
    assert_eq!(narrative_trace["entries"][0]["change"], "disclosedInDraft");
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&projectRevision=0"),
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, unrelated_draft) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 2, "title": "另一章", "body": "海风吹过港口。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "knowledge-unrelated-draft"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, context_after_new_snapshot) =
        call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{context_after_new_snapshot}");
    let historic_ready_uri = format!(
        "/api/projects/{project_id}/knowledge/snapshots/{}/objects/{object_id}",
        context_after_new_snapshot["snapshotId"].as_str().unwrap()
    );
    let (status, historic_ready) = call(application(&path), "GET", &historic_ready_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historic_ready}");
    assert_eq!(historic_ready["status"], "ready");
    assert_eq!(historic_ready["batchId"], batch_id);
    let alias_uri =
        format!("/api/projects/{project_id}/knowledge/entities?name=%E5%B0%8F%E5%A4%95");
    let (_, alias_before_edit) = call(application(&path), "GET", &alias_uri, None).await;
    assert_eq!(alias_before_edit["entries"][0]["id"], "linxi");
    let (_, second_batch) = call(
        application(&path),
        "POST",
        &batch_uri,
        Some(json!({
            "objectId": format!("draft-{}", unrelated_draft["chapterId"].as_str().unwrap()),
            "sourceRevisionId": unrelated_draft["revisionId"],
            "expectedRevision": 0, "extractorVersion": "test-v1"
        })),
    )
    .await;
    let second_batch_id = second_batch["id"].as_str().unwrap();
    let second_capability = "second-model-secret-123456789012345678901234";
    let second_hash = format!("{:x}", Sha256::digest(second_capability.as_bytes()));
    let (_, second_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "modelKnowledge", "inputRevision": 0, "threadId": "thread-model-2",
            "target": {"batchId": second_batch_id}, "capabilityHash": second_hash,
            "capabilityExpiresAt": expires_at, "authorIntent": "Model the second chapter"
        })),
    )
    .await;
    let second_context_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-model-2&projectRevision=0&purpose=critic&budget=24&byteBudget=60000",
        second_task["id"].as_str().unwrap()
    );
    let (status, second_context) = call_as(
        application(&path),
        "GET",
        &second_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second_context}");
    let entity_catalog_source = read_frozen_context_source(
        &path, project_id, &second_task, second_capability, &second_context, "/operationContext/entityCatalog",
    ).await;
    let catalog_ids = entity_catalog_source["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect::<Vec<_>>();
    assert!(catalog_ids.contains(&"letter-sent"));
    assert!(catalog_ids.contains(&"father-linxi"));
    let father = entity_catalog_source["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "father-linxi")
        .unwrap();
    assert_eq!(father["claims"][0]["modelingBatchId"], batch_id);
    let observer_capability = "knowledge-observer-secret-123456789012345";
    let observer_hash = format!("{:x}", Sha256::digest(observer_capability.as_bytes()));
    let (status, observer_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "advisory", "inputRevision": 0, "threadId": "thread-knowledge-observer",
            "target": {"type":"project"}, "capabilityHash": observer_hash,
            "capabilityExpiresAt": expires_at, "authorIntent": "Inspect current knowledge"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{observer_task}");
    let observer_context_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-knowledge-observer&projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
        observer_task["id"].as_str().unwrap()
    );
    let (status, _) = call_as(
        application(&path),
        "GET",
        &observer_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(observer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_publish_uri =
        format!("/api/projects/{project_id}/modeling-batches/{second_batch_id}/publish");
    let (status, conflict) = call_as(
        application(&path),
        "POST",
        &second_publish_uri,
        Some(json!({
            "taskId": second_task["id"], "threadId": "thread-model-2", "expectedRevision": 0,
            "entities": [{"id": "linxi", "kind": "location", "displayName": "港口",
                "aliases": [], "evidence": {"quote": "港口", "start": 4, "end": 6}}],
            "assertions": []
        })),
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{conflict}");
    let (status, second_published) = call_as(
        application(&path),
        "POST",
        &second_publish_uri,
        Some(json!({
            "taskId": second_task["id"], "threadId": "thread-model-2", "expectedRevision": 0,
            "entities": [{"id": "harbor-person", "kind": "character", "displayName": "港口",
                "aliases": ["林夕"], "evidence": {"quote": "港口", "start": 4, "end": 6}}],
            "assertions": []
        })),
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second_published}");
    let (status, stale_observer_context) = call_as(
        application(&path),
        "GET",
        &observer_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(observer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_observer_context}");
    let (status, stale_completion) = call_as(
        application(&path),
        "POST",
        &format!(
            "/api/projects/{project_id}/tasks/{}/complete",
            observer_task["id"].as_str().unwrap()
        ),
        Some(json!({"threadId": "thread-knowledge-observer", "summary": "Outdated observation"})),
        CANDIDATE_TOKEN,
        Some(observer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_completion}");
    let catalog_uri = format!("/api/projects/{project_id}/knowledge/entities");
    let (status, catalog) = call(application(&path), "GET", &catalog_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{catalog}");
    assert_eq!(catalog["truncated"], false);
    assert_eq!(catalog["ambiguityScanComplete"], true);
    assert_eq!(
        catalog["ambiguousNames"],
        json!([{
            "name": "林夕", "candidateIds": ["harbor-person", "linxi"],
            "canonicalIds": ["harbor-person", "linxi"],
            "authorDecidedDistinct": false
        }])
    );
    let harbor_entry = catalog["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "harbor-person")
        .unwrap();
    assert_eq!(
        harbor_entry["claims"][0]["sourceRevisionId"],
        unrelated_draft["revisionId"]
    );
    assert_eq!(harbor_entry["claims"][0]["evidence"]["quote"], "港口");
    let (status, _) = call_as(
        application(&path),
        "GET",
        &catalog_uri,
        None,
        CANDIDATE_TOKEN,
        Some(second_capability),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, after_unrelated) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=reader&readingChapter=1"),
        None,
    )
    .await;
    assert_eq!(after_unrelated["entries"].as_array().unwrap().len(), 3);
    assert_eq!(after_unrelated["coverage"]["complete"], true);
    let writer_capability = "writer-secret-123456789012345678901234567890";
    let writer_hash = format!("{:x}", Sha256::digest(writer_capability.as_bytes()));
    let (_, writer_task) = call(application(&path), "POST", &task_uri, Some(json!({
        "operation": "generateChapter", "inputRevision": 0, "threadId": "thread-writer-1",
        "target": {"chapterId": draft["chapterId"], "chapterNumber": 1, "readingChapter": 1, "sceneOrder": 1},
        "capabilityHash": writer_hash, "capabilityExpiresAt": expires_at,
        "authorIntent": "Continue chapter one"
    }))).await;
    let writer_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-writer-1&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000",
        writer_task["id"].as_str().unwrap()
    );
    let (status, writer_context) = call_as(
        application(&path),
        "GET",
        &writer_uri,
        None,
        CANDIDATE_TOKEN,
        Some(writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{writer_context}");
    let writer_text_source = read_frozen_context_source(
        &path, project_id, &writer_task, writer_capability, &writer_context, "/relevantText",
    ).await;
    assert_eq!(writer_text_source.as_array().unwrap().len(), 1);
    let writer_modeling_source = read_frozen_context_source(
        &path, project_id, &writer_task, writer_capability, &writer_context, "/operationContext/modeling",
    ).await;
    assert_eq!(
        writer_modeling_source
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mismatched_uri = writer_uri.replace("readingChapter=1", "readingChapter=2");
    let (status, _) = call_as(
        application(&path),
        "GET",
        &mismatched_uri,
        None,
        CANDIDATE_TOKEN,
        Some(writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, next_writer) = call(application(&path), "POST", &task_uri, Some(json!({
        "operation": "generateChapter", "inputRevision": 0, "threadId": "thread-writer-2",
        "target": {"chapterId": unrelated_draft["chapterId"], "chapterNumber": 2, "readingChapter": 2, "sceneOrder": 1},
        "capabilityHash": writer_hash, "capabilityExpiresAt": expires_at,
        "authorIntent": "Write chapter two"
    }))).await;
    let next_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-writer-2&projectRevision=0&purpose=writer&readingChapter=2&sceneOrder=1&budget=24&byteBudget=60000",
        next_writer["id"].as_str().unwrap()
    );
    let (status, next_context) = call_as(
        application(&path),
        "GET",
        &next_uri,
        None,
        CANDIDATE_TOKEN,
        Some(writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next_context}");
    let next_knowledge = read_frozen_context_source(
        &path, project_id, &next_writer, writer_capability, &next_context, "/operationContext/workingKnowledge",
    ).await;
    assert_eq!(
        next_knowledge["entries"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        next_knowledge["coverage"]["complete"],
        true
    );

    let (status, revised) = call(application(&path), "POST", &draft_uri, Some(json!({
        "payload": {"chapterId": draft["chapterId"], "chapterNumber": 1, "title": "信", "body": "林夕藏起信件。"},
        "expectedRevision": 0, "expectedCandidateRevisionId": draft["revisionId"],
        "idempotencyKey": "knowledge-draft-2"
    }))).await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, knowledge) = call(application(&path), "GET", &knowledge_uri, None).await;
    let revised_source = knowledge["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["objectId"] == object_id)
        .unwrap();
    assert_eq!(revised_source["status"], "model_pending");
    assert_eq!(revised_source["assertions"], json!([]));
    assert_eq!(revised_source["sourceRevisionId"], revised["revisionId"]);
    let (_, alias_after_edit) = call(application(&path), "GET", &alias_uri, None).await;
    assert_eq!(alias_after_edit["entries"], json!([]));
    let (_, historical_alias_after_edit) = call(
        application(&path),
        "GET",
        &format!(
            "{alias_uri}&snapshotId={}",
            context_after_new_snapshot["snapshotId"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(historical_alias_after_edit["entries"][0]["id"], "linxi");
    let (_, stale_state) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author"),
        None,
    )
    .await;
    assert_eq!(stale_state["entries"], json!([]));
    assert_eq!(stale_state["coverage"]["complete"], false);
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&projectRevision=0&cursor={world_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, pending_trace) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&projectRevision=0"),
        None,
    )
    .await;
    assert_eq!(pending_trace["entries"], json!([]));
    assert_eq!(pending_trace["coverage"]["complete"], false);
    assert_eq!(pending_trace["coverage"]["pendingSourceCount"], 1);
    let historical_snapshot = context_after_new_snapshot["snapshotId"].as_str().unwrap();
    let (status, historical_state) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author&snapshotId={historical_snapshot}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_state}");
    assert_eq!(historical_state["entries"].as_array().unwrap().len(), 3);
    assert_eq!(
        historical_state["entries"][0]["confirmationState"],
        "candidate"
    );
    assert_eq!(
        historical_state["coverage"]["pendingSources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let historical_world_uri =
        format!("{evolution_uri}?dimension=world&snapshotId={historical_snapshot}");
    let (status, historical_world) =
        call(application(&path), "GET", &historical_world_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historical_world}");
    assert_eq!(historical_world["entries"].as_array().unwrap().len(), 3);
    assert_eq!(historical_world["entries"][1]["predicate"], "burnedLetter");
    assert_eq!(
        historical_world["entries"][1]["confirmationState"],
        "candidate"
    );
    let (_, historical_first_page) = call(
        application(&path),
        "GET",
        &format!("{historical_world_uri}&limit=2"),
        None,
    )
    .await;
    let historical_cursor = historical_first_page["nextCursor"].as_str().unwrap();
    let (status, historical_second_page) = call(
        application(&path),
        "GET",
        &format!("{historical_world_uri}&limit=2&cursor={historical_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_second_page}");
    assert_eq!(
        historical_second_page["entries"].as_array().unwrap().len(),
        1
    );
    let (_, historical_narrative) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=narrative&snapshotId={historical_snapshot}"),
        None,
    )
    .await;
    assert_eq!(historical_narrative["entries"].as_array().unwrap().len(), 3);
    let (_, historical_creative) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=creative&snapshotId={historical_snapshot}"),
        None,
    )
    .await;
    assert_eq!(historical_creative["entries"].as_array().unwrap().len(), 2);
    let (_, revised_catalog) = call(application(&path), "GET", &catalog_uri, None).await;
    assert_eq!(revised_catalog["ambiguousNames"], json!([]));
    assert_eq!(revised_catalog["entries"].as_array().unwrap().len(), 1);
    let (status, _) = call(
        application(&path),
        "POST",
        &batch_uri,
        Some(json!({
            "objectId": object_id, "sourceRevisionId": draft["revisionId"],
            "expectedRevision": 0, "extractorVersion": "test-v2"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let stale_accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        modeling_review["id"].as_str().unwrap()
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &stale_accept_uri,
        Some(json!({
            "expectedRevision": 0, "idempotencyKey": "stale-modeling-accept", "selections": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, current_batch) = call(
        application(&path),
        "POST",
        &batch_uri,
        Some(json!({
            "objectId": object_id, "sourceRevisionId": revised["revisionId"],
            "expectedRevision": 0, "extractorVersion": "test-v2"
        })),
    )
    .await;
    let current_batch_id = current_batch["id"].as_str().unwrap();
    let current_capability = "current-model-secret-1234567890123456789012";
    let current_hash = format!("{:x}", Sha256::digest(current_capability.as_bytes()));
    let (_, current_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "modelKnowledge", "inputRevision": 0, "threadId": "thread-model-current",
            "target": {"batchId": current_batch_id}, "capabilityHash": current_hash,
            "capabilityExpiresAt": expires_at, "authorIntent": "Model the revised source"
        })),
    )
    .await;
    let current_publish_uri =
        format!("/api/projects/{project_id}/modeling-batches/{current_batch_id}/publish");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &current_publish_uri,
        Some(json!({
            "taskId": current_task["id"], "threadId": "thread-model-current", "expectedRevision": 0,
            "entities": [{"id": "linxi", "kind": "character", "displayName": "林夕",
                "aliases": [], "evidence": {"quote": "林夕", "start": 0, "end": 2}},
                {"id":"hide-letter","kind":"event","displayName":"藏起信件","aliases":[],"evidence":{"quote":"藏起信件","start":2,"end":6}}],
            "assertions": [{"kind": "event", "subjectId": "hide-letter", "predicate": "hidLetter", "storyTime":null,
                "value": {"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null}, "evidence": {"quote": "藏起信件", "start": 2, "end": 6}}]
        })),
        CANDIDATE_TOKEN,
        Some(current_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, current_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "draft", "revisionId": revised["revisionId"],
            "modelingBatchId": current_batch_id, "proposals": [], "selections": {},
            "idempotencyKey": "review-modeling-current"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{current_review}");
    let fact_id = current_review["factProposals"][0]["id"].as_str().unwrap();
    let scene_group_id = current_review["factProposals"][0]["sceneGroupId"]
        .as_str()
        .unwrap();
    let accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        current_review["id"].as_str().unwrap()
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 0, "idempotencyKey": "accept-modeling-current",
            "selections": {"factDecisions": {fact_id: "confirmed"}}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, accepted) = call(application(&path), "POST", &accept_uri, Some(json!({
        "expectedRevision": 0, "idempotencyKey": "accept-modeling-current",
        "selections": {"sceneGroupIds": [scene_group_id], "factDecisions": {fact_id: "confirmed"}}
    }))).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (_, confirmed_context) = call(application(&path), "GET",
        &format!("/api/projects/{project_id}/context?projectRevision=1&purpose=planner&budget=24&byteBudget=60000"), None).await;
    let confirmed_snapshot = confirmed_context["snapshotId"].as_str().unwrap().to_owned();
    assert_eq!(confirmed_context["stateFacts"], json!([]));
    assert_eq!(
        confirmed_context["coverage"]["scannedScope"]["typedEventFactsWithheld"],
        1
    );
    let (_, confirmed_state) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author"),
        None,
    )
    .await;
    assert_eq!(
        confirmed_state["entries"][0]["confirmationState"],
        "confirmed"
    );
    assert_eq!(confirmed_state["entries"][0]["confirmedFactId"], fact_id);
    assert_eq!(confirmed_state["coverage"]["confirmedEntries"], 1);
    let (status, confirmed_snapshot_state) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author&snapshotId={confirmed_snapshot}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{confirmed_snapshot_state}");
    assert_eq!(
        confirmed_snapshot_state["entries"][0]["confirmationState"],
        "confirmed"
    );
    assert_eq!(
        confirmed_snapshot_state["entries"][0]["confirmedFactId"],
        fact_id
    );
    let confirmed_writer_capability = "confirmed-writer-secret-123456789012345678";
    let confirmed_writer_hash = format!(
        "{:x}",
        Sha256::digest(confirmed_writer_capability.as_bytes())
    );
    let (_, confirmed_writer_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "generateChapter", "inputRevision": 1,
            "threadId": "thread-writer-confirmed",
            "target": {"chapterId": unrelated_draft["chapterId"], "chapterNumber": 2,
                "readingChapter": 2, "sceneOrder": 1},
            "capabilityHash": confirmed_writer_hash, "capabilityExpiresAt": expires_at,
            "authorIntent": "Continue using confirmed knowledge"
        })),
    )
    .await;
    let confirmed_writer_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-writer-confirmed&projectRevision=1&purpose=writer&readingChapter=2&sceneOrder=1&budget=24&byteBudget=60000",
        confirmed_writer_task["id"].as_str().unwrap()
    );
    let (status, confirmed_writer_context) = call_as(
        application(&path),
        "GET",
        &confirmed_writer_uri,
        None,
        CANDIDATE_TOKEN,
        Some(confirmed_writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{confirmed_writer_context}");
    assert_eq!(confirmed_writer_context["stateFacts"], json!([]));
    assert_eq!(
        confirmed_writer_context["coverage"]["scannedScope"]["typedEventFactsWithheld"],
        1
    );
    let confirmed_knowledge = read_frozen_context_source(
        &path, project_id, &confirmed_writer_task, confirmed_writer_capability, &confirmed_writer_context, "/operationContext/workingKnowledge",
    ).await;
    assert_eq!(
        confirmed_knowledge["entries"][0]["confirmationState"],
        "candidate"
    );
    assert!(confirmed_knowledge["entries"][0]["confirmedFactId"].is_null());
    let (status, writer_checkpoint) = call_as(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterId": unrelated_draft["chapterId"], "chapterNumber": 2,
                "title": "另一章", "body": "林夕在港口继续调查。"},
            "expectedRevision": 1,
            "expectedCandidateRevisionId": unrelated_draft["revisionId"],
            "idempotencyKey": "writer-own-checkpoint",
            "taskId": confirmed_writer_task["id"], "threadId": "thread-writer-confirmed"
        })),
        CANDIDATE_TOKEN,
        Some(confirmed_writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{writer_checkpoint}");
    let (status, continued_context) = call_as(
        application(&path),
        "GET",
        &confirmed_writer_uri,
        None,
        CANDIDATE_TOKEN,
        Some(confirmed_writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{continued_context}");
    assert_eq!(continued_context["stateFacts"], json!([]));
    assert_eq!(
        continued_context["coverage"]["scannedScope"]["typedEventFactsWithheld"],
        1
    );
    let (status, changed_draft) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterId": draft["chapterId"], "chapterNumber": 1,
                "title": "信", "body": "林夕交出信件。"},
            "expectedRevision": 1, "expectedCandidateRevisionId": revised["revisionId"],
            "idempotencyKey": "knowledge-draft-3"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{changed_draft}");
    let (status, confirmed_history) = call(
        application(&path),
        "GET",
        &format!("{evolution_uri}?dimension=world&snapshotId={confirmed_snapshot}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{confirmed_history}");
    assert_eq!(confirmed_history["entries"][0]["predicate"], "hidLetter");
    assert_eq!(
        confirmed_history["entries"][0]["confirmationState"],
        "confirmed"
    );
    assert_eq!(confirmed_history["entries"][0]["confirmedFactId"], fact_id);
    let (_, confirmed_snapshot_after_edit) = call(
        application(&path),
        "GET",
        &format!("{state_uri}?perspective=author&snapshotId={confirmed_snapshot}"),
        None,
    )
    .await;
    assert_eq!(
        confirmed_snapshot_after_edit["entries"][0]["confirmedFactId"],
        fact_id
    );
    let (status, old_history_after_acceptance) =
        call(application(&path), "GET", &historical_world_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{old_history_after_acceptance}");
    assert_eq!(
        old_history_after_acceptance["entries"],
        historical_world["entries"]
    );
    let (status, old_page_after_acceptance) = call(
        application(&path),
        "GET",
        &format!("{historical_world_uri}&limit=2&cursor={historical_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{old_page_after_acceptance}");
    assert_eq!(
        old_page_after_acceptance["entries"],
        historical_second_page["entries"]
    );
    let (status, invalid_history_mode) = call(
        application(&path),
        "GET",
        &format!("{historical_world_uri}&projectRevision=1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid_history_mode}");
    let (_, stale_context) = call(application(&path), "GET",
        &format!("/api/projects/{project_id}/context?projectRevision=1&purpose=planner&budget=24&byteBudget=60000"), None).await;
    assert_eq!(stale_context["stateFacts"], json!([]));
    let (status, stale_writer_context) = call_as(
        application(&path),
        "GET",
        &confirmed_writer_uri,
        None,
        CANDIDATE_TOKEN,
        Some(confirmed_writer_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_writer_context}");
    let undo_uri = format!(
        "/api/projects/{project_id}/commits/{}/undo",
        accepted["commitId"].as_str().unwrap()
    );
    let (status, undo_response) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedRevision": 1, "idempotencyKey": "undo-modeling-current"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{undo_response}");
}

use novel_codex_story_core::{AssertionIndexIntegrityRepository, EpistemicIndexIntegrityRepository};
