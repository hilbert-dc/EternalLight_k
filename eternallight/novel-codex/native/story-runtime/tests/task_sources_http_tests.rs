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
const AUTHOR: &str = "planning-page-author-123456789012345";
const CANDIDATE: &str = "planning-page-candidate-123456789012";
const CAP: &str = "planning-page-capability-123456789012";
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
                .header("x-novel-task-capability", CAP)
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
    // Axum's typed-query rejection is text, while repository responses are JSON.
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{method} {uri}: {error}; {}",
            String::from_utf8_lossy(&bytes)
        );
        json!({"queryRejection":String::from_utf8_lossy(&bytes)})
    });
    (status, value)
}
async fn task(app: &Router, base: &str, operation: &str, thread: &str, target: Value) -> Value {
    let (_, ws) = call(
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
    let (status,value)=call(app,"POST",&format!("{base}/tasks"),AUTHOR,json!({"operation":operation,
        "target":target,"inputRevision":ws["revision"],"threadId":thread,"authorIntent":"依据已保存内容讨论第1章的改稿方向，暂不修改。",
        "capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":expires})).await;
    assert_eq!(status, StatusCode::CREATED, "{operation}: {value}");
    value
}
async fn context(app: &Router, base: &str, task: &Value) -> Value {
    let (status, value) = call(
        app,
        "GET",
        &format!(
            "{base}/context?taskId={}&threadId={}&projectRevision={}&purpose={}&byteBudget=60000",
            task["id"].as_str().unwrap(),
            task["threadId"].as_str().unwrap(),
            task["inputRevision"],
            task["purpose"].as_str().unwrap()
        ),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":json!({"content":[{"type":"text","text":value.to_string()}]}).to_string()});
    assert!(
        eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
            .unwrap()
            .as_u64()
            .unwrap()
            <= 9000
    );
    assert_eq!(value["coverage"]["complete"], false);
    value
}
async fn page(
    app: &Router,
    base: &str,
    task: &Value,
    source: &Value,
    cursor: &Value,
) -> (StatusCode, Value) {
    let suffix = cursor
        .as_str()
        .map(|cursor| format!("&cursor={cursor}"))
        .unwrap_or_default();
    call(
        app,
        "GET",
        &format!(
            "{base}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{suffix}",
            task["id"].as_str().unwrap(),
            task["threadId"].as_str().unwrap(),
            task["inputRevision"],
            source["sourceId"].as_str().unwrap()
        ),
        CANDIDATE,
        Value::Null,
    )
    .await
}
async fn read(app: &Router, base: &str, task: &Value, entry: &Value) -> Value {
    let mut cursor = Value::Null;
    let mut text = String::new();
    loop {
        let (status, value) = page(app, base, task, entry, &cursor).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["contentHash"], entry["contentHash"]);
        assert_eq!(value["byteRange"]["start"], text.len());
        let part = value["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        assert!(value.to_string().len() <= 6500);
        text.push_str(part);
        assert_eq!(value["byteRange"]["end"], text.len());
        cursor = value["nextCursor"].clone();
        if cursor.is_null() {
            assert_eq!(value["endOfInputFromCursor"], true);
            assert_eq!(value["byteRange"]["total"], text.len());
            break;
        }
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        entry["contentHash"].as_str().unwrap()
    );
    serde_json::from_str(&text).unwrap()
}
fn entry(context: &Value, path: &str) -> Value {
    context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == path)
        .unwrap()
        .clone()
}
#[tokio::test]
async fn task_sources_preserve_visible_originals_after_own_checkpoint_without_widening_pov() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("source.sqlite3");
    let app = router(task_fixture::state(
        db.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, project) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"按需读取"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    let outline = json!({"rootNodeId":"book","nodes":(0..=10).map(|n|json!({"id":format!("node-{n}"),"kind":if n==0{"book"}else{"volume"},"position":n,"title":"卷","purpose":"贡献","summary":"因果🙂\"\\\n".repeat(120),"dependencyKeys":[],"certainty":"tentative","protection":"free"})).collect::<Vec<_>>()});
    let (_,saved)=call(&app,"POST",&format!("{base}/artifacts/outline"),AUTHOR,json!({"payload":outline,"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"})).await;
    assert!(saved["revisionId"].is_string(), "{saved}");
    let body = "海潮原文🙂\"\\\n".repeat(700);
    assert!(body.chars().count() > 3000);
    let mut first = Value::Null;
    for n in 1..=3 {
        let payload = json!({"chapterId":format!("chapter-{n}"),"chapterNumber":n,"title":"原题","body":format!("{}\n\n---\n\n锁定原文",body.trim()),"completeness":"chapterComplete","sceneGroups":[{"id":"editable","sceneId":null,"body":body,"locked":false},{"id":"locked","sceneId":null,"body":"锁定原文","locked":true}]});
        let (status,saved)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":format!("draft-{n}")})).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        if n == 1 {
            first = saved;
        }
    }
    for operation in [
        "advisory",
        "generateBrief",
        "generateOutline",
        "generatePlan",
        "generatePropertyRules",
    ] {
        let current = task(
            &app,
            &base,
            operation,
            operation,
            json!({"type":if operation=="advisory"{"project"}else{"artifact"}}),
        )
        .await;
        let pack = context(&app, &base, &current).await;
        assert!(!pack.to_string().contains(&body));
        let original = read(&app, &base, &current, &entry(&pack, "/relevantText")).await;
        assert_eq!(original.as_array().unwrap().len(), 3);
        assert_eq!(original[0]["content"]["sceneGroups"][1]["body"], "锁定原文");
        assert_eq!(
            read(
                &app,
                &base,
                &current,
                &entry(&pack, "/narrativeGoals/outline")
            )
            .await["nodes"],
            outline["nodes"]
        );
    }
    let target = json!({"type":"chapter","chapterId":"chapter-1","chapterNumber":1,"readingChapter":1,"sceneOrder":1,"povCharacterId":"person","storyTime":1,"scope":{"type":"selectedScenes","sourceRevisionId":first["revisionId"],"groupIds":["editable"]}});
    let writer = task(&app, &base, "generateChapter", "writer", target).await;
    let pack = context(&app, &base, &writer).await;
    let source = entry(&pack, "/relevantText");
    let original = read(&app, &base, &writer, &source).await;
    assert_eq!(original.as_array().unwrap().len(), 1);
    assert_eq!(original[0]["content"]["sceneGroups"][1]["body"], "锁定原文");
    let (_, first_page) = page(&app, &base, &writer, &source, &Value::Null).await;
    assert!(first_page["nextCursor"].is_string());
    let sibling = task(
        &app,
        &base,
        "generateChapter",
        "other-writer",
        writer["target"].clone(),
    )
    .await;
    let (status, rejected) = page(&app, &base, &sibling, &source, &first_page["nextCursor"]).await;
    assert_eq!(
        (status, rejected["error"]["message"].clone()),
        (StatusCode::BAD_REQUEST, json!("TaskOutputForbidden"))
    );
    let mut next = original[0]["content"].clone();
    next["sceneGroups"][0]["body"] = json!("修改后原组");
    next["body"] = json!("修改后原组\n\n---\n\n锁定原文");
    next["completeness"] = json!("chapterComplete");
    let (status,receipt)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,json!({"taskId":writer["id"],"threadId":"writer","payload":next,"expectedRevision":0,"expectedCandidateRevisionId":first["revisionId"],"idempotencyKey":"own-checkpoint"})).await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    assert_eq!(read(&app, &base, &writer, &source).await, original);
    assert_eq!(
        entry(&context(&app, &base, &writer).await, "/relevantText"),
        source
    );
    let (status,query)=call(&app,"POST",&format!("{base}/tasks/{}/query",writer["id"].as_str().unwrap()),CANDIDATE,json!({"threadId":"writer","expectedRevision":0,"plan":{"question":"读取前章世界事实","byteBudget":9000,"ranking":{"type":"literal"},"steps":[{"id":"prior","operation":{"type":"passages","cursor":null,"maxChars":400,"limit":2}}]}})).await;
    assert_eq!(status, StatusCode::OK, "{query}");
    assert_eq!(query["steps"][0]["results"], json!([]));
    assert_eq!(query["steps"][0]["findings"], json!([]));
    assert_eq!(query["steps"][0]["coverage"]["povTextUnverified"], true);
    assert_eq!(query["steps"][0]["coverage"]["complete"], false);
}

