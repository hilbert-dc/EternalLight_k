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
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"冻结原稿"}),
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
    let frozen = read_section(&app, &base, &task, &context, "/relevantText").await;
    assert_eq!(frozen.as_array().unwrap().len(), 1);
    assert_eq!(frozen[0]["sourceRevisionId"], saved["revisionId"]);
    assert_eq!(frozen[0]["content"]["sceneGroups"], payload["sceneGroups"]);
    assert_eq!(frozen[0]["content"]["sceneGroups"][0]["body"], body);
    assert_eq!(frozen[0]["authority"], "frozenWriterInput");
    assert_model_item_bound(&context);
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
    let after_source = read_section(&app, &base, &task, &after, "/relevantText").await;
    assert_eq!(after_source, frozen);
    assert_eq!(after_source[0]["sourceRevisionId"], saved["revisionId"]);
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
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"原稿来源"}),
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

fn assert_model_item_bound(value: &Value) {
    let result = json!({"content":[{"type":"text","text":value.to_string()}]});
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":result.to_string()});
    let tokens = eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(tokens <= 9000, "actual model item has {tokens} tokens");
}

async fn read_section(
    app: &Router,
    root: &str,
    task: &Value,
    context: &Value,
    section: &str,
) -> Value {
    let entry = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == section)
        .unwrap_or_else(|| panic!("missing current source {section}: {context}"));
    assert_eq!(
        context.pointer(section).unwrap()["sourceId"],
        entry["sourceId"]
    );
    read_content(
        app,
        root,
        task,
        entry["sourceId"].as_str().unwrap(),
        entry["contentHash"].as_str(),
    )
    .await
}

// Rebuild only the actually returned parsed fields; no database/author read can
// stand in for the task's complete source transport and character coverage.
async fn read_content(
    app: &Router,
    root: &str,
    task: &Value,
    source: &str,
    expected_hash: Option<&str>,
) -> Value {
    let endpoint = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={source}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let mut cursor = String::new();
    let mut content_hash = expected_hash.map(str::to_owned);
    let mut returned = Value::Null;
    let mut chunks = std::collections::BTreeMap::<String, (usize, usize, String)>::new();
    let mut from = 0usize;
    let mut total = None;
    loop {
        let (status, page) = call(
            app,
            "GET",
            &format!("{endpoint}{cursor}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source);
        assert_eq!(page["representation"], "parsedFields");
        assert_eq!(page["contentRead"], true);
        assert!(page.to_string().len() <= 6500);
        assert_model_item_bound(&page);
        let hash = page["contentHash"].as_str().unwrap();
        assert_eq!(
            content_hash.get_or_insert_with(|| hash.to_owned()).as_str(),
            hash
        );
        assert_eq!(page["coverage"]["fromEntry"], from);
        let entries = page["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        from += entries.len();
        assert_eq!(page["coverage"]["toEntry"], from);
        let count = page["coverage"]["totalEntries"].as_u64().unwrap() as usize;
        assert_eq!(*total.get_or_insert(count), count);
        assert!(from <= count);
        for entry in entries {
            let pointer = entry["path"].as_str().unwrap();
            if let Some(text) = entry["text"].as_str() {
                let range = &entry["characterRange"];
                let start = range["start"].as_u64().unwrap() as usize;
                let end = range["end"].as_u64().unwrap() as usize;
                let full = range["total"].as_u64().unwrap() as usize;
                let chunk = chunks
                    .entry(pointer.to_owned())
                    .or_insert((0, full, String::new()));
                assert_eq!((start, full), (chunk.0, chunk.1));
                assert_eq!(end, start + text.chars().count());
                assert!(end <= full);
                chunk.0 = end;
                chunk.2.push_str(text);
            } else {
                assert!(entry.get("value").is_some(), "{entry}");
                set_field(&mut returned, pointer, entry["value"].clone());
            }
        }
        if let Some(next) = page["nextCursor"].as_str() {
            assert!(from < count);
            let next = format!("&cursor={next}");
            assert_ne!(next, cursor);
            cursor = next;
        } else {
            assert_eq!(from, count);
            break;
        }
    }
    for (pointer, (end, total, text)) in chunks {
        assert_eq!(end, total);
        set_field(&mut returned, &pointer, json!(text));
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(returned.to_string().as_bytes())),
        content_hash.unwrap()
    );
    returned
}

fn set_field(value: &mut Value, pointer: &str, field: Value) {
    let parts: Vec<String> = pointer.strip_prefix('/').map_or_else(Vec::new, |pointer| {
        pointer
            .split('/')
            .map(|part| part.replace("~1", "/").replace("~0", "~"))
            .collect()
    });
    fn put(value: &mut Value, parts: &[String], field: Value) {
        let Some((part, rest)) = parts.split_first() else {
            *value = field;
            return;
        };
        if value.is_null() {
            *value = if part.parse::<usize>().is_ok() {
                json!([])
            } else {
                json!({})
            };
        }
        match value {
            Value::Array(items) => {
                let index = part.parse::<usize>().unwrap();
                if items.len() <= index {
                    items.resize(index + 1, Value::Null);
                }
                put(&mut items[index], rest, field);
            }
            Value::Object(object) => put(
                object.entry(part.clone()).or_insert(Value::Null),
                rest,
                field,
            ),
            _ => panic!("non-container returned source at {parts:?}"),
        }
    }
    put(value, &parts, field);
}
