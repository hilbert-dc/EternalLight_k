use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
const AUTHOR: &str = "writer-baseline-author-123456789012345";
const CANDIDATE: &str = "writer-baseline-candidate-123456789012";
const CAPABILITY: &str = "writer-baseline-capability-12345678901";
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {actor}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn writer(app: &Router, base: &str, scope: Value, thread: &str) -> Value {
    let (_, workspace) = call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status,task) = call(app,"POST",&format!("{base}/tasks"),AUTHOR,json!({"operation":"generateChapter",
        "target":{"type":"chapter","chapterId":workspace["chapters"][0]["id"],"chapterNumber":1,"readingChapter":1,"sceneOrder":1,"povCharacterId":"person","storyTime":1,"scope":scope},
        "inputRevision":workspace["revision"],"threadId":thread,"authorIntent":"修订整章，保留原文中未涉及的故事。",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires})).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    task
}
async fn query(
    app: &Router,
    base: &str,
    task: &Value,
    kind: &str,
    cursor: Value,
) -> (StatusCode, Value) {
    call(app,"POST",&format!("{base}/tasks/{}/query",task["id"].as_str().unwrap()),CANDIDATE,
        json!({"threadId":task["threadId"],"expectedRevision":task["inputRevision"],"plan":{"question":"完整读取我的冻结原稿",
        "byteBudget":9000,"ranking":{"type":"literal"},"steps":[{"id":"baseline","operation":{"type":kind,"cursor":cursor,"maxChars":400,"limit":2}}]}})).await
}
#[tokio::test]
async fn whole_writer_pages_frozen_baseline_without_pov_world_access_or_latest_substitution() {
    let directory = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        directory.path().join("baseline.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, project) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"冻结原稿"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    let body = format!("{}末行", "海潮🌊\"\\\n".repeat(650));
    let payload = json!({"chapterId":"chapter-one","chapterNumber":1,"title":"原稿","body":body,
        "sceneGroups":[{"id":"original-group","sceneId":null,"body":body,"locked":false}],"completeness":"chapterComplete"});
    let (status,saved)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,
        json!({"payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"original"})).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let task = writer(&app, &base, json!({"type":"wholeChapter"}), "writer").await;
    let context_uri = format!(
        "{base}/context?taskId={}&threadId=writer&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&storyTime=1&povCharacterId=person&budget=24&byteBudget=14000",
        task["id"].as_str().unwrap()
    );
    let (status, context) = call(&app, "GET", &context_uri, CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["operationContext"]["requiredSourceReads"]["sourceRevisionId"],
        saved["revisionId"]
    );
    assert_eq!(
        context["operationContext"]["requiredSourceReads"]["plan"]["steps"][0]["operation"]["type"],
        "writerBaselinePassages"
    );
    assert!(!context.to_string().contains(body.as_str()));
    let (_, ordinary) = query(&app, &base, &task, "passages", Value::Null).await;
    assert_eq!(ordinary["steps"][0]["results"], json!([]));
    assert_eq!(ordinary["steps"][0]["coverage"]["povTextUnverified"], true);
    let (status, first) = query(&app, &base, &task, "writerBaselinePassages", Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let mut cursor = first["steps"][0]["nextCursor"].clone();
    assert!(cursor.is_string());
    let original_cursor = cursor.clone();
    let mut replacement = payload.clone();
    replacement["body"] = json!("新的正文。");
    replacement["sceneGroups"][0]["body"] = json!("新的正文。");
    let (status,checkpoint)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,
        json!({"taskId":task["id"],"threadId":"writer","payload":replacement,"expectedRevision":0,"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"checkpoint"})).await;
    assert_eq!(status, StatusCode::CREATED, "{checkpoint}");
    let mut covered = vec![false; body.chars().count()];
    let mut page = first;
    loop {
        assert_eq!(page["coverage"]["responseTruncated"], false);
        assert_eq!(page["steps"][0]["coverage"]["pageComplete"], true);
        for passage in page["steps"][0]["results"].as_array().unwrap() {
            let evidence = &page["evidence"][passage["evidenceRef"].as_str().unwrap()];
            assert_eq!(evidence["sourceRevisionId"], saved["revisionId"]);
            let start = evidence["start"].as_u64().unwrap() as usize;
            let end = evidence["end"].as_u64().unwrap() as usize;
            assert_eq!(
                evidence["quote"],
                body.chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>()
            );
            covered[start..end].fill(/*value*/ true);
        }
        if cursor.is_null() {
            break;
        }
        let (status, next) = query(&app, &base, &task, "writerBaselinePassages", cursor).await;
        assert_eq!(status, StatusCode::OK, "{next}");
        cursor = next["steps"][0]["nextCursor"].clone();
        page = next;
    }
    assert!(covered.iter().all(|read| *read));
    let (_, after) = call(&app, "GET", &context_uri, CANDIDATE, Value::Null).await;
    assert_eq!(
        after["operationContext"]["requiredSourceReads"]["sourceRevisionId"],
        saved["revisionId"]
    );
    let foreign = writer(&app, &base, json!({"type":"wholeChapter"}), "other-writer").await;
    assert_eq!(
        query(
            &app,
            &base,
            &foreign,
            "writerBaselinePassages",
            original_cursor
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let selected=writer(&app,&base,json!({"type":"selectedScenes","sourceRevisionId":checkpoint["revisionId"],"groupIds":["original-group"]}),"selected").await;
    assert_eq!(
        query(
            &app,
            &base,
            &selected,
            "writerBaselinePassages",
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn baseline_source_is_frozen_author_selection_and_empty_first_write_is_not_a_baseline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection.sqlite3");
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, project) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"原稿来源"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    let (status, chapter) = call(
        &app,
        "POST",
        &format!("{base}/chapters/materialize-standalone"),
        AUTHOR,
        json!({"title":"空章","expectedRevision":0,"idempotencyKey":"empty"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chapter}");
    let chapter_id = chapter["chapterId"].as_str().unwrap();
    let first = writer(&app, &base, json!({"type":"wholeChapter"}), "empty-writer").await;
    assert_eq!(
        query(&app, &base, &first, "writerBaselinePassages", Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let payload = |body: &str| {
        json!({"chapterId":chapter_id,"chapterNumber":1,"title":"来源","body":body,
        "sceneGroups":[{"id":"source","sceneId":null,"body":body,"locked":false}],"completeness":"chapterComplete"})
    };
    let (status,first_save)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,
        json!({"taskId":first["id"],"threadId":first["threadId"],"payload":payload("首次正文"),"expectedRevision":chapter["resultRevision"],"expectedCandidateRevisionId":null,"idempotencyKey":"first-save"})).await;
    assert_eq!(status, StatusCode::CREATED, "{first_save}");
    assert_eq!(
        query(&app, &base, &first, "writerBaselinePassages", Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status,original)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,
        json!({"payload":payload("已采用原稿"),"expectedRevision":chapter["resultRevision"],"expectedCandidateRevisionId":first_save["revisionId"],"idempotencyKey":"original"})).await;
    assert_eq!(status, StatusCode::CREATED, "{original}");
    // Seed the valid accepted-only head shape; exercise all reads through the production HTTP entry.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE story_object_heads_v2 SET accepted_revision_id=candidate_revision_id,candidate_revision_id=NULL WHERE object_id=?1",[format!("draft-{chapter_id}")]).unwrap();
    let accepted = writer(
        &app,
        &base,
        json!({"type":"wholeChapter"}),
        "accepted-writer",
    )
    .await;
    assert_eq!(
        accepted["inputManifest"]["expectedCandidateRevisionId"],
        Value::Null
    );
    let (status, page) = query(
        &app,
        &base,
        &accepted,
        "writerBaselinePassages",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let evidence = &page["evidence"][page["steps"][0]["results"][0]["evidenceRef"]
        .as_str()
        .unwrap()];
    assert_eq!(evidence["sourceRevisionId"], original["revisionId"]);
    let (status,newer)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,
        json!({"payload":payload("较新候选"),"expectedRevision":chapter["resultRevision"],"expectedCandidateRevisionId":null,"idempotencyKey":"newer"})).await;
    assert_eq!(status, StatusCode::CREATED, "{newer}");
    let (status,latest)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,
        json!({"payload":payload("尚未选用的最新候选"),"expectedRevision":chapter["resultRevision"],"expectedCandidateRevisionId":newer["revisionId"],"idempotencyKey":"latest"})).await;
    assert_eq!(status, StatusCode::CREATED, "{latest}");
    let (status,selected)=call(&app,"PUT",&format!("{base}/working-baselines/draft"),AUTHOR,
        json!({"objectId":chapter_id,"revisionId":newer["revisionId"],"expectedSelectionRevision":0})).await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    let working = writer(
        &app,
        &base,
        json!({"type":"wholeChapter"}),
        "working-writer",
    )
    .await;
    let (status, page) = query(&app, &base, &working, "writerBaselinePassages", Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let evidence = &page["evidence"][page["steps"][0]["results"][0]["evidenceRef"]
        .as_str()
        .unwrap()];
    assert_eq!(evidence["sourceRevisionId"], newer["revisionId"]);
    assert_eq!(evidence["quote"], "较新候选");
    let (status,own)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,
        json!({"taskId":working["id"],"threadId":working["threadId"],"payload":payload("本任务改写保存"),
        "expectedRevision":chapter["resultRevision"],"expectedCandidateRevisionId":latest["revisionId"],"idempotencyKey":"working-checkpoint"})).await;
    assert_eq!(status, StatusCode::CREATED, "{own}");
    let (status, page) = query(&app, &base, &working, "writerBaselinePassages", Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let evidence = &page["evidence"][page["steps"][0]["results"][0]["evidenceRef"]
        .as_str()
        .unwrap()];
    assert_eq!(evidence["sourceRevisionId"], newer["revisionId"]);
    assert_eq!(evidence["quote"], "较新候选");
}
