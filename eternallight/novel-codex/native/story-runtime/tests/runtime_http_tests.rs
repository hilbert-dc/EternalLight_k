use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use novel_codex_story_core::{
    BranchId, KnowledgePerspective, KnowledgeStateQuery, ProjectId, RelationDirection,
    RelationGraphQuery, RelationGraphScope, SqliteStoryRepository, WriterKnowledgeScope,
};
use novel_codex_story_runtime::entity_audit_tick;
use novel_codex_story_runtime::fact_audit_tick;
use novel_codex_story_runtime::knowledge_audit_tick;
use novel_codex_story_runtime::relation_audit_tick;
use novel_codex_story_runtime::router;
use novel_codex_story_runtime::search_audit_tick;
use novel_codex_story_runtime::snapshot_knowledge_audit_tick;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

const AUTHOR_TOKEN: &str = "author-token-12345678901234567890123";
const CANDIDATE_TOKEN: &str = "candidate-token-12345678901234567890";

#[tokio::test]
async fn proposal_set_http_previews_applies_and_undoes_a_group() {
    use novel_codex_story_core::{
        BranchId, CommitId, CreateReviewRequest, IdempotencyKey, ObjectId, RevisionId,
        SaveCandidateRequest,
    };

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("proposal-set.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"多对象提案"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = ProjectId::parse(created["project"]["id"].as_str().unwrap()).unwrap();
    let branch = BranchId::parse("main").unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let mut snapshot = repository
        .branch_head(&project, &branch)
        .unwrap()
        .unwrap()
        .snapshot_id;
    for (object, kind, revision, review) in [
        (
            "artifact-brief",
            "brief",
            "proposal-brief-revision",
            "proposal-brief-review",
        ),
        (
            "artifact-outline",
            "outline",
            "proposal-outline-revision",
            "proposal-outline-review",
        ),
    ] {
        let object_id = ObjectId::parse(object).unwrap();
        let revision_id = RevisionId::parse(revision).unwrap();
        let next_snapshot_id =
            novel_codex_story_core::SnapshotId::parse(format!("snapshot-{review}")).unwrap();
        repository
            .save_candidate(SaveCandidateRequest {
                project_id: project.clone(),
                branch_id: branch.clone(),
                object_id: object_id.clone(),
                object_kind: kind.into(),
                revision_id: revision_id.clone(),
                commit_id: CommitId::parse(format!("candidate-{review}")).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("save-{review}")).unwrap(),
                request_digest: review.into(),
                expected_project_revision: 0,
                expected_snapshot_id: snapshot,
                next_snapshot_id: next_snapshot_id.clone(),
                expected_candidate_revision_id: None,
                content_digest: "content".into(),
                content_json: "{}".into(),
                chapter_projection: None,
                task_output: None,
                linked_impact_ids: vec![],
            })
            .unwrap();
        repository
            .create_review(CreateReviewRequest {
                review_id: review.into(),
                project_id: project.clone(),
                object_id,
                object_kind: kind.into(),
                revision_id,
                idempotency_key: IdempotencyKey::parse(format!("review-{review}")).unwrap(),
                request_digest: review.into(),
                checks_json: "[]".into(),
                proposals_json: "[]".into(),
                modeling_batch_id: None,
                selections_json: "{}".into(),
            })
            .unwrap();
        snapshot = next_snapshot_id;
    }
    let uri = format!("/api/projects/{}/proposal-sets", project.as_str());
    let request = json!({"baseSnapshotId":snapshot.as_str(), "intent":"先改设定再改大纲",
        "operations":[
            {"operationId":"brief","reviewId":"proposal-brief-review","dependsOn":[]},
            {"operationId":"outline","reviewId":"proposal-outline-review","dependsOn":["brief"]}
        ], "idempotencyKey":"proposal-http-key"});
    let (status, _) = call_as(
        application(&path),
        "POST",
        &uri,
        Some(request.clone()),
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, proposal) = call(application(&path), "POST", &uri, Some(request)).await;
    assert_eq!(status, StatusCode::CREATED, "{proposal}");
    let id = proposal["id"].as_str().unwrap();
    let (status, _) = call_as(application(&path), "GET", &uri, None, CANDIDATE_TOKEN, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, listing) = call(application(&path), "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listing["data"][0]["id"], id);
    assert_eq!(listing["data"][0]["status"], "proposed");
    let (status, invalid_cursor) =
        call(application(&path), "GET", &format!("{uri}?cursor=-1"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid_cursor}");
    let preview_uri = format!("{uri}/{id}/preview");
    let (status, partial) = call(
        application(&path),
        "POST",
        &preview_uri,
        Some(json!({"selectedOperationIds":["outline"],"selectionsByOperationId":{"outline":{}}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{partial}");
    assert_eq!(partial["selection"]["declaredClosureReady"], false);
    assert_eq!(
        partial["selection"]["missingDependencies"],
        json!([{"operationId":"outline","requires":"brief"}])
    );
    let (status, complete) = call(
        application(&path),
        "POST",
        &preview_uri,
        Some(json!({"selectedOperationIds":["brief","outline"],"selectionsByOperationId":{"brief":{},"outline":{}}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{complete}");
    assert_eq!(complete["selection"]["declaredClosureReady"], true);
    assert_eq!(complete["selection"]["applyReady"], true);
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    let apply_uri = format!("{uri}/{id}/apply");
    let apply_request = json!({
        "selectedOperationIds": ["brief", "outline"],
        "selectionsByOperationId": {"brief": {}, "outline": {}},
        "idempotencyKey": "proposal-http-apply"
    });
    let (status, _) = call_as(
        application(&path),
        "POST",
        &apply_uri,
        Some(apply_request.clone()),
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, applied) = call(
        application(&path),
        "POST",
        &apply_uri,
        Some(apply_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["resultRevision"], 1);
    let (status, applied_status) =
        call(application(&path), "GET", &format!("{uri}/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(applied_status["status"], "applied");
    assert_eq!(
        applied_status["application"]["id"],
        applied["applicationId"]
    );
    let (status, workspace_after_apply) = call(
        application(&path),
        "GET",
        &format!("/api/projects/{}/creative-workspace", project.as_str()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        workspace_after_apply["headSnapshotId"],
        applied["resultSnapshotId"]
    );
    assert_eq!(
        workspace_after_apply["commits"][0]["proposalApplicationId"],
        applied["applicationId"]
    );
    let (_, applied_listing) = call(application(&path), "GET", &uri, None).await;
    assert_eq!(applied_listing["data"][0]["status"], "applied");
    let (status, replay) = call(application(&path), "POST", &apply_uri, Some(apply_request)).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["applicationId"], applied["applicationId"]);
    assert_eq!(replay["replayed"], true);
    let undo_uri = format!(
        "/api/projects/{}/proposal-applications/{}/undo",
        project.as_str(),
        applied["applicationId"].as_str().unwrap()
    );
    let (status, undone) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedProjectRevision": 1, "idempotencyKey": "proposal-http-undo"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    assert_eq!(undone["resultRevision"], 2);
    let (status, undone_status) =
        call(application(&path), "GET", &format!("{uri}/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone_status["status"], "undone");
    let mut cursor = None::<String>;
    let mut group_history = Vec::new();
    for _ in 0..8 {
        let suffix = cursor
            .as_ref()
            .map_or(String::new(), |value| format!("&cursor={value}"));
        let (status, page) = call(
            application(&path), "GET",
            &format!("/api/projects/{}/knowledge/evolution?dimension=creative&projectRevision=2&limit=1{suffix}", project.as_str()),
            None,
        ).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        group_history.extend(
            page["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| entry["proposalApplicationId"] == applied["applicationId"])
                .cloned(),
        );
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(group_history.len(), 4);
    assert!(cursor.is_none());
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
}

#[tokio::test]
async fn later_chapter_can_publish_source_bound_knowledge_of_an_earlier_proposition() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cross-chapter-knowledge.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"跨章节认知"})),
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
    assert_eq!(
        writer_context["operationContext"]["workingKnowledge"]["perspective"],
        "character"
    );
    assert_eq!(
        writer_context["operationContext"]["workingKnowledge"]["entries"],
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
async fn hypothesis_routes_are_author_only_and_reject_unpinned_assumptions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hypothesis-api.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"假设边界"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{project_id}/hypotheses");
    let payload = json!({
        "baseSnapshotId": "missing-snapshot",
        "intent": "如果师父没有离开",
        "assumptions": [{"kind":"assumeClaim", "subjectId":"master", "predicate":"alive", "value":true, "storyTime":1}],
        "idempotencyKey": "hypothesis-api-test",
    });
    let (status, _) = call_as(
        application(&path),
        "POST",
        &uri,
        Some(payload.clone()),
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(application(&path), "POST", &uri, Some(payload)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{uri}/hypothesis-missing"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{uri}/hypothesis-missing"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn impact_analysis_is_author_only_and_never_claims_global_completeness() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("impact-analysis.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"影响分析"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{project_id}/impact-analysis?entityId=event-a");
    let (status, report) = call(application(&path), "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["findings"], json!([]));
    assert_eq!(report["coverage"]["globalImpactComplete"], false);
    assert_eq!(report["authority"], "readOnlyHypothesis");
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{uri}&fromRevisionId=only-one"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_as(application(&path), "GET", &uri, None, CANDIDATE_TOKEN, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

fn application(path: &std::path::Path) -> Router {
    router(task_fixture::state(
        path.to_owned(),
        AUTHOR_TOKEN.into(),
        CANDIDATE_TOKEN.into(),
    ))
}

async fn call(
    application: Router,
    method: &str,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    call_as(application, method, uri, payload, AUTHOR_TOKEN, None).await
}

async fn call_as(
    application: Router,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    token: &str,
    capability: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if let Some(capability) = capability {
        builder = builder.header("x-novel-task-capability", capability);
    }
    let body = if let Some(payload) = payload {
        builder = builder.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = application
        .author_fixture_request(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn work_task_api_pages_all_active_tasks_and_rejects_invalid_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Many tasks"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = created["project"]["id"].as_str().unwrap();
    let create_uri = format!("/api/projects/{project_id}/tasks");
    for index in 0..31 {
        let (status, _) = call(
            application(&path),
            "POST",
            &create_uri,
            Some(json!({
                "operation": "advisory",
                "inputRevision": 0,
                "threadId": format!("thread-{index}"),
                "target": {"type":"project"},
                "capabilityHash": "a".repeat(64),
                "capabilityExpiresAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 600,
                "authorIntent": "Discuss this story"
            })),
        ).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let list_uri = format!("/api/projects/{project_id}/work-tasks");
    let (status, first) = call(application(&path), "GET", &list_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["activeCount"], 31);
    assert_eq!(first["tasks"].as_array().unwrap().len(), 30);
    let cursor = first["nextCursor"].as_str().unwrap();
    let (status, second) = call(
        application(&path),
        "GET",
        &format!("{list_uri}?cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(second["nextCursor"], Value::Null);
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{list_uri}?cursor=invalid"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn author_search_audit_runs_in_worker_and_can_pause_or_resume() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"Audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/search-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(search_audit_tick(&path).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = search_audit_tick(&path).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert!(done.audit.complete());
    let (status, fetched) =
        call(application(&path), "GET", &format!("{base}/{job_id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["status"], "completed");
    assert_eq!(fetched["verifiedConsistent"], true);
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["id"], job_id);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_entity_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("entity-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"Entity audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/entity-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(entity_audit_tick(&path).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = entity_audit_tick(&path).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_relation_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("relation-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"Relation audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/relation-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    assert_eq!(started["verifiedConsistent"], Value::Null);
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(relation_audit_tick(&path).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = relation_audit_tick(&path).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_knowledge_audit_checks_both_indexes_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("knowledge-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"Knowledge audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/knowledge-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    assert_eq!(started["verifiedConsistent"], Value::Null);
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(knowledge_audit_tick(&path).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = knowledge_audit_tick(&path).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    assert_eq!(latest["job"]["audit"]["assertions"]["publishedBatches"], 0);
    assert_eq!(latest["job"]["audit"]["epistemic"]["publishedBatches"], 0);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_fact_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fact-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"Fact audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/fact-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(fact_audit_tick(&path).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = fact_audit_tick(&path).unwrap().unwrap();
    assert_eq!(
        (done.status.as_str(), done.verified_consistent),
        ("completed", Some(true))
    );
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["audit"]["facts"], 0);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_historical_knowledge_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot-knowledge-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title":"Historical knowledge audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/snapshot-knowledge-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    assert_eq!(started["verifiedConsistent"], Value::Null);
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(snapshot_knowledge_audit_tick(&path).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = snapshot_knowledge_audit_tick(&path).unwrap().unwrap();
    assert_eq!(
        (done.status.as_str(), done.verified_consistent),
        ("completed", Some(true))
    );
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["audit"]["members"], 0);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn candidate_writes_are_task_bound_and_capability_is_revoked_on_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Capability"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let capability = "task-secret-123456789012345678901234567890";
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    let capability_expires_at = std::time::SystemTime::now()
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
            "operation": "generateBrief",
            "inputRevision": 0,
            "threadId": "thread-capability",
            "target": {},
            "capabilityHash": capability_hash,
            "capabilityExpiresAt": capability_expires_at,
            "authorIntent": "Create a brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let candidate_write = json!({
        "payload": {"premise": "memory"},
        "expectedRevision": 0,
        "expectedCandidateRevisionId": null,
        "idempotencyKey": "candidate-brief-1",
        "taskId": task_id,
        "threadId": "thread-capability"
    });
    let (status, artifact) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_write.clone()),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(artifact["revisionId"].is_string());
    let (status, second_checkpoint) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "memory, continued"},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": artifact["revisionId"],
            "idempotencyKey": "candidate-brief-2",
            "taskId": task_id,
            "threadId": "thread-capability"
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second_checkpoint}");
    let task_context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=thread-capability&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, _) = call_as(
        application(&path),
        "GET",
        &task_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let completion_uri = format!("/api/projects/{project_id}/tasks/{task_id}/complete");
    let (status, completed) = call_as(
        application(&path),
        "POST",
        &completion_uri,
        Some(json!({"threadId": "thread-capability", "summary": "done"})),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["outputRefs"].as_array().unwrap().len(), 2);

    let (status, error) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_write),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error["error"]["code"], "task_capability_invalid");

    let next_capability = "next-task-secret-123456789012345678901";
    let next_hash = format!("{:x}", Sha256::digest(next_capability.as_bytes()));
    let (status, next_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "thread-capability-next", "target": {},
            "capabilityHash": next_hash, "capabilityExpiresAt": capability_expires_at,
            "authorIntent": "Revise the existing brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{next_task}");
    assert_eq!(
        next_task["inputManifest"]["expectedCandidateRevisionId"],
        second_checkpoint["revisionId"]
    );
    assert_eq!(
        next_task["inputManifest"]["outputObjectId"],
        "artifact-brief"
    );
    let (status, author_change) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "author changed it"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": second_checkpoint["revisionId"],
            "idempotencyKey": "author-concurrent-brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{author_change}");
    let stale_context_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-capability-next&projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
        next_task["id"].as_str().unwrap()
    );
    let (status, stale_context) = call_as(
        application(&path),
        "GET",
        &stale_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(next_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_context}");
    let (status, stale_task_write) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "unfrozen task write"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": author_change["revisionId"],
            "idempotencyKey": "stale-task-brief",
            "taskId": next_task["id"], "threadId": "thread-capability-next"
        })),
        CANDIDATE_TOKEN,
        Some(next_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_task_write}");
}

#[tokio::test]
async fn artifact_review_accept_undo_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Round trip", "logline": "Atomic story"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = created["project"]["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let candidate_request = json!({
        "payload": {"premise": "memory", "stakes": "identity"},
        "expectedRevision": 0,
        "expectedCandidateRevisionId": null,
        "idempotencyKey": "save-brief-1",
        "linkedImpactIds": []
    });
    let (status, candidate) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, replay) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_request),
    )
    .await;
    assert_eq!(candidate, replay);

    let revision_id = candidate["revisionId"].as_str().unwrap();
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "brief",
            "revisionId": revision_id,
            "proposals": [],
            "selections": {},
            "idempotencyKey": "review-brief-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review["id"].as_str().unwrap();
    let accept_uri = format!("/api/projects/{project_id}/reviews/{review_id}/accept");
    let (status, accepted) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 0,
            "idempotencyKey": "accept-brief-1",
            "selections": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["resultRevision"], 1);

    let commit_id = accepted["commitId"].as_str().unwrap();
    let undo_uri = format!("/api/projects/{project_id}/commits/{commit_id}/undo");
    let (status, undone) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedRevision": 1,
            "idempotencyKey": "undo-brief-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone["resultRevision"], 2);

    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (status, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(workspace["revision"], 2);
    assert!(workspace["artifacts"]["brief"]["accepted"].is_null());
    assert_eq!(
        workspace["artifacts"]["brief"]["candidate"]["id"],
        revision_id
    );
}

#[tokio::test]
async fn chinese_search_uses_selected_revision_and_returns_exact_source_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Search"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, first) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "夜里的港口", "body": "林夕把密函藏进灯塔。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "search-first"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let search_uri = format!("/api/projects/{project_id}/search");
    let (status, found) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%AF%86%E5%87%BD&kind=draft"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["results"].as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found["results"][0]["sourcePath"], "/sceneGroups/0/body");
    assert_eq!(found["results"][0]["sourceRevisionId"], first["revisionId"]);
    assert_eq!(
        found["results"][0]["evidence"],
        json!({
            "quote": "密函", "start": 3, "end": 5
        })
    );
    assert_eq!(found["coverage"]["complete"], true);
    let (_, hidden_metadata) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q={}", first["chapterId"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(hidden_metadata["results"], json!([]));
    let (_, one_character) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%87%BD"),
        None,
    )
    .await;
    assert_eq!(one_character["results"][0]["evidence"]["quote"], "函");
    let (_, title_match) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%B8%AF%E5%8F%A3"),
        None,
    )
    .await;
    assert_eq!(title_match["results"][0]["sourcePath"], "/title");
    let (status, _) = call(
        application(&path),
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload": {"premise": "港城正在失去记忆。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "search-brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, artifact_match) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%A4%B1%E5%8E%BB&kind=brief"),
        None,
    )
    .await;
    assert_eq!(artifact_match["results"][0]["kind"], "brief");
    assert!(
        artifact_match["results"][0]["sourcePath"]
            .as_str()
            .unwrap()
            .ends_with("/premise")
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 2, "title": "第二章", "body": "林夕把另一封信藏起。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "search-second-chapter"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, first_page) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1"),
        None,
    )
    .await;
    assert_eq!(first_page["results"].as_array().unwrap().len(), 1);
    let search_cursor = first_page["nextCursor"].as_str().unwrap();
    let (_, next_page) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1&cursor={search_cursor}"),
        None,
    )
    .await;
    assert_eq!(next_page["results"].as_array().unwrap().len(), 1);
    assert!(next_page["nextCursor"].is_null());
    assert_ne!(
        first_page["results"][0]["id"],
        next_page["results"][0]["id"]
    );
    let (_, other) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Other"})),
    )
    .await;
    let other_id = other["project"]["id"].as_str().unwrap();
    let (_, isolated) = call(
        application(&path),
        "GET",
        &format!("/api/projects/{other_id}/search?q=%E5%AF%86%E5%87%BD"),
        None,
    )
    .await;
    assert_eq!(isolated["results"], json!([]));
    let (status, revised) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterId": first["chapterId"], "chapterNumber": 1,
                "title": "改稿后的港口", "body": "林夕把信纸藏进灯塔。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": first["revisionId"],
            "idempotencyKey": "search-revised"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let (_, stale) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%AF%86%E5%87%BD"),
        None,
    )
    .await;
    assert_eq!(stale["results"], json!([]));
    let (_, current) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E4%BF%A1%E7%BA%B8"),
        None,
    )
    .await;
    assert_eq!(
        current["results"][0]["sourceRevisionId"],
        revised["revisionId"]
    );
    assert_eq!(current["results"][0]["title"], "改稿后的港口");
    let first_snapshot = found["coverage"]["snapshotId"].as_str().unwrap();
    let historical_first_uri =
        format!("{search_uri}?q=%E5%AF%86%E5%87%BD&kind=draft&snapshotId={first_snapshot}");
    let (status, historical_first) =
        call(application(&path), "GET", &historical_first_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historical_first}");
    assert_eq!(
        historical_first["results"][0]["sourceRevisionId"],
        first["revisionId"]
    );
    assert_eq!(historical_first["results"][0]["title"], "夜里的港口");
    assert_eq!(historical_first["coverage"]["snapshotId"], first_snapshot);
    let (_, not_yet_written) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E4%BF%A1%E7%BA%B8&snapshotId={first_snapshot}"),
        None,
    )
    .await;
    assert_eq!(not_yet_written["results"], json!([]));
    let paged_snapshot = first_page["coverage"]["snapshotId"].as_str().unwrap();
    let historical_page_uri =
        format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1&snapshotId={paged_snapshot}");
    let (_, historical_page_one) =
        call(application(&path), "GET", &historical_page_uri, None).await;
    let historical_cursor = historical_page_one["nextCursor"].as_str().unwrap();
    let (status, historical_page_two) = call(
        application(&path),
        "GET",
        &format!("{historical_page_uri}&cursor={historical_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_page_two}");
    assert_eq!(historical_page_two["results"].as_array().unwrap().len(), 1);
    let (status, cross_project) = call(
        application(&path),
        "GET",
        &format!(
            "/api/projects/{other_id}/search?q=%E5%AF%86%E5%87%BD&snapshotId={first_snapshot}"
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{cross_project}");
    let (status, stale_cursor) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1&cursor={search_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_cursor}");
    let (status, changed_query) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%B8%AF%E5%8F%A3&limit=1&cursor={search_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{changed_query}");
}

#[tokio::test]
async fn confirmed_fact_is_visible_in_context_and_undo_removes_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Facts"})),
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
        Some(json!({"title": "Impacts"})),
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
            "proposals": [{"predicate": "lampDurationSeconds", "value": 47, "evidence": "Chapter 1"}],
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
            "selections": {"factDecisions": {fact_id: "confirmed"}}
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

#[tokio::test]
async fn planning_review_seat_uses_frozen_context_and_persists_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Review"})),
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
                "id": "book-1", "kind": "book", "position": 1,
                "title": "The Last Lamp", "purpose": "Test memory", "summary": "A city forgets.",
                "dependencyKeys": [], "certainty": "tentative", "protection": "free"
            }]},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "planning-review-outline-1"
        })),
    )
    .await;
    let start_uri = format!("/api/projects/{project_id}/planning-reviews");
    let (status, run) = call(
        application(&path),
        "POST",
        &start_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": outline["revisionId"],
            "targetNodeId": null, "authorIntent": "Check causality",
            "roles": ["structure", "character"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let run_id = run["id"].as_str().unwrap();
    let capability = "review-secret-123456789012345678901234567890";
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let seat_uri =
        format!("/api/projects/{project_id}/planning-reviews/{run_id}/seats/structure/tasks");
    let (_, workspace) = call(
        application(&path),
        "GET",
        &format!("/api/projects/{project_id}/creative-workspace"),
        None,
    )
    .await;
    let (status, _) = call(application(&path), "POST", &format!("/api/projects/{project_id}/work-tasks/work-{run_id}/budget-authorization"), Some(json!({
        "expectedVersion":0,"expectedProjectRevision":0,"expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":3,"maxTokens":1000,"deadlineAt":expires_at
    }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, task) = call(
        application(&path),
        "POST",
        &seat_uri,
        Some(json!({
            "threadId": "thread-structure", "model": "test",
            "capabilityHash": capability_hash, "capabilityExpiresAt": expires_at
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let task_id = task["id"].as_str().unwrap();
    let query_plan = json!({"question":"规划证据是否充分", "byteBudget":9000, "ranking":{"type":"literal"}, "steps":[
        {"id":"text", "operation":{"type":"text", "query":"港城", "limit":1}},
        {"id":"state", "operation":{"type":"state", "entityId":"linxi", "perspective":"author"}}
    ]});
    let encoded: String = query_plan
        .to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    let context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=thread-structure&projectRevision=0&purpose=planner&budget=24&byteBudget=60000&queryPlan={encoded}"
    );
    let (status, context) = call_as(
        application(&path),
        "GET",
        &context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        context["queryReport"]["policy"]["operation"],
        "reviewPlanningSeat"
    );
    assert_eq!(context["queryReport"]["policy"], context["queryScope"]);
    assert_eq!(
        context["operationContext"]["planningReview"]["role"],
        "structure"
    );
    assert_eq!(
        context["operationContext"]["planningReview"]["checks"],
        json!(["structure:book-1"])
    );
    let report_uri = format!("/api/projects/{project_id}/planning-reviews/{run_id}/reports");
    let (status, report) = call_as(
        application(&path),
        "POST",
        &report_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-structure", "expectedRevision": 0,
            "report": {
                "summary": "Causality is visible.", "strengths": [], "findings": [],
                "coverage": [{"checkId": "structure:book-1", "status": "checked", "reason": "Reviewed"}]
            }
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["status"], "complete");
    let complete_uri = format!("/api/projects/{project_id}/tasks/{task_id}/complete");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &complete_uri,
        Some(json!({"threadId": "thread-structure", "summary": "done"})),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let finalize_uri = format!("/api/projects/{project_id}/planning-reviews/{run_id}/finalize");
    settlement::settle(&path, project_id, task_id);
    let (status, finalized) =
        call(application(&path), "POST", &finalize_uri, Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finalized["status"], "partial");
    assert_eq!(finalized["reports"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn draft_checkpoint_creates_chapter_and_is_visible_in_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Draft"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, draft) = call(
        application(&path),
        "POST",
        &uri,
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "Opening", "body": "The lamp went dark."},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "draft-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (_, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(
        workspace["chapters"][0]["candidate"]["id"],
        draft["revisionId"]
    );
    assert_eq!(
        workspace["chapters"][0]["candidate"]["body"],
        "The lamp went dark."
    );
}

#[tokio::test]
async fn modeling_candidates_follow_exact_draft_revision_and_reject_late_publication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Knowledge"})),
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
    let catalog_ids = second_context["operationContext"]["entityCatalog"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect::<Vec<_>>();
    assert!(catalog_ids.contains(&"letter-sent"));
    assert!(catalog_ids.contains(&"father-linxi"));
    let father = second_context["operationContext"]["entityCatalog"]["entries"]
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
    assert_eq!(writer_context["relevantText"].as_array().unwrap().len(), 1);
    assert_eq!(
        writer_context["operationContext"]["modeling"]
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
    assert_eq!(
        next_context["operationContext"]["workingKnowledge"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        next_context["operationContext"]["workingKnowledge"]["coverage"]["complete"],
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
    assert_eq!(
        confirmed_writer_context["operationContext"]["workingKnowledge"]["entries"][0]["confirmationState"],
        "candidate"
    );
    assert!(confirmed_writer_context["operationContext"]["workingKnowledge"]["entries"][0]["confirmedFactId"].is_null());
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

#[tokio::test]
async fn standalone_chapter_and_working_source_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Working source"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let materialize_uri = format!("/api/projects/{project_id}/chapters/materialize-standalone");
    let (status, materialized) = call(
        application(&path),
        "POST",
        &materialize_uri,
        Some(json!({"title": "Opening", "expectedRevision": 0, "idempotencyKey": "standalone-1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{materialized}");
    let chapter_id = materialized["chapterId"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, draft) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterId": chapter_id, "chapterNumber": 1, "title": "Opening", "body": "First version."},
            "expectedRevision": 1, "expectedCandidateRevisionId": null, "idempotencyKey": "working-draft-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let baseline_uri = format!("/api/projects/{project_id}/working-baselines/draft");
    let (status, baseline) = call(
        application(&path),
        "PUT",
        &baseline_uri,
        Some(json!({"objectId": chapter_id, "revisionId": draft["revisionId"], "expectedSelectionRevision": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(baseline["selectionRevision"], 1);
    let continuation_uri = format!("/api/projects/{project_id}/continuations/draft");
    let (status, continuation) = call(
        application(&path),
        "PUT",
        &continuation_uri,
        Some(json!({"targetId": chapter_id, "workingRevisionId": draft["revisionId"], "authorIntent": "Continue with the storm."})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(continuation["status"], "ready");
    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (_, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(
        workspace["chapters"][0]["working"]["body"],
        "First version."
    );
    assert_eq!(
        workspace["continuationIntents"][0]["authorIntent"],
        "Continue with the storm."
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settlement;
