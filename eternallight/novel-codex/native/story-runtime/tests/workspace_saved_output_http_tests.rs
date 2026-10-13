//! Current HTTP permission and byte/hash contracts for actual saved workspace output.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
const AUTHOR: &str = "workspace-output-author-123456789012345";
const CANDIDATE: &str = "workspace-output-candidate-123456789012";
const CAP: &str = "workspace-output-capability-1234567890";
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
    let raw = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&raw).unwrap_or_else(|e| {
            panic!(
                "{method} {uri} {status}: {e}; {}",
                String::from_utf8_lossy(&raw)
            )
        }),
    )
}
async fn setup() -> (tempfile::TempDir, Router, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = novel_codex_story_runtime::router(task_fixture::state(
        dir.path().join("workspace.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (s,p)=call(&app,"POST","/api/projects",AUTHOR,json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"实际产物回读"})).await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    (
        dir,
        app,
        format!("/api/projects/{}", p["project"]["id"].as_str().unwrap()),
    )
}
async fn task(app: &Router, base: &str, op: &str, thread: &str, target: Value) -> Value {
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (s,t)=call(app,"POST",&format!("{base}/tasks"),AUTHOR,json!({"operation":op,"inputRevision":0,"threadId":thread,"target":target,"authorIntent":"在当前授权内保存候选稿，读取实际保存内容，不采用、不发布。","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":expires})).await;
    assert_eq!(s, StatusCode::CREATED, "{t}");
    t
}
fn binding(t: &Value) -> String {
    format!(
        "threadId={}&expectedRevision={}",
        t["threadId"].as_str().unwrap(),
        t["inputRevision"]
    )
}
async fn context(app: &Router, base: &str, t: &Value) -> Value {
    let (s, p) = call(
        app,
        "GET",
        &format!(
            "{base}/context?taskId={}&threadId={}&projectRevision={}&purpose={}&byteBudget=60000",
            t["id"].as_str().unwrap(),
            t["threadId"].as_str().unwrap(),
            t["inputRevision"],
            t["purpose"].as_str().unwrap()
        ),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    p
}
async fn catalogue(app: &Router, base: &str, t: &Value, directory: &Value) -> Vec<Value> {
    let mut items = Vec::new();
    let mut cursor = None;
    loop {
        let tail = cursor
            .as_ref()
            .map(|c| format!("&cursor={c}"))
            .unwrap_or_default();
        let (s, p) = call(
            app,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source-content?{}&sourceId={}&view=workspace{tail}",
                t["id"].as_str().unwrap(),
                binding(t),
                directory.as_str().unwrap()
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{p}");
        assert!(p.to_string().len() <= 6500);
        assert_eq!(p["contentRead"], false);
        items.extend(p["items"].as_array().unwrap().clone());
        cursor = p["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return items;
        }
    }
}
async fn raw(app: &Router, base: &str, t: &Value, source: &Value) -> (String, Value) {
    let mut cursor = None;
    let mut text = String::new();
    let mut hash = None;
    let mut path = None;
    loop {
        let tail = cursor
            .as_ref()
            .map(|c| format!("&cursor={c}"))
            .unwrap_or_default();
        let (s, p) = call(
            app,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source?{}&sourceId={}{tail}",
                t["id"].as_str().unwrap(),
                binding(t),
                source.as_str().unwrap()
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{p}");
        assert!(p.to_string().len() <= 6500);
        assert_eq!(p["byteRange"]["start"], text.len());
        let part = p["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        text.push_str(part);
        assert_eq!(p["byteRange"]["end"], text.len());
        if let Some(h) = &hash {
            assert_eq!(h, &p["contentHash"])
        } else {
            hash = Some(p["contentHash"].clone())
        }
        if let Some(v) = &path {
            assert_eq!(v, &p["sectionPath"])
        } else {
            path = Some(p["sectionPath"].clone())
        }
        cursor = p["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(p["byteRange"]["total"], text.len());
            break;
        }
    }
    assert_eq!(
        hash.unwrap(),
        format!("{:x}", Sha256::digest(text.as_bytes()))
    );
    (text.clone(), serde_json::from_str(&text).unwrap())
}
fn relevant(pack: &Value) -> Value {
    pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["sectionPath"] == "/relevantText")
        .unwrap()
        .clone()
}

#[tokio::test]
async fn model_can_read_own_checkpoint_again_after_another_save_without_replacing_frozen_input_or_widening_prefix()
 {
    for prefix in [false, true] {
        let (_dir, app, base) = setup().await;
        let body = "原稿前缀\n\n---\n\n受保护秘密后缀";
        let mut initial = Value::Null;
        for n in 1..=2 {
            let (s,p)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"payload":{"chapterId":format!("chapter-{n}"),"chapterNumber":n,"title":"原题","body":body,"sceneGroups":[{"id":"one","body":"原稿前缀","locked":false},{"id":"two","body":"受保护秘密后缀","locked":true}],"completeness":"chapterComplete"},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":format!("original-{n}")})).await;
            assert_eq!(s, StatusCode::CREATED, "{p}");
            if n == 1 {
                initial = p
            }
        }
        let mut target = json!({"type":"chapter","chapterId":"chapter-1","chapterNumber":1,"readingChapter":1,"sceneOrder":1,"scope":{"type":"wholeChapter"}});
        if prefix {
            target["povCharacterId"] = json!("person");
            target["readScope"] = json!({"type":"throughScene","endSceneOrder":1});
            target["scope"] = json!({"type":"throughScene","endSceneOrder":1})
        }
        let writer = task(&app, &base, "generateChapter", "writer", target).await;
        let pack = context(&app, &base, &writer).await;
        let input = relevant(&pack);
        let (_, original) = raw(&app, &base, &writer, &input["sourceId"]).await;
        let mut payload = original[0]["content"].clone();
        let new_body = "新保存前缀🙂\"\\\n".repeat(800);
        payload["sceneGroups"][0]["body"] = json!(new_body);
        payload["body"] = json!(
            payload["sceneGroups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| g["body"].as_str().unwrap().trim())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        );
        payload["completeness"] = json!("chapterComplete");
        if !prefix {
            payload["title"] = json!("已保存新题")
        }
        let (s,first)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,json!({"taskId":writer["id"],"threadId":"writer","payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":initial["revisionId"],"idempotencyKey":"save-one"})).await;
        assert_eq!(s, StatusCode::CREATED, "{first}");
        assert_eq!(first["savedSource"]["status"], "available");
        assert_eq!(first["savedSource"]["revisionId"], first["revisionId"]);
        assert_eq!(first["savedSource"]["contentRead"], false);
        let direct = raw(&app, &base, &writer, &first["savedSource"]["sourceId"]).await;
        assert_eq!(direct.1["content"]["sceneGroups"][0]["body"], new_body);
        if prefix { assert!(!direct.0.contains("受保护秘密后缀")); }

        let current = context(&app, &base, &writer).await;
        assert_eq!(relevant(&current), input);
        assert_eq!(
            raw(&app, &base, &writer, &input["sourceId"]).await.1,
            original
        );
        let items = catalogue(&app, &base, &writer, &current["taskWorkspace"]["sourceId"]).await;
        if prefix {
            assert!(items.iter().all(|i| i["objectId"] == "draft-chapter-1"))
        }
        let own = items
            .iter()
            .find(|v| v["purpose"] == "savedCheckpoint" && v["revisionId"] == first["revisionId"])
            .unwrap()
            .clone();
        assert_eq!(own["producerTaskId"], writer["id"]);
        assert_eq!(own["outputOrdinal"], 1);
        assert_eq!(own["isCurrentAtListing"], true);
        assert_eq!(own["sourceId"], first["savedSource"]["sourceId"]);
        let (first_text, first_content) = raw(&app, &base, &writer, &own["sourceId"]).await;
        if prefix {
            assert!(!first_text.contains("受保护秘密后缀"));
            assert_eq!(
                first_content["content"]["sceneGroups"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            )
        } else {
            assert_eq!(first_content["content"]["title"], "已保存新题")
        }
        payload["sceneGroups"][0]["body"] = json!("第二次合法保存");
        payload["body"] = json!(
            payload["sceneGroups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| g["body"].as_str().unwrap().trim())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        );
        let (s,second)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),CANDIDATE,json!({"taskId":writer["id"],"threadId":"writer","payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":first["revisionId"],"idempotencyKey":"save-two"})).await;
        assert_eq!(s, StatusCode::CREATED, "{second}");
        assert_eq!(
            raw(&app, &base, &writer, &own["sourceId"]).await.0,
            first_text
        );
        let current = context(&app, &base, &writer).await;
        let items = catalogue(&app, &base, &writer, &current["taskWorkspace"]["sourceId"]).await;
        assert_eq!(
            items
                .iter()
                .find(
                    |v| v["revisionId"] == first["revisionId"] && v["purpose"] == "savedCheckpoint"
                )
                .unwrap()["isCurrentAtListing"],
            false
        );
        assert_eq!(
            items
                .iter()
                .find(|v| v["revisionId"] == second["revisionId"]
                    && v["purpose"] == "savedCheckpoint")
                .unwrap()["outputOrdinal"],
            2
        );
        assert_eq!(
            raw(&app, &base, &writer, &input["sourceId"]).await.1,
            original
        );
        let frozen = items
            .iter()
            .find(|v| v["authority"] == "frozenWriterInput")
            .unwrap();
        assert_eq!(frozen["title"], "原题");
        let foreign = task(
            &app,
            &base,
            "advisory",
            "foreign",
            json!({"type":"project"}),
        )
        .await;
        context(&app, &base, &foreign).await;
        let (s, p) = call(
            &app,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source?{}&sourceId={}",
                foreign["id"].as_str().unwrap(),
                binding(&foreign),
                own["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(p["error"]["code"], "task_source_not_in_catalog");
    }
}

#[tokio::test]
async fn actual_planning_report_is_readable_via_same_lazy_sources_without_revision_or_global_report_access()
 {
    let (_dir, app, base) = setup().await;
    let (s,outline)=call(&app,"POST",&format!("{base}/artifacts/outline"),AUTHOR,json!({"payload":{"nodes":[{"id":"book","kind":"book","position":1,"title":"全书","purpose":"悬疑","summary":"已有大纲","dependencyKeys":[],"certainty":"tentative","protection":"free"}]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"})).await;
    assert_eq!(s, StatusCode::CREATED, "{outline}");
    let (s,run)=call(&app,"POST",&format!("{base}/planning-reviews"),AUTHOR,json!({"artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,"authorIntent":"结构评审，不改正文","roles":["structure"]})).await;
    assert_eq!(s, StatusCode::OK, "{run}");
    let work = run["workTaskId"].as_str().unwrap();
    let (_, ws) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    let (s,budget)=call(&app,"POST",&format!("{base}/work-tasks/{work}/budget-authorization"),AUTHOR,json!({"expectedVersion":0,"expectedProjectRevision":0,"expectedSourceDigest":ws["sourceDigest"],"maxRequests":16,"maxTokens":200000,"deadlineAt":null})).await;
    assert_eq!(s, StatusCode::OK, "{budget}");
    let run_uri = format!("{base}/planning-reviews/{}", run["id"].as_str().unwrap());
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (s,seat)=call(&app,"POST",&format!("{run_uri}/seats/structure/tasks"),AUTHOR,json!({"threadId":"seat-root","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":expires})).await;
    assert_eq!(s, StatusCode::OK, "{seat}");
    let report = json!({"summary":"原始报告🙂\"\\\n".repeat(600),"strengths":[],"findings":[],"coverage":[{"checkId":"structure:book","status":"checked"}]});
    let (s,receipt)=call(&app,"POST",&format!("{run_uri}/reports"),AUTHOR,json!({"taskId":seat["id"],"threadId":seat["threadId"],"expectedRevision":0,"report":report})).await;
    assert_eq!(s, StatusCode::OK, "{receipt}");
    let pack = context(&app, &base, &seat).await;
    let items = catalogue(&app, &base, &seat, &pack["taskWorkspace"]["sourceId"]).await;
    let saved = items
        .iter()
        .find(|i| i["purpose"] == "savedReport")
        .unwrap();
    assert_eq!(saved["authority"], "nativePlanningReport");
    assert_eq!(saved["revisionId"], receipt["id"]);
    assert_eq!(saved["producerTaskId"], seat["id"]);
    assert_eq!(
        raw(&app, &base, &seat, &saved["sourceId"]).await.1["content"],
        report
    );
    let foreign = task(
        &app,
        &base,
        "advisory",
        "foreign",
        json!({"type":"project"}),
    )
    .await;
    let pack = context(&app, &base, &foreign).await;
    assert!(
        !catalogue(&app, &base, &foreign, &pack["taskWorkspace"]["sourceId"])
            .await
            .iter()
            .any(|i| i["revisionId"] == receipt["id"])
    );
    let (s, _) = call(
        &app,
        "GET",
        &format!(
            "{base}/tasks/{}/task-source?{}&sourceId={}",
            foreign["id"].as_str().unwrap(),
            binding(&foreign),
            saved["sourceId"].as_str().unwrap()
        ),
        CANDIDATE,
        Value::Null,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
}