#[tokio::test]
async fn bound_rolling_plan_sources_keep_all_twenty_five_scene_links() {
    let temp = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        temp.path().join("scenes.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"全部场景按需读取"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let scenes=(0..25).map(|index|json!({"id":format!("scene-{index}"),"goal":format!("第{index}场因果"),"beats":["推进"],"expectedEffects":[]})).collect::<Vec<_>>();
    let (status,plan)=call(&app,"POST",&format!("{base}/artifacts/plan"),AUTHOR,json!({"payload":{"chapters":[{"chapterPlanId":"planned","chapterNumber":1,"goal":"全部衔接","scenes":scenes}]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"plan"})).await;
    assert_eq!(status, StatusCode::CREATED, "{plan}");
    let (status,review)=call(&app,"POST",&format!("{base}/reviews"),AUTHOR,json!({"artifactKind":"plan","revisionId":plan["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"review"})).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(
        &app,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        AUTHOR,
        json!({"expectedRevision":0,"selections":{},"idempotencyKey":"accept"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (_, ws) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    let (status,chapter)=call(&app,"POST",&format!("{base}/chapters/materialize-from-plan"),AUTHOR,json!({"chapterPlanId":"planned","expectedRevision":ws["revision"],"idempotencyKey":"chapter"})).await;
    assert_eq!(status, StatusCode::OK, "{chapter}");
    let (_, ws) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    let groups=(0..25).map(|index|json!({"id":format!("group-{index}"),"sceneId":format!("scene-{index}"),"body":format!("第{index}场实际正文"),"locked":index==24})).collect::<Vec<_>>();
    let body = groups
        .iter()
        .map(|group| group["body"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    let (status,saved)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"expectedRevision":ws["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"draft","payload":{"chapterId":chapter["chapterId"],"chapterNumber":1,"title":"全部场景","body":body,"sceneGroups":groups,"completeness":"chapterComplete","sourcePlanRefs":{"chapterPlanId":"planned","planRevisionId":plan["revisionId"],"localOnly":false,"lineageStatus":"current","outlineRevisionId":null}}})).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let task = task(
        &app,
        &base,
        "generatePlan",
        "rolling",
        json!({"type":"artifact","planFromChapter":2}),
    )
    .await;
    let pack = context(&app, &base, &task).await;
    let links = read(
        &app,
        &base,
        &task,
        &entry(&pack, "/operationContext/scenePlanDraftLinks"),
    )
    .await;
    assert_eq!(links.as_array().unwrap().len(), 25);
    assert_eq!(
        links
            .as_array()
            .unwrap()
            .iter()
            .map(|link| link["sceneId"].clone())
            .collect::<Vec<_>>(),
        scenes
            .iter()
            .map(|scene| scene["id"].clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        read(&app, &base, &task, &entry(&pack, "/relevantText")).await[0]["content"]["sceneGroups"],
        json!(groups)
    );
}
