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
fn assert_source_recovery(rejected: &Value, task: &Value, code: &str) {
    assert_eq!(rejected["error"]["code"], code);
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("先调用 read_context 获取本轮来源目录")
    );
    assert_eq!(
        rejected["error"]["details"],
        json!({"taskId":task["id"],"retryable":true,"sourceIsolation":"currentTaskOnly"})
    );
    assert_eq!(
        rejected["error"]["recovery"],
        json!({"tool":"read_context","arguments":{
        "expectedRevision":task["inputRevision"]
    },"discardPreviousSourceIdAndCursor":true})
    );
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
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"按需读取"}),
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
    let index_uri = format!(
        "{base}/tasks/{}/task-source-content?threadId=writer&expectedRevision={}&sourceId={}&view=index",
        writer["id"].as_str().unwrap(),
        writer["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let (code, frozen_index) = call(&app, "GET", &index_uri, CANDIDATE, Value::Null).await;
    assert_eq!(code, StatusCode::OK, "{frozen_index}");
    assert_eq!(frozen_index["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        frozen_index["items"][0]["identity"]["chapterId"],
        "chapter-1"
    );
    assert!(!frozen_index.to_string().contains("chapter-2"));
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
    assert_eq!(status, StatusCode::CONFLICT);
    assert_source_recovery(&rejected, &sibling, "task_source_catalog_required");
    for view in ["", "&view=index"] {
        let uri = format!(
            "{base}/tasks/{}/task-source-content?threadId=other-writer&expectedRevision={}&sourceId={}{view}",
            sibling["id"].as_str().unwrap(),
            sibling["inputRevision"],
            source["sourceId"].as_str().unwrap()
        );
        let (status, parsed) = call(&app, "GET", &uri, CANDIDATE, Value::Null).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(parsed, rejected);
    }
    let sibling_context = context(&app, &base, &sibling).await;
    let sibling_source = entry(&sibling_context, "/relevantText");
    assert_ne!(sibling_source["sourceId"], source["sourceId"]);
    let (status, wrong_task) = page(&app, &base, &sibling, &source, &Value::Null).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_source_recovery(&wrong_task, &sibling, "task_source_not_in_catalog");
    let unknown = json!({"sourceId":format!("source-{}", "0".repeat(64))});
    assert_eq!(
        page(&app, &base, &sibling, &unknown, &Value::Null).await.1,
        wrong_task,
        "Unknown and foreign IDs must not disclose another task's catalogue or contents"
    );
    let (status, wrong_cursor) = page(
        &app,
        &base,
        &sibling,
        &sibling_source,
        &first_page["nextCursor"],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_source_recovery(&wrong_cursor, &sibling, "task_source_cursor_invalid");
    assert_eq!(
        read(&app, &base, &sibling, &sibling_source).await,
        original,
        "Recovery requires an explicit current directory read; it never substitutes another task's source"
    );
    let mut next = original[0]["content"].clone();
    next["sceneGroups"][0]["body"] = json!("修改后原组");
    next["body"] = json!("修改后原组\n\n---\n\n锁定原文");
    next["completeness"] = json!("chapterComplete");
    let (status,receipt)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,json!({"taskId":writer["id"],"threadId":"writer","payload":next,"expectedRevision":0,"expectedCandidateRevisionId":first["revisionId"],"idempotencyKey":"own-checkpoint"})).await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    assert_eq!(read(&app, &base, &writer, &source).await, original);
    assert_eq!(
        call(&app, "GET", &index_uri, CANDIDATE, Value::Null)
            .await
            .1,
        frozen_index
    );
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
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"全部场景按需读取"}),
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

#[tokio::test]
async fn parsed_source_pages_preserve_unicode_and_reject_foreign_cursors() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("parsed.sqlite3");
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"解析阅读"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let body = "原文🙂\"\\\n".repeat(7000);
    let (status,draft)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"draft","payload":{"chapterId":"chapter-1","chapterNumber":1,"title":"原题","body":body,"sceneGroups":[{"id":"editable","sceneId":null,"body":body,"locked":false}],"completeness":"chapterComplete"}})).await;
    assert_eq!(status, StatusCode::CREATED);
    let task = task(&app, &base, "advisory", "reader", json!({"type":"project"})).await;
    let pack = context(&app, &base, &task).await;
    let source = entry(&pack, "/relevantText");
    let endpoint = format!(
        "{base}/tasks/{}/task-source-content?threadId=reader&expectedRevision={}&sourceId={}",
        task["id"].as_str().unwrap(),
        task["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let locate = format!("{endpoint}&view=locate&quote=%E5%8E%9F%E6%96%87");
    let (status, first) = call(&app, "GET", &locate, CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["matchCount"], 7000);
    assert!(first["nextCursor"].is_string());
    let next = format!("{locate}&cursor={}", first["nextCursor"].as_str().unwrap());
    let (status, second) = call(&app, "GET", &next, CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert!(second["entries"][0]["editAnchor"]["start"].as_u64().unwrap() > first["entries"].as_array().unwrap().last().unwrap()["editAnchor"]["start"].as_u64().unwrap());
    assert_eq!(first["coverage"]["completeSource"],false);
    let changed = next.replace("quote=%E5%8E%9F%E6%96%87", "quote=%E6%96%87");
    assert_eq!(call(&app,"GET",&changed,CANDIDATE,Value::Null).await.0,StatusCode::BAD_REQUEST);
    let mut cursor = String::new();
    let mut recovered = String::new();
    let mut count = 0;
    // Host event persistence may write while a tool returns source pages.
    // Metadata-only writes preserve all frozen prose and task source versions.
    let concurrent = std::thread::spawn(move || {
        let db = rusqlite::Connection::open(path).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        for _ in 0..200 {
            db.execute("UPDATE story_projects_v2 SET updated_at=updated_at+1", [])
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    loop {
        let (status, page) = call(
            &app,
            "GET",
            &format!("{endpoint}{cursor}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.to_string().len() <= 63 * 1024);
        let envelope = json!({"content":[{"type":"text","text":page.to_string()}]});
        let model_item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":envelope.to_string()});
        assert!(model_item.to_string().len() <= 256 * 1024 - 1024);
        assert_eq!(page["representation"], "parsedFields");
        assert_eq!(page["contentRead"], true);
        assert_eq!(page["returnedChapterNumbers"], json!([1]));
        assert_eq!(page["sectionPath"], "/relevantText");
        assert!(page["chapterNumbers"].is_array());
        for field in page["entries"].as_array().unwrap() {
            if field["path"].as_str().unwrap().ends_with("/content/sceneGroups/0/body") {
                recovered.push_str(
                    field["text"]
                        .as_str()
                        .or_else(|| field["value"].as_str())
                        .unwrap(),
                );
            }
        }
        count += 1;
        if let Some(next) = page["nextCursor"].as_str() {
            cursor = format!("&cursor={next}");
        } else {
            break;
        }
    }
    concurrent.join().unwrap();
    assert!(count > 1);
    assert_eq!(recovered, body);
    let restored = context(&app, &base, &task).await;
    let coverage = &entry(&restored, "/relevantText")["returnedCoverage"];
    assert_eq!(coverage["complete"], true);
    assert_eq!(coverage["semanticUnderstanding"], false);
    assert_eq!(coverage["authority"], "returnedParsedFieldsOnly");
    assert_eq!(coverage["nextUnreadCursor"], Value::Null);
    let (_, _) = call(&app, "GET", &endpoint, CANDIDATE, Value::Null).await;
    assert_eq!(
        entry(&context(&app, &base, &task).await, "/relevantText")["returnedCoverage"]["returnedEntries"],
        coverage["returnedEntries"]
    );
    let plan_endpoint = format!("{base}/tasks/{}/work-plan", task["id"].as_str().unwrap());
    let read_plan = format!(
        "{plan_endpoint}?threadId=reader&expectedRevision={}",
        task["inputRevision"]
    );
    let (status, initial) = call(&app, "GET", &read_plan, CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial["version"], 0);
    let quote: String = body.chars().take(8).collect();
    let update = json!({"taskId":task["id"],"threadId":"reader","expectedRevision":task["inputRevision"],"expectedPlanVersion":0,"idempotencyKey":"checked","plan":{"intentMode":"executeGoals","goals":[{"id":"read","title":"核对原文","acceptance":"完整读取相关正文","kind":"check","status":"satisfied","dependsOn":[],"readingQuestion":"原文是什么","checkScope":"本章正文","approach":"按返回游标读完","assessment":"已读全部分页并核对原文","sourceIds":[source["sourceId"]],"sourceChecks":[{"sourceId":source["sourceId"],"revisionId":draft["revisionId"],"contentHash":source["contentHash"],"evidence":{"quote":quote,"start":0,"end":quote.chars().count()}}],"evidence":[],"continuation":null}]}});
    let (status, receipt) = call(&app, "POST", &plan_endpoint, CANDIDATE, update.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["version"], 1);
    let (_, retry) = call(&app, "POST", &plan_endpoint, CANDIDATE, update.clone()).await;
    assert_eq!(retry["version"], 1);
    let (_, saved) = call(
        &app,
        "GET",
        &format!("{read_plan}&goalId=read"),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(saved["goal"]["status"], "satisfied");
    assert_eq!(saved["goal"]["sourceIds"][0], source["sourceId"]);
    let mut stale = update;
    stale["idempotencyKey"] = json!("stale");
    assert!(
        !call(&app, "POST", &plan_endpoint, CANDIDATE, stale)
            .await
            .0
            .is_success()
    );

    let (status, _) = call(
        &app,
        "GET",
        &format!("{endpoint}&cursor=wrong:0"),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn workspace_reads_keep_exact_scope_and_child_pages_do_not_fill_root_receipts() {
    use tower::ServiceExt;
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("workspace.sqlite3");
    let app = router(task_fixture::state(
        db_path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"工作区读取"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    for n in 1..=3 {
        let(status,saved)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":format!("draft-{n}"),"payload":{"chapterId":format!("chapter-{n}"),"chapterNumber":n,"title":format!("第{n}章"),"body":format!("第{n}章实际原文🙂"),"completeness":"chapterComplete"}})).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
    }
    let task = task(
        &app,
        &base,
        "advisory",
        "workspace-root",
        json!({"type":"project"}),
    )
    .await;
    let pack = context(&app, &base, &task).await;
    let source = pack["taskWorkspace"]["sourceId"].as_str().unwrap();
    assert_eq!(
        entry(&pack, "/authorRequirementsSource")["sectionPath"],
        "/authorRequirementsSource"
    );
    let endpoint = format!(
        "{base}/tasks/{}/task-source-content?threadId=workspace-root&expectedRevision={}",
        task["id"].as_str().unwrap(),
        task["inputRevision"]
    );
    let (status, excerpts) = call(&app,"GET",&format!("{endpoint}&view=passages&edge=end&characters=2"),CANDIDATE,Value::Null).await;
    assert_eq!(status,StatusCode::OK,"{excerpts}");
    assert_eq!(excerpts["selection"],"frozenTaskBaseline");
    assert_eq!(excerpts["items"].as_array().unwrap().len(),3);
    for item in excerpts["items"].as_array().unwrap() {
        assert_eq!(item["excerpt"]["text"],"文🙂");
        assert_eq!(item["excerpt"]["editAnchor"]["sourceRevisionId"],item["revisionId"]);
        assert_eq!(item["excerpt"]["editAnchor"]["writeAuthority"],"notGranted");
    }
    assert_eq!(excerpts["coverage"]["completeSource"],false);
    let item = &excerpts["items"][0];
    let scene_url = format!("{endpoint}&view=scene&sourceId={}&groupId={}",item["sourceId"].as_str().unwrap(),item["excerpt"]["editAnchor"]["groupId"].as_str().unwrap());
    let (status, scene) = call(&app,"GET",&scene_url,CANDIDATE,Value::Null).await;
    assert_eq!(status,StatusCode::OK,"{scene}");
    assert_eq!(scene["contentHash"],item["contentHash"]);
    assert!(scene["entries"].as_array().unwrap().iter().any(|e| e["editAnchor"]["groupId"] == item["excerpt"]["editAnchor"]["groupId"]));
    assert_eq!(call(&app,"GET",&format!("{scene_url}&quote=invalid"),CANDIDATE,Value::Null).await.0,StatusCode::BAD_REQUEST);

    for suffix in ["view=passages&edge=end", "view=passages&edge=end&characters=513", "view=passages&edge=middle&characters=2", "view=workspace&edge=end&characters=2", "view=passages&edge=end&characters=2&objectKind=brief"] {
        assert_eq!(call(&app,"GET",&format!("{endpoint}&{suffix}"),CANDIDATE,Value::Null).await.0,StatusCode::BAD_REQUEST);
    }
    let (status, directory) = call(
        &app,
        "GET",
        &format!("{endpoint}&view=workspace&chapterNumber=2&objectKind=draft"),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{directory}");
    assert_eq!(directory["items"].as_array().unwrap().len(), 1);
    assert_eq!(directory["visibility"]["planningArtifacts"], "eligibleWithinTaskScope");
    assert_eq!(directory["visibility"]["existenceOutsideScope"], "unknown");
    assert_eq!(directory["contentRead"], false);
    assert!(!directory.to_string().contains("实际原文"));
    let revision = directory["items"][0]["revisionId"].as_str().unwrap();
    let (status, exact) = call(&app, "GET", &format!("{endpoint}&view=workspace&chapterNumber=2&revisionId={revision}"), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{exact}");
    assert_eq!(exact["items"], directory["items"]);
    for suffix in ["".to_owned(), format!("&view=workspace&sourceId=source-{}", "f".repeat(64)), format!("&sourceId={source}&revisionId={revision}")] {
        let (status, _) = call(&app, "GET", &format!("{endpoint}{suffix}"), CANDIDATE, Value::Null).await;
        assert!(status.is_client_error(), "{suffix}");
    }
    let (status, hidden) = call(&app, "GET", &format!("{endpoint}&view=workspace&revisionId=not-admitted"), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(hidden["items"].as_array().unwrap().is_empty());
    let leaf = directory["items"][0]["sourceId"].as_str().unwrap();
    let request = Request::builder()
        .uri(format!("{endpoint}&sourceId={leaf}"))
        .header("authorization", format!("Bearer {CANDIDATE}"))
        .header("x-novel-task-capability", CAP)
        .header("x-novel-tool-name", "read_source_content")
        .header("x-novel-root-thread", "workspace-root")
        .header("x-novel-caller-thread", "trusted-child")
        .header("x-novel-parent-thread", "workspace-root")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let child: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(child.to_string().contains("第2章实际原文"));
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_task_source_returned_v1 WHERE source_id=?1",
            [leaf],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    // Boundary excerpts already recorded a root dependency. Verify the child
    // independently instead of assuming this source has only one reader.
    let child_reads: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_task_workspace_reads_v1 WHERE source_id=?1 AND reader_thread_id='trusted-child'",
            [leaf],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(child_reads, 1);
    let (status, root) = call(
        &app,
        "GET",
        &format!("{endpoint}&sourceId={leaf}"),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{root}");
    assert_eq!(root["contentHash"], child["contentHash"]);
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_task_source_returned_v1 WHERE source_id=?1",
            [leaf],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{endpoint}&sourceId={leaf}&chapterNumber=2"),
            CANDIDATE,
            Value::Null
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn frozen_index_locates_a_later_chapter_without_reading_prior_bodies() {
    let temp = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        temp.path().join("index.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, project) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"直接定位章节"}),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    for n in 1..=12 {
        let (status, saved) = call(&app, "POST", &format!("{base}/drafts/checkpoints"), AUTHOR, json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":format!("draft-{n}"),"payload":{"chapterId":format!("chapter-{n}"),"chapterNumber":n,"title":format!("真实标题{n}"),"body":format!("第{n}章原文🙂\"\\\n").repeat(if n == 6 { 4000 } else { 400 }),"completeness":"chapterComplete"}})).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
    }
    let current = task(
        &app,
        &base,
        "advisory",
        "index-reader",
        json!({"type":"project"}),
    )
    .await;
    let pack = context(&app, &base, &current).await;
    let source = entry(&pack, "/relevantText");
    let endpoint = format!(
        "{base}/tasks/{}/task-source-content?threadId=index-reader&expectedRevision={}&sourceId={}",
        current["id"].as_str().unwrap(),
        current["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let mut cursor = String::new();
    let mut selected = Value::Null;
    let mut identities = std::collections::BTreeSet::new();
    let mut first_index_cursor = None;
    loop {
        let (status, index) = call(
            &app,
            "GET",
            &format!("{endpoint}&view=index{cursor}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{index}");
        assert!(index.to_string().len() <= 6500);
        assert_eq!(index["representation"], "sourceIndex");
        assert_eq!(index["coverage"]["contentRead"], false);
        assert!(!index.to_string().contains("章原文"));
        for item in index["items"].as_array().unwrap() {
            assert!(identities.insert(item["identity"]["chapterNumber"].as_u64().unwrap()));
            if item["identity"]["chapterNumber"] == 6 {
                selected = item.clone();
            }
        }
        if let Some(next) = index["nextCursor"].as_str() {
            first_index_cursor.get_or_insert(next.to_owned());
            cursor = format!("&cursor={next}");
        } else {
            break;
        }
    }
    assert_eq!(identities.len(), 12);
    let after_index = context(&app, &base, &current).await;
    assert_eq!(
        entry(&after_index, "/relevantText")["returnedCoverage"]["returnedEntries"],
        0
    );
    assert_eq!(selected["identity"]["chapterId"], "chapter-6");
    assert_eq!(selected["identity"]["title"], "真实标题6");
    assert!(selected["entryRange"]["start"].as_u64().unwrap() > 0);
    let body_path = format!("{}/content/body", selected["path"].as_str().unwrap());
    let mut cursor = selected["readCursor"].as_str().unwrap().to_owned();
    let mut recovered = String::new();
    let mut pages = 0;
    loop {
        let (status, page) = call(
            &app,
            "GET",
            &format!("{endpoint}&cursor={cursor}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["coverage"]["complete"], false);
        pages += 1;
        assert!(selected["entryRange"]["end"].as_u64().unwrap() < page["coverage"]["totalEntries"].as_u64().unwrap(), "the regression must select a non-final record");
        assert_eq!(page["returnedChapterNumbers"], json!([6]));
        assert!(page["coverage"]["toEntry"].as_u64().unwrap() <= selected["entryRange"]["end"].as_u64().unwrap());
        for field in page["entries"].as_array().unwrap() {
            assert!(field["path"].as_str().unwrap().starts_with(&format!("{}/", selected["path"].as_str().unwrap())), "unselected field: {field}");
            if field["path"] == body_path {
                recovered.push_str(
                    field["text"]
                        .as_str()
                        .or_else(|| field["value"].as_str())
                        .unwrap(),
                );
            }
        }
        if page["coverage"]["toEntry"].as_u64().unwrap()
            >= selected["entryRange"]["end"].as_u64().unwrap()
        {
            assert_eq!(page["nextCursor"], Value::Null);
            break;
        }
        cursor = page["nextCursor"].as_str().unwrap().to_owned();
    }
    assert!(pages > 1);
    assert_eq!(recovered, "第6章原文🙂\"\\\n".repeat(4000));
    let restored = context(&app, &base, &current).await;
    let coverage = &entry(&restored, "/relevantText")["returnedCoverage"];
    assert_eq!(coverage["complete"], false);
    assert!(coverage["entryRanges"][0]["start"].as_u64().unwrap() > 0);
    assert!(
        coverage["nextUnreadCursor"]
            .as_str()
            .unwrap()
            .split(':').nth(1) == Some("0")
    );
    let fence = selected["readCursor"].as_str().unwrap().split(':').next().unwrap();
    for url in [
        format!("{endpoint}&cursor={fence}:0:0"),
        format!("{endpoint}&cursor={fence}:0:{}", coverage["totalEntries"].as_u64().unwrap()+1),
        format!("{endpoint}&cursor={fence}:00:1"),
        format!("{endpoint}&cursor={fence}:0"),
        format!("{endpoint}&cursor={}", first_index_cursor.unwrap()),
        format!(
            "{endpoint}&view=index&cursor={}",
            selected["readCursor"].as_str().unwrap()
        ),
    ] {
        assert_eq!(
            call(&app, "GET", &url, CANDIDATE, Value::Null).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let other = task(
        &app,
        &base,
        "advisory",
        "foreign-reader",
        json!({"type":"project"}),
    )
    .await;
    let foreign = format!(
        "{base}/tasks/{}/task-source-content?threadId=foreign-reader&expectedRevision={}&sourceId={}&view=index",
        other["id"].as_str().unwrap(),
        other["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let (status, rejected) = call(&app, "GET", &foreign, CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_source_recovery(&rejected, &other, "task_source_catalog_required");
}
