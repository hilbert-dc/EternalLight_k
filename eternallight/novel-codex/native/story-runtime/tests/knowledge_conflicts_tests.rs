use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR_TOKEN: &str = "conflict-author-token-123456789012345";
const CANDIDATE_TOKEN: &str = "conflict-candidate-token-12345678901";
const CAPABILITY: &str = "conflict-task-capability-123456789012345";

fn application(path: &std::path::Path) -> Router {
    router(task_fixture::state(
        path.to_owned(),
        AUTHOR_TOKEN.into(),
        CANDIDATE_TOKEN.into(),
    ))
}

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    candidate: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri).header(
        "authorization",
        format!(
            "Bearer {}",
            if candidate {
                CANDIDATE_TOKEN
            } else {
                AUTHOR_TOKEN
            }
        ),
    );
    if candidate {
        request = request.header("x-novel-task-capability", CAPABILITY);
    }
    let body = if let Some(payload) = payload {
        request = request.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = application(path)
        .author_fixture_request(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn epistemic_disagreement_is_source_bound_time_scoped_and_frozen_in_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("epistemic-disagreement.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"认知分歧"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let body = "林夕确信门开着。林夕又怀疑门开着。";
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, draft) = call(&path, "POST", &draft_uri, Some(json!({
        "payload":{"chapterNumber":1,"title":"门","body":body},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"epistemic-first"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, batch) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/modeling-batches"),
        Some(json!({
            "objectId":format!("draft-{}",draft["chapterId"].as_str().unwrap()),
            "sourceRevisionId":draft["revisionId"],"expectedRevision":0,"extractorVersion":"test"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path, "POST", &format!("/api/projects/{project_id}/tasks"), Some(json!({
        "operation":"modelKnowledge","inputRevision":0,"threadId":"epistemic-thread",
        "target":{"batchId":batch["id"]},"capabilityExpiresAt":expires,
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"authorIntent":"提取认知"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
    let (status, published) = call(&path, "POST", &format!("/api/projects/{project_id}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()), Some(json!({
        "taskId":task["id"],"threadId":"epistemic-thread","expectedRevision":0,
        "entities":[
            {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":evidence},
            {"id":"door","kind":"proposition","displayName":"门开着","aliases":[],"evidence":evidence}
        ],
        "assertions":[
            {"kind":"epistemic","subjectId":"door","propositionId":"door","predicate":"knows",
                "epistemicOwnerId":"linxi","epistemicState":"known","epistemicTime":1,
                "epistemicUntilExclusive":3,"epistemicUntilEvidence":evidence,"value":true,"evidence":evidence},
            {"kind":"epistemic","subjectId":"door","propositionId":"door","predicate":"suspects",
                "epistemicOwnerId":"linxi","epistemicState":"suspected","epistemicTime":2,
                "value":true,"evidence":evidence}
        ]
    })), true).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let state_uri =
        format!("/api/projects/{project_id}/knowledge/state?perspective=character&ownerId=linxi");
    let (status, disputed) = call(
        &path,
        "GET",
        &format!("{state_uri}&epistemicTime=2"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{disputed}");
    assert_eq!(disputed["findings"].as_array().unwrap().len(), 1);
    assert_eq!(disputed["findings"][0]["type"], "epistemicDisagreement");
    assert_eq!(
        disputed["findings"][0]["propositionRef"]["batchId"],
        batch["id"]
    );
    let (_, later) = call(
        &path,
        "GET",
        &format!("{state_uri}&epistemicTime=3"),
        None,
        false,
    )
    .await;
    assert_eq!(later["findings"], json!([]));
    assert_eq!(later["entries"].as_array().unwrap().len(), 1);
    let (status, saved) = call(&path, "POST", &draft_uri, Some(json!({
        "payload":{"chapterNumber":2,"title":"次日","body":"次日。"},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"epistemic-second"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let project = ProjectId::parse(project_id).unwrap();
    let snapshot = SqliteStoryRepository::open(&path)
        .unwrap()
        .branch_head(
            &project,
            &novel_codex_story_core::BranchId::parse("main").unwrap(),
        )
        .unwrap()
        .unwrap()
        .snapshot_id;
    let (status, revised) = call(&path, "POST", &draft_uri, Some(json!({
        "payload":{"chapterId":draft["chapterId"],"chapterNumber":1,"title":"门","body":"门关着。"},
        "expectedRevision":0,"expectedCandidateRevisionId":draft["revisionId"],"idempotencyKey":"epistemic-revised"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let (_, current) = call(
        &path,
        "GET",
        &format!("{state_uri}&epistemicTime=2"),
        None,
        false,
    )
    .await;
    assert_eq!(current["findings"], json!([]));
    assert_eq!(current["coverage"]["complete"], false);
    let (status, historical) = call(
        &path,
        "GET",
        &format!(
            "{state_uri}&epistemicTime=2&snapshotId={}",
            snapshot.as_str()
        ),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["findings"], disputed["findings"]);
    let (status, _) = call(&path, "GET", &state_uri, None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn relation_graph_expands_claims_by_direction_and_snapshot_without_inference() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("relation-graph.sqlite3");
    let (status, project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Relation graph"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = project["project"]["id"].as_str().unwrap();
    let mut first_draft = Value::Null;
    for (chapter, body, person_id) in [
        (1, "林夕在港口。", "linxi"),
        (2, "张衡也到了港口。", "zhangheng"),
    ] {
        let (status, draft) = call(
            &path,
            "POST",
            &format!("/api/projects/{project_id}/drafts/checkpoints"),
            Some(json!({
                "payload":{"chapterNumber":chapter,"title":format!("第{chapter}章"),"body":body},
                "expectedRevision":0,"expectedCandidateRevisionId":null,
                "idempotencyKey":format!("graph-draft-{chapter}")
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        if chapter == 1 {
            first_draft = draft.clone();
        }
        let (status, batch) = call(
            &path,
            "POST",
            &format!("/api/projects/{project_id}/modeling-batches"),
            Some(json!({
                "objectId":format!("draft-{}", draft["chapterId"].as_str().unwrap()),
                "sourceRevisionId":draft["revisionId"],"expectedRevision":0,
                "extractorVersion":"graph-test"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let batch_id = batch["id"].as_str().unwrap();
        let thread_id = format!("graph-thread-{chapter}");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let (status, task) = call(
            &path,
            "POST",
            &format!("/api/projects/{project_id}/tasks"),
            Some(json!({
                "operation":"modelKnowledge","inputRevision":0,"threadId":thread_id,
                "target":{"batchId":batch_id},
                "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt":expires,"authorIntent":"Ground graph"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
        let (status, published) = call(
            &path,
            "POST",
            &format!("/api/projects/{project_id}/modeling-batches/{batch_id}/publish"),
            Some(json!({
                "taskId":task["id"],"threadId":thread_id,"expectedRevision":0,
                "entities":[
                    {"id":person_id,"kind":"character","displayName":if chapter == 1 {"林夕"} else {"张衡"},
                     "aliases":[],"evidence":evidence},
                    {"id":"port","kind":"location","displayName":"港口","aliases":[],"evidence":evidence},
                    {"id":format!("location-{chapter}"),"kind":"proposition","displayName":body,"aliases":[],"evidence":evidence}
                ],
                "assertions":[{"kind":"relation","subjectId":person_id,
                    "predicate":"locatedAt","relationType":"locatedAt","targetId":"port",
                    "propositionId":format!("location-{chapter}"),
                    "value":true,"validFrom":chapter,"evidence":evidence}]
            })),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{published}");
    }
    let base = format!("/api/projects/{project_id}/knowledge/relations?entityId=linxi");
    let (status, graph) = call(&path, "GET", &format!("{base}&maxHops=2"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{graph}");
    assert_eq!(graph["results"].as_array().unwrap().len(), 2);
    assert_eq!(graph["results"][0]["hop"], 1);
    assert_eq!(graph["results"][1]["hop"], 2);
    assert_eq!(graph["results"][1]["fromId"], "zhangheng");
    assert_eq!(graph["results"][1]["transitive"], false);
    assert_eq!(graph["results"][1]["confirmationState"], "candidate");
    assert_eq!(graph["coverage"]["complete"], true);
    let (_, outgoing) = call(
        &path,
        "GET",
        &format!("{base}&maxHops=2&direction=outgoing"),
        None,
        false,
    )
    .await;
    assert_eq!(outgoing["results"].as_array().unwrap().len(), 1);
    let (_, incoming) = call(
        &path,
        "GET",
        &format!("{base}&direction=incoming"),
        None,
        false,
    )
    .await;
    assert_eq!(incoming["results"], json!([]));
    let (_, filtered) = call(
        &path,
        "GET",
        &format!("{base}&relationType=parentOf"),
        None,
        false,
    )
    .await;
    assert_eq!(filtered["results"], json!([]));
    assert_eq!(filtered["coverage"]["complete"], true);
    let (status, invalid) = call(&path, "GET", &format!("{base}&maxHops=4"), None, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
    let (_, timed) = call(
        &path,
        "GET",
        &format!("{base}&maxHops=2&worldTime=1"),
        None,
        false,
    )
    .await;
    assert_eq!(timed["results"].as_array().unwrap().len(), 1);
    let (_, limited) = call(
        &path,
        "GET",
        &format!("{base}&maxHops=2&limit=1"),
        None,
        false,
    )
    .await;
    assert_eq!(limited["coverage"]["truncatedMatches"], true);
    assert_eq!(limited["coverage"]["complete"], false);
    let (status, forbidden) = call(&path, "GET", &base, None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{forbidden}");

    let (status, frozen) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload":{"premise":"冻结关系证据"},
            "expectedRevision":0,"expectedCandidateRevisionId":null,
            "idempotencyKey":"graph-freeze-history"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{frozen}");
    let (_, context) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"),
        None,
        false,
    ).await;
    let snapshot_id = context["snapshotId"].as_str().unwrap();
    let (status, revised) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({
            "payload":{"chapterId":first_draft["chapterId"],"chapterNumber":1,
                "title":"第一章","body":"林夕离开港口。"},
            "expectedRevision":0,"expectedCandidateRevisionId":first_draft["revisionId"],
            "idempotencyKey":"graph-revised"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let (_, current) = call(&path, "GET", &base, None, false).await;
    assert_eq!(current["results"], json!([]));
    assert_eq!(current["coverage"]["complete"], false);
    let (status, historical) = call(
        &path,
        "GET",
        &format!("{base}&maxHops=2&snapshotId={snapshot_id}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(
        historical["results"].as_array().unwrap().len(),
        2,
        "{historical}"
    );
    assert_eq!(historical["coverage"]["complete"], true);
    let (_, other) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Other"})),
        false,
    )
    .await;
    let other_id = other["project"]["id"].as_str().unwrap();
    let (status, cross_project) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{other_id}/knowledge/relations?entityId=linxi&snapshotId={snapshot_id}"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{cross_project}");
}

#[tokio::test]
async fn writer_joint_search_keeps_future_knowledge_hidden_and_requires_name_disambiguation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("writer-knowledge-search.sqlite3");
    let (_, project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Writer knowledge"})),
        false,
    )
    .await;
    let project_id = project["project"]["id"].as_str().unwrap();
    let drafts_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let batches_uri = format!("/api/projects/{project_id}/modeling-batches");
    let tasks_uri = format!("/api/projects/{project_id}/tasks");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let mut target_chapter_id = String::new();
    for (number, body, entity_id) in [
        (1, "林夕留下证词。", Some("linxi-first")),
        (2, "林夕读到密函。", Some("linxi-second")),
        (3, "港口仍在等待。", None),
        (4, "未来使者泄露结局。", Some("future-messenger")),
    ] {
        let (status, draft) = call(
            &path,
            "POST",
            &drafts_uri,
            Some(json!({
                "payload": {"chapterNumber":number,"title":format!("第{number}章"),"body":body},
                "expectedRevision":0,"expectedCandidateRevisionId":null,
                "idempotencyKey":format!("writer-knowledge-draft-{number}")
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        if number == 3 {
            target_chapter_id = draft["chapterId"].as_str().unwrap().to_owned();
        }
        let Some(entity_id) = entity_id else { continue };
        let (status, batch) = call(
            &path,
            "POST",
            &batches_uri,
            Some(json!({
                "objectId":format!("draft-{}", draft["chapterId"].as_str().unwrap()),
                "sourceRevisionId":draft["revisionId"],"expectedRevision":0,
                "extractorVersion":"writer-search-test"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let batch_id = batch["id"].as_str().unwrap();
        let thread_id = format!("model-thread-{number}");
        let (status, task) = call(
            &path,
            "POST",
            &tasks_uri,
            Some(json!({
                "operation":"modelKnowledge","inputRevision":0,"threadId":thread_id,
                "target":{"batchId":batch_id},
                "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt":expires,"authorIntent":"提取章节知识"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let model_search = format!(
            "/api/projects/{project_id}/tasks/{}/search?threadId={thread_id}&expectedRevision=0&q=%E6%9E%97%E5%A4%95",
            task["id"].as_str().unwrap()
        );
        let (status, scoped) = call(&path, "GET", &model_search, None, true).await;
        assert_eq!(status, StatusCode::OK, "{scoped}");
        assert_eq!(scoped["coverage"]["scope"]["type"], "chapter");
        assert_eq!(scoped["coverage"]["scope"]["chapterNumber"], number);
        let display_name = if number == 4 {
            "未来使者"
        } else {
            "林夕"
        };
        let quote = display_name;
        let (status, published) = call(
            &path,
            "POST",
            &format!("{batches_uri}/{batch_id}/publish"),
            Some(json!({
                "taskId":task["id"],"threadId":thread_id,"expectedRevision":0,
                "entities":[{"id":entity_id,"kind":"character","displayName":display_name,
                    "aliases":[],"evidence":{"quote":quote,"start":0,"end":quote.chars().count()}}],
                "assertions":[{"kind":"assertion","subjectId":entity_id,
                    "predicate":"appeared","value":true,
                    "evidence":{"quote":quote,"start":0,"end":quote.chars().count()}}]
            })),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{published}");
    }
    let (status, writer) = call(&path, "POST", &tasks_uri, Some(json!({
        "operation":"generateChapter","inputRevision":0,"threadId":"writer-knowledge-thread",
        "target":{"type":"chapter","chapterId":target_chapter_id,"chapterNumber":3,"readingChapter":3},
        "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"authorIntent":"续写第三章"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{writer}");
    let uri = format!(
        "/api/projects/{project_id}/tasks/{}/search?threadId=writer-knowledge-thread&expectedRevision=0&q=&entityName=%E6%9E%97%E5%A4%95",
        writer["id"].as_str().unwrap()
    );
    let (status, ambiguous) = call(&path, "GET", &uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{ambiguous}");
    assert_eq!(
        ambiguous["knowledge"]["coverage"]["requiresDisambiguation"],
        true
    );
    assert_eq!(ambiguous["knowledge"]["results"], json!([]));
    assert_eq!(
        ambiguous["knowledge"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!ambiguous.to_string().contains("未来使者"));
    let (status, selected) = call(
        &path,
        "GET",
        &format!("{uri}&entityId=linxi-first"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    assert_eq!(
        selected["knowledge"]["results"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        selected["knowledge"]["results"][0]["evidence"]["quote"],
        "林夕"
    );
    assert_eq!(selected["knowledge"]["coverage"]["complete"], true);
    let (status, forbidden) = call(
        &path,
        "GET",
        &format!("{uri}&entityId=future-messenger"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{forbidden}");
    let (status, future) = call(&path, "GET", &format!("/api/projects/{project_id}/tasks/{}/search?threadId=writer-knowledge-thread&expectedRevision=0&q=&entityName=%E6%9C%AA%E6%9D%A5%E4%BD%BF%E8%80%85", writer["id"].as_str().unwrap()), None, true).await;
    assert_eq!(status, StatusCode::OK, "{future}");
    assert_eq!(future["knowledge"]["candidates"], json!([]));
    assert_eq!(future["knowledge"]["results"], json!([]));
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let id = ProjectId::parse(project_id).unwrap();
    let incremental = repository.audit_entity_index(&id).unwrap();
    assert!(incremental.complete(), "{incremental:?}");
    assert_eq!(incremental.published_batches, 3);
    assert_eq!(repository.rebuild_entity_index(&id).unwrap(), incremental);
    let (_, selected_after_rebuild) = call(
        &path,
        "GET",
        &format!("{uri}&entityId=linxi-first"),
        None,
        true,
    )
    .await;
    assert_eq!(selected_after_rebuild["knowledge"], selected["knowledge"]);
}

#[tokio::test]
async fn joint_query_reports_unmodeled_draft_instead_of_claiming_no_knowledge() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pending-query.sqlite3");
    let (_, project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Pending knowledge"})),
        false,
    )
    .await;
    let project_id = project["project"]["id"].as_str().unwrap();
    let (status, draft) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "第一章", "body": "林夕留下证词。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "pending-query-draft"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (_, text) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/search?q=%E6%9E%97%E5%A4%95"),
        None,
        false,
    )
    .await;
    let snapshot_id = text["coverage"]["snapshotId"].as_str().unwrap();
    let (status, query) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/search?q=%E6%9E%97%E5%A4%95&entityName=%E6%9E%97%E5%A4%95&snapshotId={snapshot_id}"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{query}");
    assert_eq!(query["results"][0]["evidence"]["quote"], "林夕");
    assert_eq!(query["knowledge"]["results"], json!([]));
    assert_eq!(query["knowledge"]["coverage"]["missingSourceCount"], 1);
    assert_eq!(query["knowledge"]["coverage"]["complete"], false);
}

#[tokio::test]
async fn opposing_source_claims_are_explained_without_overwriting_and_stale_source_disappears() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conflicts.sqlite3");
    let (_, project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Conflicts"})),
        false,
    )
    .await;
    let project_id = project["project"]["id"].as_str().unwrap();
    let drafts_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let batches_uri = format!("/api/projects/{project_id}/modeling-batches");
    let tasks_uri = format!("/api/projects/{project_id}/tasks");
    let mut first_draft = Value::Null;
    for (index, (body, quote, end, value)) in [
        ("林夕烧掉信件。", "烧掉信件", 6, true),
        ("林夕没有烧掉信件。", "没有烧掉信件", 8, false),
    ]
    .into_iter()
    .enumerate()
    {
        let chapter = index + 1;
        let (status, draft) = call(&path, "POST", &drafts_uri, Some(json!({
            "payload": {"chapterNumber": chapter, "title": format!("第{chapter}章"), "body": body},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": format!("conflict-draft-{chapter}")
        })), false).await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        if index == 0 {
            first_draft = draft.clone();
        }
        let (_, batch) = call(
            &path,
            "POST",
            &batches_uri,
            Some(json!({
                "objectId": format!("draft-{}", draft["chapterId"].as_str().unwrap()),
                "sourceRevisionId": draft["revisionId"], "expectedRevision": 0,
                "extractorVersion": "conflict-test"
            })),
            false,
        )
        .await;
        let batch_id = batch["id"].as_str().unwrap();
        let thread_id = format!("conflict-thread-{chapter}");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let (_, task) = call(
            &path,
            "POST",
            &tasks_uri,
            Some(json!({
                "operation": "modelKnowledge", "inputRevision": 0, "threadId": thread_id,
                "target": {"batchId": batch_id},
                "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt": expires, "authorIntent": "Ground the source"
            })),
            false,
        )
        .await;
        let publish_uri = format!("{batches_uri}/{batch_id}/publish");
        let (status, published) = call(&path, "POST", &publish_uri, Some(json!({
            "taskId": task["id"], "threadId": thread_id, "expectedRevision": 0,
            "entities": [{"id": "linxi", "kind": "character", "displayName": "林夕", "aliases": [],
                "evidence": {"quote": "林夕", "start": 0, "end": 2}}],
            "assertions": [{"kind": "assertion", "subjectId": "linxi", "predicate": "burnedLetter",
                "value": value, "storyTime": 7,
                "evidence": {"quote": quote, "start": 2, "end": end}}]
        })), true).await;
        assert_eq!(status, StatusCode::OK, "{published}");
    }
    let state_uri =
        format!("/api/projects/{project_id}/knowledge/state?perspective=world&worldTime=7");
    let (status, state) = call(&path, "GET", &state_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{state}");
    assert_eq!(state["entries"].as_array().unwrap().len(), 2);
    assert_eq!(state["findings"].as_array().unwrap().len(), 1);
    assert_eq!(state["findings"][0]["type"], "polarityConflict");
    let mut evidence = [
        state["findings"][0]["leftEvidence"]["quote"]
            .as_str()
            .unwrap(),
        state["findings"][0]["rightEvidence"]["quote"]
            .as_str()
            .unwrap(),
    ];
    evidence.sort_unstable();
    assert_eq!(evidence, ["没有烧掉信件", "烧掉信件"]);
    assert_eq!(
        state["findings"][0]["confirmationState"],
        "candidateConflict"
    );
    assert_eq!(state["coverage"]["complete"], true);
    let (status, _) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload": {"premise": "保存冲突状态"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "freeze-conflict-history"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, context) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"),
        None,
        false,
    )
    .await;
    let frozen_snapshot = context["snapshotId"].as_str().unwrap();
    let (status, revised) = call(
        &path,
        "POST",
        &drafts_uri,
        Some(json!({
            "payload": {"chapterId": first_draft["chapterId"], "chapterNumber": 3,
                "title": "第三章", "body": "林夕藏起信件。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": first_draft["revisionId"],
            "idempotencyKey": "conflict-draft-revised"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let (_, state) = call(&path, "GET", &state_uri, None, false).await;
    assert_eq!(state["findings"], json!([]));
    assert_eq!(state["entries"].as_array().unwrap().len(), 1);
    assert_eq!(state["coverage"]["complete"], false);
    let (status, historic) = call(
        &path,
        "GET",
        &format!("{state_uri}&snapshotId={frozen_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historic}");
    assert_eq!(historic["entries"].as_array().unwrap().len(), 2);
    assert_eq!(historic["findings"].as_array().unwrap().len(), 1);
    assert_eq!(historic["coverage"]["complete"], true);
    let (status, historical_reader) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/knowledge/state?perspective=reader&readingChapter=1&snapshotId={frozen_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_reader}");
    assert_eq!(historical_reader["entries"].as_array().unwrap().len(), 1);
    assert_eq!(historical_reader["entries"][0]["sourceChapter"], 1);
    let (status, historical_narrative) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/knowledge/evolution?dimension=narrative&snapshotId={frozen_snapshot}"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_narrative}");
    assert_eq!(historical_narrative["entries"][0]["readingChapter"], 1);
    assert_eq!(historical_narrative["entries"][1]["readingChapter"], 2);
    let (status, historical_world) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/knowledge/evolution?dimension=world&snapshotId={frozen_snapshot}"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_world}");
    assert_eq!(historical_world["entries"][0]["sourceChapter"], 1);
    assert_eq!(historical_world["entries"][1]["sourceChapter"], 2);
}

#[tokio::test]
async fn author_identity_decisions_reproject_conflicts_and_undo_without_rewriting_sources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("identity.sqlite3");
    let (status, project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Identity decisions"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = project["project"]["id"].as_str().unwrap();
    let drafts_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let batches_uri = format!("/api/projects/{project_id}/modeling-batches");
    let tasks_uri = format!("/api/projects/{project_id}/tasks");
    for (index, (body, entity_id, quote, end, value)) in [
        ("林夕烧掉信件。", "linxi-a", "烧掉信件", 6, true),
        ("林夕没有烧掉信件。", "linxi-b", "没有烧掉信件", 8, false),
    ]
    .into_iter()
    .enumerate()
    {
        let chapter = index + 1;
        let (status, draft) = call(
            &path,
            "POST",
            &drafts_uri,
            Some(json!({
                "payload": {"chapterNumber": chapter, "title": format!("第{chapter}章"), "body": body},
                "expectedRevision": 0, "expectedCandidateRevisionId": null,
                "idempotencyKey": format!("identity-draft-{chapter}")
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        let (status, batch) = call(
            &path,
            "POST",
            &batches_uri,
            Some(json!({
                "objectId": format!("draft-{}", draft["chapterId"].as_str().unwrap()),
                "sourceRevisionId": draft["revisionId"], "expectedRevision": 0,
                "extractorVersion": "identity-test"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let batch_id = batch["id"].as_str().unwrap();
        let thread_id = format!("identity-thread-{chapter}");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let (status, task) = call(
            &path,
            "POST",
            &tasks_uri,
            Some(json!({
                "operation": "modelKnowledge", "inputRevision": 0, "threadId": thread_id,
                "target": {"batchId": batch_id},
                "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt": expires, "authorIntent": "Ground identity"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let (status, published) = call(
            &path,
            "POST",
            &format!("{batches_uri}/{batch_id}/publish"),
            Some(json!({
                "taskId": task["id"], "threadId": thread_id, "expectedRevision": 0,
                "entities": [{"id": entity_id, "kind": "character", "displayName": "林夕",
                    "aliases": if entity_id == "linxi-b" { vec!["小夕"] } else { vec![] },
                    "evidence": {"quote": "林夕", "start": 0, "end": 2}}],
                "assertions": [{"kind": "assertion", "subjectId": entity_id,
                    "predicate": "burnedLetter", "value": value, "storyTime": 7,
                    "evidence": {"quote": quote, "start": 2, "end": end}}]
            })),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{published}");
    }

    let catalog_uri = format!("/api/projects/{project_id}/knowledge/entities");
    let state_uri =
        format!("/api/projects/{project_id}/knowledge/state?perspective=world&worldTime=7");
    let (_, before) = call(&path, "GET", &catalog_uri, None, false).await;
    assert_eq!(
        before["ambiguousNames"][0]["candidateIds"],
        json!(["linxi-a", "linxi-b"])
    );
    let (status, exact_name) = call(
        &path,
        "GET",
        &format!("{catalog_uri}?name=%E6%9E%97%E5%A4%95"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{exact_name}");
    assert_eq!(exact_name["entries"].as_array().unwrap().len(), 2);
    assert_eq!(
        exact_name["entries"][0]["claims"][0]["nameMatchEvidence"],
        "quoted"
    );
    assert_eq!(
        exact_name["ambiguousNames"][0]["candidateIds"],
        json!(["linxi-a", "linxi-b"])
    );
    let (status, alias_match) = call(
        &path,
        "GET",
        &format!("{catalog_uri}?name=%E5%B0%8F%E5%A4%95"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{alias_match}");
    assert_eq!(alias_match["entries"].as_array().unwrap().len(), 1);
    assert_eq!(alias_match["entries"][0]["id"], "linxi-b");
    assert_eq!(
        alias_match["entries"][0]["claims"][0]["nameMatchEvidence"],
        "unverifiedName"
    );
    assert_eq!(
        alias_match["entries"][0]["claims"][0]["evidence"]["quote"],
        "林夕"
    );
    let (_, before_state) = call(&path, "GET", &state_uri, None, false).await;
    assert_eq!(before_state["findings"], json!([]));

    let identity_uri = format!("/api/projects/{project_id}/artifacts/identity");
    let (status, invalid) = call(
        &path,
        "POST",
        &identity_uri,
        Some(json!({
            "payload": {"decisions": [
                {"kind": "same", "fromId": "linxi-a", "toId": "linxi-b"},
                {"kind": "same", "fromId": "linxi-b", "toId": "linxi-a"}]},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "identity-invalid-cycle"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");

    let (status, candidate) = call(
        &path,
        "POST",
        &identity_uri,
        Some(json!({
            "payload": {"decisions": [{"kind": "same", "fromId": "linxi-b", "toId": "linxi-a"}]},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "identity-merge"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{candidate}");
    let context_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, before_accept_context) = call(&path, "GET", &context_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{before_accept_context}");
    let before_accept_snapshot = before_accept_context["snapshotId"].as_str().unwrap();
    let before_accept_uri = format!("{catalog_uri}?snapshotId={before_accept_snapshot}");
    let (status, historical_before) = call(&path, "GET", &before_accept_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{historical_before}");
    assert_eq!(
        historical_before["ambiguousNames"][0]["candidateIds"],
        json!(["linxi-a", "linxi-b"])
    );
    assert_eq!(historical_before["identityRevisionId"], Value::Null);
    let (status, state_before_accept) = call(
        &path,
        "GET",
        &format!("{state_uri}&snapshotId={before_accept_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{state_before_accept}");
    assert_eq!(state_before_accept["entries"].as_array().unwrap().len(), 2);
    assert_eq!(state_before_accept["findings"], json!([]));
    assert_eq!(state_before_accept["identityRevisionId"], Value::Null);
    assert_eq!(state_before_accept["coverage"]["complete"], true);
    let joint_query = format!(
        "/api/projects/{project_id}/search?q=&entityName=%E6%9E%97%E5%A4%95&snapshotId={before_accept_snapshot}"
    );
    let (status, ambiguous_query) = call(&path, "GET", &joint_query, None, false).await;
    assert_eq!(status, StatusCode::OK, "{ambiguous_query}");
    assert_eq!(ambiguous_query["knowledge"]["results"], json!([]));
    assert_eq!(
        ambiguous_query["knowledge"]["coverage"]["requiresDisambiguation"],
        true
    );
    let (status, selected_query) = call(
        &path,
        "GET",
        &format!("{joint_query}&entityId=linxi-b"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{selected_query}");
    assert_eq!(
        selected_query["knowledge"]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        selected_query["knowledge"]["results"][0]["evidence"]["quote"],
        "没有烧掉信件"
    );
    let (status, invalid_selection) = call(
        &path,
        "GET",
        &format!("{joint_query}&entityId=unknown"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid_selection}");
    let (status, unfrozen) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/search?q=&entityName=%E6%9E%97%E5%A4%95"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{unfrozen}");
    let (status, candidate_bypass) = call(&path, "GET", &joint_query, None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{candidate_bypass}");
    let evolution_uri = format!("/api/projects/{project_id}/knowledge/evolution");
    let (_, trace_before_accept) = call(
        &path,
        "GET",
        &format!("{evolution_uri}?dimension=world&snapshotId={before_accept_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(trace_before_accept["identityRevisionId"], Value::Null);
    assert_eq!(trace_before_accept["entries"][1]["targetId"], "linxi-b");
    assert_eq!(
        trace_before_accept["entries"][1]["canonicalTargetId"],
        "linxi-b"
    );
    let (status, reader_before_accept) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/knowledge/state?perspective=reader&readingChapter=1&snapshotId={before_accept_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reader_before_accept}");
    assert_eq!(
        reader_before_accept["entries"].as_array().unwrap().len(),
        1,
        "{reader_before_accept}"
    );
    assert_eq!(reader_before_accept["entries"][0]["sourceChapter"], 1);
    let (status, historical_alias) = call(
        &path,
        "GET",
        &format!("{before_accept_uri}&name=%E5%B0%8F%E5%A4%95"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_alias}");
    assert_eq!(historical_alias["entries"][0]["canonicalId"], "linxi-b");
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, review) = call(
        &path,
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "identity", "revisionId": candidate["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "review-identity-merge"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(&path, "POST",
        &format!("{review_uri}/{}/accept", review["id"].as_str().unwrap()),
        Some(json!({"expectedRevision": 0, "idempotencyKey": "accept-identity-merge", "selections": {}})),
        false).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let accepted_context_uri = context_uri.replace("projectRevision=0", "projectRevision=1");
    let (status, accepted_context) = call(&path, "GET", &accepted_context_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{accepted_context}");
    let merged_snapshot = accepted_context["snapshotId"].as_str().unwrap();
    let merged_history_uri = format!("{catalog_uri}?snapshotId={merged_snapshot}");
    let (status, historical_merged) = call(&path, "GET", &merged_history_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{historical_merged}");
    assert_eq!(historical_merged["ambiguousNames"], json!([]));
    assert_eq!(historical_merged["entries"][1]["canonicalId"], "linxi-a");
    let (status, state_merged) = call(
        &path,
        "GET",
        &format!("{state_uri}&snapshotId={merged_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{state_merged}");
    assert_eq!(state_merged["findings"].as_array().unwrap().len(), 1);
    assert_eq!(state_merged["findings"][0]["subjectId"], "linxi-a");
    let merged_query_uri = format!(
        "/api/projects/{project_id}/search?q=%E4%BF%A1%E4%BB%B6&entityName=%E6%9E%97%E5%A4%95&snapshotId={merged_snapshot}"
    );
    let (status, merged_query) = call(&path, "GET", &merged_query_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{merged_query}");
    assert!(!merged_query["results"].as_array().unwrap().is_empty());
    assert_eq!(merged_query["knowledge"]["selectedCanonicalId"], "linxi-a");
    assert_eq!(merged_query["knowledge"]["coverage"]["complete"], true);
    let mut subjects = merged_query["knowledge"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["assertion"]["subjectId"].as_str().unwrap())
        .collect::<Vec<_>>();
    subjects.sort_unstable();
    assert_eq!(subjects, ["linxi-a", "linxi-b"]);
    let (_, limited_query) = call(
        &path,
        "GET",
        &format!("{merged_query_uri}&limit=1"),
        None,
        false,
    )
    .await;
    assert_eq!(
        limited_query["knowledge"]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        limited_query["knowledge"]["coverage"]["truncatedMatches"],
        true
    );
    assert_eq!(limited_query["knowledge"]["coverage"]["complete"], false);
    let (_, merged_trace) = call(
        &path,
        "GET",
        &format!("{evolution_uri}?dimension=world&snapshotId={merged_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(merged_trace["identityRevisionId"], candidate["revisionId"]);
    assert_eq!(merged_trace["entries"][1]["targetId"], "linxi-b");
    assert_eq!(merged_trace["entries"][1]["canonicalTargetId"], "linxi-a");
    let (_, merged_narrative) = call(
        &path,
        "GET",
        &format!("{evolution_uri}?dimension=narrative&snapshotId={merged_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(
        merged_narrative["entries"][1]["canonicalTargetId"],
        "linxi-a"
    );
    let (_, merged_catalog) = call(&path, "GET", &catalog_uri, None, false).await;
    assert_eq!(merged_catalog["ambiguousNames"], json!([]));
    assert_eq!(merged_catalog["entries"][1]["canonicalId"], "linxi-a");
    assert_eq!(
        merged_catalog["identityRevisionId"],
        candidate["revisionId"]
    );
    let (_, merged_state) = call(&path, "GET", &state_uri, None, false).await;
    assert_eq!(merged_state["findings"].as_array().unwrap().len(), 1);
    assert_eq!(merged_state["findings"][0]["subjectId"], "linxi-a");
    let mut raw_subjects = [
        merged_state["findings"][0]["leftClaimSubjectId"]
            .as_str()
            .unwrap(),
        merged_state["findings"][0]["rightClaimSubjectId"]
            .as_str()
            .unwrap(),
    ];
    raw_subjects.sort_unstable();
    assert_eq!(raw_subjects, ["linxi-a", "linxi-b"]);

    let (status, undone) = call(
        &path,
        "POST",
        &format!(
            "/api/projects/{project_id}/commits/{}/undo",
            accepted["commitId"].as_str().unwrap()
        ),
        Some(json!({"expectedRevision": 1, "idempotencyKey": "undo-identity-merge"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let undone_context_uri = context_uri.replace("projectRevision=0", "projectRevision=2");
    let (status, undone_context) = call(&path, "GET", &undone_context_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{undone_context}");
    let undone_snapshot = undone_context["snapshotId"].as_str().unwrap();
    let (status, historical_undone) = call(
        &path,
        "GET",
        &format!("{catalog_uri}?snapshotId={undone_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_undone}");
    assert_eq!(
        historical_undone["ambiguousNames"][0]["candidateIds"],
        json!(["linxi-a", "linxi-b"])
    );
    assert_eq!(historical_undone["identityRevisionId"], Value::Null);
    let (status, state_undone) = call(
        &path,
        "GET",
        &format!("{state_uri}&snapshotId={undone_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{state_undone}");
    assert_eq!(state_undone["findings"], json!([]));
    let (status, undone_query) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/search?q=&entityName=%E6%9E%97%E5%A4%95&snapshotId={undone_snapshot}"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone_query}");
    assert_eq!(undone_query["knowledge"]["results"], json!([]));
    assert_eq!(
        undone_query["knowledge"]["coverage"]["requiresDisambiguation"],
        true
    );
    let (_, merged_query_again) = call(&path, "GET", &merged_query_uri, None, false).await;
    assert_eq!(
        merged_query_again["knowledge"]["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let (_, undone_trace) = call(
        &path,
        "GET",
        &format!("{evolution_uri}?dimension=world&snapshotId={undone_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(undone_trace["identityRevisionId"], Value::Null);
    assert_eq!(undone_trace["entries"][1]["canonicalTargetId"], "linxi-b");
    let (_, old_trace_again) = call(
        &path,
        "GET",
        &format!("{evolution_uri}?dimension=world&snapshotId={merged_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(
        old_trace_again["entries"][1]["canonicalTargetId"],
        "linxi-a"
    );
    let (_, state_merged_again) = call(
        &path,
        "GET",
        &format!("{state_uri}&snapshotId={merged_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(state_merged_again["findings"].as_array().unwrap().len(), 1);
    let (status, replayed_merge) = call(&path, "GET", &merged_history_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{replayed_merge}");
    assert_eq!(replayed_merge["entries"][1]["canonicalId"], "linxi-a");
    let (_, restored_catalog) = call(&path, "GET", &catalog_uri, None, false).await;
    assert_eq!(
        restored_catalog["ambiguousNames"][0]["authorDecidedDistinct"],
        false
    );
    assert_eq!(restored_catalog["identityRevisionId"], Value::Null);
    let (_, restored_state) = call(&path, "GET", &state_uri, None, false).await;
    assert_eq!(restored_state["findings"], json!([]));

    let (status, distinct) = call(&path, "POST", &identity_uri, Some(json!({
        "payload": {"decisions": [{"kind": "distinct", "leftId": "linxi-a", "rightId": "linxi-b"}]},
        "expectedRevision": 2, "expectedCandidateRevisionId": candidate["revisionId"],
        "idempotencyKey": "identity-distinct"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{distinct}");
    let (status, distinct_review) = call(
        &path,
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "identity", "revisionId": distinct["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "review-identity-distinct"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{distinct_review}");
    let (status, distinct_accepted) = call(&path, "POST",
        &format!("{review_uri}/{}/accept", distinct_review["id"].as_str().unwrap()),
        Some(json!({"expectedRevision": 2, "idempotencyKey": "accept-identity-distinct", "selections": {}})),
        false).await;
    assert_eq!(status, StatusCode::OK, "{distinct_accepted}");
    let (_, distinguished) = call(&path, "GET", &catalog_uri, None, false).await;
    assert_eq!(
        distinguished["ambiguousNames"][0]["authorDecidedDistinct"],
        true
    );
    assert_eq!(
        distinguished["ambiguousNames"][0]["canonicalIds"],
        json!(["linxi-a", "linxi-b"])
    );
    let distinct_context_uri = context_uri.replace("projectRevision=0", "projectRevision=3");
    let (status, distinct_context) = call(&path, "GET", &distinct_context_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{distinct_context}");
    let distinct_snapshot = distinct_context["snapshotId"].as_str().unwrap();
    let (status, historical_distinct) = call(
        &path,
        "GET",
        &format!("{catalog_uri}?snapshotId={distinct_snapshot}&name=%E6%9E%97%E5%A4%95"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_distinct}");
    assert_eq!(
        historical_distinct["ambiguousNames"][0]["authorDecidedDistinct"],
        true
    );
    let (_, other_project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Unrelated"})),
        false,
    )
    .await;
    let other_id = other_project["project"]["id"].as_str().unwrap();
    let (status, _) = call(
        &path,
        "GET",
        &format!("/api/projects/{other_id}/knowledge/entities?snapshotId={distinct_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &path,
        "GET",
        &format!("/api/projects/{other_id}/knowledge/state?perspective=world&snapshotId={distinct_snapshot}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, unknown) = call(
        &path,
        "POST",
        &identity_uri,
        Some(json!({
            "payload": {"decisions": [{"kind": "same", "fromId": "linxi-b", "toId": "unknown"}]},
            "expectedRevision": 3, "expectedCandidateRevisionId": distinct["revisionId"],
            "idempotencyKey": "identity-unknown"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{unknown}");
    let (status, unknown_review) = call(
        &path,
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "identity", "revisionId": unknown["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "review-identity-unknown"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{unknown_review}");
    let (status, rejected) = call(&path, "POST",
        &format!("{review_uri}/{}/accept", unknown_review["id"].as_str().unwrap()),
        Some(json!({"expectedRevision": 3, "idempotencyKey": "accept-identity-unknown", "selections": {}})),
        false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let (_, unchanged) = call(&path, "GET", &catalog_uri, None, false).await;
    assert_eq!(unchanged["identityRevisionId"], distinct["revisionId"]);
    assert_eq!(
        unchanged["ambiguousNames"][0]["authorDecidedDistinct"],
        true
    );
    let (status, rejected_review) = call(
        &path,
        "POST",
        &format!(
            "{review_uri}/{}/reject-identity",
            unknown_review["id"].as_str().unwrap()
        ),
        Some(json!({"expectedRevision": 3})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected_review}");
    assert_eq!(rejected_review["status"], "rejected");
    let (_, after_rejection) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(after_rejection["revision"], 3);
    assert_eq!(
        after_rejection["artifacts"]["identity"]["accepted"]["id"],
        distinct["revisionId"]
    );
    assert!(
        after_rejection["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .any(|review| review["id"] == unknown_review["id"] && review["status"] == "rejected")
    );
}

#[tokio::test]
async fn world_evolution_orders_backstory_before_later_chapters_and_pages_by_world_time() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world-evolution.sqlite3");
    let (status, project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Backstory"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = project["project"]["id"].as_str().unwrap();
    let drafts_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let batches_uri = format!("/api/projects/{project_id}/modeling-batches");
    let tasks_uri = format!("/api/projects/{project_id}/tasks");
    for (chapter, body, story_time) in [(1, "林夕在今夜点灯。", 20), (2, "林夕在过去点灯。", 5)]
    {
        let (status, draft) = call(
            &path,
            "POST",
            &drafts_uri,
            Some(json!({
                "payload": {"chapterNumber": chapter, "title": format!("第{chapter}章"), "body": body},
                "expectedRevision": 0, "expectedCandidateRevisionId": null,
                "idempotencyKey": format!("world-draft-{chapter}")
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        let (status, batch) = call(
            &path,
            "POST",
            &batches_uri,
            Some(json!({
                "objectId": format!("draft-{}", draft["chapterId"].as_str().unwrap()),
                "sourceRevisionId": draft["revisionId"], "expectedRevision": 0,
                "extractorVersion": "world-time-test"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let batch_id = batch["id"].as_str().unwrap();
        let thread_id = format!("world-thread-{chapter}");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let (status, task) = call(
            &path,
            "POST",
            &tasks_uri,
            Some(json!({
                "operation": "modelKnowledge", "inputRevision": 0, "threadId": thread_id,
                "target": {"batchId": batch_id},
                "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt": expires, "authorIntent": "Ground world time"
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let (status, published) = call(
            &path,
            "POST",
            &format!("{batches_uri}/{batch_id}/publish"),
            Some(json!({
                "taskId": task["id"], "threadId": thread_id, "expectedRevision": 0,
                "entities": [{"id": "linxi", "kind": "character", "displayName": "林夕",
                    "aliases": [], "evidence": {"quote": "林夕", "start": 0, "end": 2}},
                    {"id":format!("lit-lamp-{story_time}"),"kind":"event","displayName":"点灯","aliases":[],"evidence":{"quote":"点灯","start":5,"end":7}}],
                "assertions": [{"kind": "event", "subjectId": format!("lit-lamp-{story_time}"), "predicate": "litLamp",
                    "value": {"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null}, "storyTime": story_time,
                    "evidence": {"quote": "点灯", "start": 5, "end": 7}}]
            })),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{published}");
    }
    let uri = format!(
        "/api/projects/{project_id}/knowledge/evolution?dimension=world&projectRevision=0&limit=1"
    );
    let (status, first) = call(&path, "GET", &uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["entries"][0]["worldTime"], 5);
    assert_eq!(first["entries"][0]["sourceChapter"], 2);
    assert_eq!(first["coverage"]["ordering"], "storyTimeThenSource");
    assert_eq!(first["coverage"]["chronologyComplete"], true);
    assert_eq!(first["coverage"]["complete"], false);
    let cursor = first["nextCursor"].as_str().unwrap();
    let (status, second) = call(&path, "GET", &format!("{uri}&cursor={cursor}"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["entries"][0]["worldTime"], 20);
    assert_eq!(second["entries"][0]["sourceChapter"], 1);
    assert_eq!(second["coverage"]["complete"], true);
    assert_eq!(second["nextCursor"], Value::Null);
    let forged_cursor = cursor.replace(":5:", ":6:");
    let (status, forged) = call(
        &path,
        "GET",
        &format!("{uri}&cursor={forged_cursor}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{forged}");
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

use novel_codex_story_core::{EntityIndexIntegrityRepository, ProjectRevisionRepository};
