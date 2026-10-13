use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::{RuntimeState, router};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
const AUTHOR: &str = "media-author-123456789012345678901";
const CANDIDATE: &str = "media-candidate-123456789012345678";
const CAP: &str = "media-capability-1234567890123456";
fn text_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
async fn call(
    app: &Router,
    method: &str,
    url: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(url)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("x-novel-task-capability", CAP)
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
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)})),
    )
}
#[tokio::test]
async fn create_standalone_media_save_and_reopen_without_novel_projection() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("media.sqlite");
    let app = router(RuntimeState::new(
        db.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).unwrap());
    for medium in ["comic", "animation"] {
        let request = json!({"title":"原创","logline":"","medium":medium,"idempotencyKey":format!("create-{medium}")});
        let (status, p) = call(&app, "POST", "/api/projects", AUTHOR, request.clone()).await;
        assert_eq!(status, StatusCode::CREATED, "{p}");
        assert!(p.get("novelEdition").is_none());
        assert_eq!(p["editions"].as_array().unwrap().len(), 1);
        let pid = p["project"]["id"].as_str().unwrap();
        let eid = p["activeEditionId"].as_str().unwrap();
        let url = format!("/api/projects/{pid}/editions/{eid}/workspace");
        let (status, w) = call(&app, "GET", &url, AUTHOR, Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{w}");
        assert_eq!(w["document"]["kind"], medium);
        assert_eq!(w["revision"], 0);
        assert!(w["sourceDigest"].as_str().unwrap().len() == 64);
        let save = json!({"expectedRevisionId":null,"document":w["document"],"idempotencyKey":"save-1","readSet":[],"sources":[]});
        let (status, saved) = call(&app, "POST", &url, AUTHOR, save.clone()).await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert!(saved["revisionId"].is_string());
        assert_eq!(
            call(&app, "POST", &url, AUTHOR, save).await.1["revisionId"],
            saved["revisionId"]
        );
        assert_eq!(
            call(
                &app,
                "GET",
                &format!("/api/projects/{pid}/state"),
                AUTHOR,
                Value::Null
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(&app, "POST", "/api/projects", AUTHOR, request).await.1["project"]["id"],
            pid
        );
        let reopened = router(RuntimeState::new(
            db.clone(),
            AUTHOR.into(),
            CANDIDATE.into(),
        ).unwrap());
        assert_eq!(
            call(&reopened, "GET", &url, AUTHOR, Value::Null).await.1["revisionId"],
            saved["revisionId"]
        );
    }
}
#[tokio::test]
async fn candidate_media_endpoint_requires_capability_and_keeps_original_scope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("media.sqlite");
    let mut repo = SqliteStoryRepository::open(&db).unwrap();
    let p = repo
        .create_creative_project(
            &CreateCreativeProjectRequest {
                title: "漫画".into(),
                logline: "".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "project".into(),
            },
            100,
        )
        .unwrap();
    let pid = ProjectId::parse(p.project.id.clone()).unwrap();
    let e = &p.editions[0];
    repo.conversation_store().create_conversation(
        &pid,
        CreateConversationRequest {
            scope_id: Some(e.scope_id.clone()),
            thread_id: "media-thread".into(),
            title: "创作".into(),
            forked_from_id: None,
            last_turn_id: None,
        },
        100,
    )
    .unwrap();
    let task=repo.create_task(CreateTaskRequest{task_id:"media-task".into(),project_id:pid.clone(),thread_id:"media-thread".into(),operation:"advisory".into(),input_revision:0,model:None,target_json:json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id}).to_string(),input_manifest_json:json!({"authorIntent":"创作漫画"}).to_string(),capability_hash:format!("{:x}",Sha256::digest(CAP.as_bytes())),capability_expires_at:4_000_000_000}).unwrap();
    drop(repo);
    let app = router(RuntimeState::new(db, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let url = format!("/api/projects/{pid}/tasks/{}/media/workspace", task.task_id);
    let read = json!({"threadId":"media-thread","expectedRevision":0,"action":"read"});
    assert_eq!(
        call(&app, "POST", &url, AUTHOR, read.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, w) = call(&app, "POST", &url, CANDIDATE, read.clone()).await;
    assert_eq!(status, StatusCode::OK, "{w}");
    assert_eq!(w["edition"]["id"], e.id);
    let impacts = json!({"threadId":"media-thread","expectedRevision":0,"action":"read","section":"sourceImpacts"});
    let (status, report) = call(&app, "POST", &url, CANDIDATE, impacts.clone()).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["authority"], "changeCandidatesOnly");
    assert_eq!(report["editionId"], e.id);
    assert_eq!(report["items"], json!([]));
    let mut invalid = impacts.clone(); invalid["version"] = json!("working");
    assert_eq!(call(&app, "POST", &url, CANDIDATE, invalid).await.0, StatusCode::BAD_REQUEST);
    invalid = impacts; invalid["threadId"] = json!("other-thread");
    assert_eq!(call(&app, "POST", &url, CANDIDATE, invalid).await.0, StatusCode::UNAUTHORIZED);

    let(status,saved)=call(&app,"POST",&url,CANDIDATE,json!({"threadId":"media-thread","expectedRevision":0,"action":"save","request":{"expectedRevisionId":null,"document":{"kind":"comic","title":"漫画","format":"pages","pages":[]},"idempotencyKey":"candidate-save","readSet":[],"sources":[]}})).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert!(saved["revisionId"].is_string());
    let frozen = call(&app, "POST", &url, CANDIDATE, read).await;
    assert_eq!(frozen.0, StatusCode::OK, "{}", frozen.1);
    assert!(frozen.1["revisionId"].is_null());
    let (status, patched)=call(&app,"POST",&url,CANDIDATE,json!({"threadId":"media-thread","expectedRevision":0,"action":"patch","request":{"expectedRevisionId":saved["revisionId"],"idempotencyKey":"candidate-patch","operations":[{"kind":"upsertPage","page":{"id":"page-added","title":"第一页","width":800,"height":1200,"readingDirection":"rightToLeft"}},{"kind":"setTitle","title":"任务作品"},{"kind":"setCreativeDesign","text":"起因😀转折与结局"}],"readSet":[],"sources":[]}})).await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    assert_eq!(patched["counts"]["pages"], 1);
    assert_ne!(patched["revisionId"], saved["revisionId"]);
    assert!(patched.get("document").is_none());
    assert_eq!(patched["documentHeader"]["textFields"][0]["field"]["kind"], "creativeDesign");
    let (status, design) = call(&app,"POST",&url,CANDIDATE,json!({"threadId":"media-thread","expectedRevision":0,"action":"read","version":"working","section":"text","textField":{"kind":"creativeDesign"},"charOffset":2,"charLimit":1})).await;
    assert_eq!(status, StatusCode::OK, "{design}");
    assert_eq!(design["text"], "😀");
    assert_eq!(design["source"]["revisionId"], patched["revisionId"]);

    let (status, _) = call(
        &app,
        "POST",
        &url,
        CANDIDATE,
        json!({"threadId":"other-thread","expectedRevision":0,"action":"read"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_local_patch_is_typed_atomic_and_durable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("author-patch.sqlite");
    let app = router(RuntimeState::new(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).unwrap());
    let (status, p) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"局部作品","logline":"","medium":"comic","idempotencyKey":"project"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{p}");
    let pid = p["project"]["id"].as_str().unwrap();
    let edition = p["activeEditionId"].as_str().unwrap();
    let patch_route = format!("/api/projects/{pid}/editions/{edition}/patch");
    let read_route = format!("/api/projects/{pid}/editions/{edition}/workspace");
    let request = json!({"expectedRevisionId":null,"idempotencyKey":"insert","operations":[{"kind":"upsertPage","page":{"id":"page-a","title":"第一页","width":800,"height":1200,"readingDirection":"rightToLeft"}},{"kind":"upsertPanel","pageId":"page-a","panel":{"id":"panel-a","description":"雨夜","rect":{"x":0,"y":0,"width":800,"height":600},"assetRevisionId":null,"referenceAssetRevisionIds":[]}}],"readSet":[],"sources":[]});
    assert_ne!(
        call(&app, "POST", &patch_route, CANDIDATE, request.clone())
            .await
            .0,
        StatusCode::OK
    );
    let (status, saved) = call(&app, "POST", &patch_route, AUTHOR, request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["document"]["pages"][0]["panels"][0]["id"], "panel-a");
    assert!(saved["documentEncoding"].is_string());
    assert_eq!(
        call(&app, "POST", &patch_route, AUTHOR, request).await.1["revisionId"],
        saved["revisionId"]
    );
    let bad = json!({"expectedRevisionId":saved["revisionId"],"idempotencyKey":"failed","operations":[{"kind":"setTitle","title":"不能写"},{"kind":"removePanel","pageId":"page-a","panelId":"missing"}],"readSet":[],"sources":[]});
    assert_ne!(
        call(&app, "POST", &patch_route, AUTHOR, bad).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", &read_route, AUTHOR, Value::Null).await.1["revisionId"],
        saved["revisionId"]
    );
    let wrong_shape = json!({"expectedRevisionId":saved["revisionId"],"idempotencyKey":"untyped","operations":[{"kind":"upsertPage","page":{"id":"page-a","title":"","width":800,"height":1200,"readingDirection":"rightToLeft","panels":[]}}],"readSet":[],"sources":[]});
    assert_eq!(
        call(&app, "POST", &patch_route, AUTHOR, wrong_shape)
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let reopened = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    assert_eq!(
        call(&reopened, "GET", &read_route, AUTHOR, Value::Null)
            .await
            .1["revisionId"],
        saved["revisionId"]
    );
}
#[tokio::test]
async fn fixed_unicode_text_pages_and_exact_candidate_patch_share_the_current_authority() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text-http.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = repo
        .create_creative_project(
            &CreateCreativeProjectRequest {
                title: "文本".into(),
                logline: "".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "text-project".into(),
            },
            1,
        )
        .unwrap();
    let pid = ProjectId::parse(p.project.id.clone()).unwrap();
    let eid = p.active_edition_id.clone();
    let text = "中😀\n".repeat(6000);
    let doc=serde_json::from_value(json!({"kind":"comic","title":"文本","format":"pages","pages":[{"id":"page","title":"页","width":800,"height":800,"readingDirection":"leftToRight","panels":[{"id":"panel","description":"说明","rect":{"x":0,"y":0,"width":100,"height":100},"assetRevisionId":null,"referenceAssetRevisionIds":[],"locked":false,"balloons":[{"id":"balloon","text":text,"speaker":"甲","rect":{"x":0,"y":0,"width":40,"height":30},"kind":"speech"}]}]}]})).unwrap();
    let first = repo
        .save_multimedia_document(
            &pid,
            &eid,
            None,
            &SaveMultimediaDocument {
                expected_revision_id: None,
                idempotency_key: "doc".into(),
                document: doc,
                read_set: vec![],
                sources: vec![],
            },
        )
        .unwrap();
    repo.conversation_store().create_conversation(
        &pid,
        CreateConversationRequest {
            scope_id: Some(first.edition.scope_id.clone()),
            thread_id: "text-thread".into(),
            title: "改稿".into(),
            forked_from_id: None,
            last_turn_id: None,
        },
        1,
    )
    .unwrap();
    let task=repo.create_task(CreateTaskRequest{task_id:"text-task".into(),project_id:pid.clone(),thread_id:"text-thread".into(),operation:"advisory".into(),input_revision:first.revision,model:None,target_json:json!({"type":"edition","editionId":eid,"scopeId":first.edition.scope_id,"branchId":first.edition.branch_id,"revisionId":first.revision_id}).to_string(),input_manifest_json:json!({"authorIntent":"精确改稿"}).to_string(),capability_hash:text_hash(CAP),capability_expires_at:4_000_000_000}).unwrap();
    drop(repo);
    let app = router(RuntimeState::new(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).unwrap());
    let route = format!("/api/projects/{pid}/tasks/{}/media/workspace", task.task_id);
    let field =
        json!({"kind":"balloonText","pageId":"page","panelId":"panel","balloonId":"balloon"});
    let read = json!({"threadId":"text-thread","expectedRevision":first.revision,"action":"read","version":"working","section":"text","textField":field,"charOffset":0,"charLimit":8000});
    let (status, page) = call(&app, "POST", &route, CANDIDATE, read.clone()).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.to_string().len() <= 65536);
    assert_eq!(page["version"], "working");
    assert_eq!(page["textDigest"], text_hash(&text));
    let mut mixed = read.clone();
    mixed["offset"] = json!(0);
    assert_ne!(
        call(&app, "POST", &route, CANDIDATE, mixed).await.0,
        StatusCode::OK
    );
    let request = json!({"expectedRevisionId":first.revision_id,"idempotencyKey":"replace","operations":[{"kind":"replaceTextRanges","field":field,"expectedTextDigest":page["textDigest"],"edits":[{"startChar":1,"endChar":2,"expectedSliceDigest":text_hash("😀"),"replacement":"晴"}]}],"readSet":[{"source":page["returnedSource"],"freshness":"pinned"}],"sources":[]});
    let submit = json!({"threadId":"text-thread","expectedRevision":first.revision,"action":"patch","request":request});
    let (status, saved) = call(&app, "POST", &route, CANDIDATE, submit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(
        call(&app, "POST", &route, CANDIDATE, submit).await.1["revisionId"],
        saved["revisionId"]
    );
    let (status, old) = call(
        &app,
        "POST",
        &format!("/api/projects/{pid}/editions/{eid}/text-page"),
        AUTHOR,
        json!({"revisionId":first.revision_id,"field":field,"charOffset":0,"charLimit":3}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{old}");
    assert_eq!(old["text"], "中😀\n");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/projects/{pid}/editions/{eid}/text-page"),
            CANDIDATE,
            json!({"revisionId":saved["revisionId"],"field":field})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let(status,working)=call(&app,"POST",&route,CANDIDATE,json!({"threadId":"text-thread","expectedRevision":first.revision,"action":"read","version":"working","section":"text","textField":field,"charOffset":0,"charLimit":3})).await;
    assert_eq!(status, StatusCode::OK, "{working}");
    assert_eq!(working["text"], "中晴\n");
    let(status,reread)=call(&app,"POST",&route,CANDIDATE,json!({"threadId":"text-thread","expectedRevision":first.revision,"action":"source","source":working["returnedSource"],"section":"text","textField":field,"charLimit":3})).await;
    assert_eq!(status, StatusCode::OK, "{reread}");
    assert_eq!(reread["text"], "中晴\n");
    let mut bad = json!({"threadId":"text-thread","expectedRevision":first.revision,"action":"patch","request":{"expectedRevisionId":saved["revisionId"],"idempotencyKey":"bad-digest","operations":[{"kind":"setTitle","title":"不得写入"},{"kind":"replaceTextRanges","field":field,"expectedTextDigest":"0".repeat(64),"edits":[{"startChar":0,"endChar":1,"expectedSliceDigest":text_hash("中"),"replacement":"改"}]}],"readSet":[],"sources":[]}});
    assert_eq!(
        call(&app, "POST", &route, CANDIDATE, bad.clone()).await.0,
        StatusCode::CONFLICT
    );
    bad["request"]["idempotencyKey"] = json!("canon-forged");
    bad["request"]["operations"][1]["field"] = json!({"kind":"canonStatement","entryId":"rule"});
    assert_ne!(
        call(&app, "POST", &route, CANDIDATE, bad).await.0,
        StatusCode::OK
    );
    let reopened = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let (status, w) = call(
        &reopened,
        "GET",
        &format!("/api/projects/{pid}/editions/{eid}/workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w["revisionId"], saved["revisionId"]);
    assert_eq!(w["document"]["title"], "文本");
}

#[tokio::test]
async fn generated_output_uses_whole_manifest_pin_asset_and_seal_authority() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("operations.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let created = repo
        .create_creative_project(
            &CreateCreativeProjectRequest {
                title: "生成漫画".into(),
                logline: "".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "project".into(),
            },
            100,
        )
        .unwrap();
    let p = ProjectId::parse(created.project.id).unwrap();
    let e = &created.editions[0];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    repo.conversation_store().create_conversation(
        &p,
        CreateConversationRequest {
            scope_id: Some(e.scope_id.clone()),
            thread_id: "generation-thread".into(),
            title: "生成".into(),
            forked_from_id: None,
            last_turn_id: None,
        },
        now,
    )
    .unwrap();
    let w = repo.multimedia_workspace(&p, &e.id, None).unwrap();
    let task=repo.create_budgeted_root_task(CreateTaskRequest{task_id:"assigned".into(),project_id:p.clone(),thread_id:"generation-thread".into(),operation:"advisory".into(),input_revision:0,model:None,target_json:json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id,"mediaBudget":{"maxRequests":2,"unitLimits":{"images":2},"maxCostMinor":null,"currency":null,"expiresAt":(now+3600)*1000}}).to_string(),input_manifest_json:json!({"authorIntent":"生成一幅插图"}).to_string(),capability_hash:format!("{:x}",Sha256::digest(CAP.as_bytes())),capability_expires_at:now+3600},&InitialWorkBudget{authorization_id:"http-generation-budget".into(),author_message_id:"http-generation-author".into(),expected_base_snapshot_id:w.head_snapshot_id,expected_source_digest:w.source_digest,max_requests:Some(5),max_tokens:Some(10000),deadline_at:Some(now+3600)},now).unwrap();
    drop(repo);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let events = seen.clone();
    let host = axum::Router::new().route(
        "/media/execute",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, axum::Json(v): axum::Json<Value>| {
                let events = events.clone();
                async move {
                    assert_eq!(
                        headers.get("authorization").unwrap(),
                        "Bearer callback-secret-1234567890123456"
                    );
                    events.lock().unwrap().push(v);
                    axum::Json(json!({"state":"running"}))
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, host).await.unwrap() });
    let state = RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap()
        .with_media_host_callback(
            format!("http://127.0.0.1:{port}/media/execute"),
            "callback-secret-1234567890123456".into(),
        )
        .unwrap();
    let app = router(state);
    let route = format!("/api/projects/{p}/tasks/{}/media/operations", task.task_id);
    let prepare = json!({"threadId":task.thread_id,"expectedRevision":0,"action":"prepare","request":{"providerId":"configured-image","kind":"image.generate","parameters":{"prompt":"雨夜"},"idempotencyKey":"http-image"}});
    assert_eq!(
        call(&app, "POST", &route, AUTHOR, prepare.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, intent) = call(&app, "POST", &route, CANDIDATE, prepare).await;
    assert_eq!(status, StatusCode::OK, "{intent}");
    assert_eq!(intent["execution"]["state"], "running");
    assert_eq!(intent["state"], "prepared");
    assert_eq!(seen.lock().unwrap().len(), 1);
    let op = intent["operation"]["id"].as_str().unwrap();
    let operation = format!("/api/projects/{p}/media-operations/{op}");
    assert_eq!(
        call(&app, "GET", &operation, CANDIDATE, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{operation}/outputs"),
            AUTHOR,
            json!({"outputCount":1})
        )
        .await
        .0,
        StatusCode::OK
    );
    let missing = json!({"state":"ready","outputs":{"assetRevisionIds":[],"artifactRevisionId":null},"providerReceipt":null,"error":null});
    assert_ne!(
        call(&app, "POST", &format!("{operation}/seal"), AUTHOR, missing)
            .await
            .0,
        StatusCode::OK
    );
    let(status,asset)=call(&app,"POST",&format!("/api/projects/{p}/editions/{}/assets",e.id),AUTHOR,json!({"operationId":format!("{op}-output-0"),"idempotencyKey":format!("{op}-asset-0"),"asset":{"name":"generated.png","kind":"image","mimeType":"image/png","digest":"a".repeat(64),"bytes":60,"width":32,"height":32,"duration":null,"sampleRate":null,"channels":null},"sources":[]})).await;
    assert_eq!(status, StatusCode::CREATED, "{asset}");
    let(status,seal)=call(&app,"POST",&format!("{operation}/seal"),AUTHOR,json!({"state":"ready","outputs":{"assetRevisionIds":[asset["assetRevisionId"]],"artifactRevisionId":null},"providerReceipt":{"id":"real-test-receipt"},"error":null})).await;
    assert_eq!(status, StatusCode::OK, "{seal}");
    assert_eq!(seal["state"], "ready");
    assert_eq!(
        call(
            &app,
            "GET",
            &format!(
                "/api/projects/{p}/media-assets/{}",
                asset["assetRevisionId"].as_str().unwrap()
            ),
            AUTHOR,
            Value::Null
        )
        .await
        .1["digest"],
        "a".repeat(64)
    );
    server.abort();
}

#[tokio::test]
async fn author_source_catalog_is_snapshot_bound_and_not_a_candidate_grant() {
    let dir = tempfile::tempdir().unwrap();
    let app = router(RuntimeState::new(dir.path().join("catalog.sqlite"), AUTHOR.into(), CANDIDATE.into()).unwrap());
    let (_, p) = call(&app,"POST","/api/projects",AUTHOR,json!({"title":"原作","logline":"","medium":"comic","idempotencyKey":"catalog"})).await;
    let pid=p["project"]["id"].as_str().unwrap(); let eid=p["activeEditionId"].as_str().unwrap();
    let workspace_url=format!("/api/projects/{pid}/editions/{eid}/workspace");
    let url=format!("/api/projects/{pid}/editions/{eid}/source-catalog");
    let (status,empty)=call(&app,"GET",&url,AUTHOR,Value::Null).await;
    assert_eq!(status,StatusCode::OK); assert_eq!(empty["total"],0);
    assert_eq!(call(&app,"GET",&url,CANDIDATE,Value::Null).await.0,StatusCode::UNAUTHORIZED);
    let (_,w)=call(&app,"GET",&workspace_url,AUTHOR,Value::Null).await;
    let (status,saved)=call(&app,"POST",&workspace_url,AUTHOR,json!({"expectedRevisionId":null,"document":w["document"],"idempotencyKey":"catalog-save","readSet":[],"sources":[]})).await;
    assert_eq!(status,StatusCode::OK,"{saved}");
    let (status,page)=call(&app,"GET",&url,AUTHOR,Value::Null).await;
    assert_eq!(status,StatusCode::OK); assert_eq!(page["items"][0]["source"],saved["documentSource"]);
    let pinned=format!("{url}?snapshotId={}",empty["snapshotId"].as_str().unwrap());
    assert_eq!(call(&app,"GET",&pinned,AUTHOR,Value::Null).await.1["total"],0);
    for query in ["offset=1", "snapshotId=missing", "limit=101", "unexpected=true"] {
        assert!(!call(&app,"GET",&format!("{url}?{query}"),AUTHOR,Value::Null).await.0.is_success());
    }
}

#[tokio::test]
async fn author_can_replace_future_adaptation_source_without_changing_historical_content() {
    let dir=tempfile::tempdir().unwrap();
    let app=router(RuntimeState::new(dir.path().join("rebind.sqlite"),AUTHOR.into(),CANDIDATE.into()).unwrap());
    let (_,p)=call(&app,"POST","/api/projects",AUTHOR,json!({"title":"原作","logline":"","medium":"comic","idempotencyKey":"source-project"})).await;
    let pid=p["project"]["id"].as_str().unwrap();let eid=p["activeEditionId"].as_str().unwrap();
    let url=format!("/api/projects/{pid}/editions/{eid}/workspace");
    let (_,w)=call(&app,"GET",&url,AUTHOR,Value::Null).await;
    let (_,first)=call(&app,"POST",&url,AUTHOR,json!({"expectedRevisionId":null,"document":w["document"],"idempotencyKey":"original","sources":[],"readSet":[]})).await;
    let previous=first["documentSource"].clone();
    let (_,edition)=call(&app,"POST",&format!("/api/projects/{pid}/editions"),AUTHOR,json!({"title":"改编","medium":"animation","idempotencyKey":"adaptation","sources":[previous]})).await;
    let adapted=edition["id"].as_str().unwrap();
    let (_,second)=call(&app,"POST",&format!("/api/projects/{pid}/editions/{eid}/patch"),AUTHOR,json!({"expectedRevisionId":first["revisionId"],"idempotencyKey":"new-original","operations":[{"kind":"setTitle","title":"新版原作"}],"readSet":[],"sources":[]})).await;
    let next=second["documentSource"].clone();
    let request=json!({"expectedRevisionId":null,"idempotencyKey":"source-replace","operations":[{"kind":"replaceEditionSource","previous":previous,"next":next}],"readSet":[],"sources":[]});
    let patch_url=format!("/api/projects/{pid}/editions/{adapted}/patch");
    assert_eq!(call(&app,"POST",&patch_url,CANDIDATE,request.clone()).await.0,StatusCode::UNAUTHORIZED);
    let (status,saved)=call(&app,"POST",&patch_url,AUTHOR,request.clone()).await;
    assert_eq!(status,StatusCode::OK,"{saved}");assert_eq!(saved["edition"]["sources"],json!([next]));
    assert!(saved["document"]["shots"].as_array().unwrap().is_empty());
    assert_eq!(call(&app,"POST",&patch_url,AUTHOR,request).await.1["revisionId"],saved["revisionId"]);
    let (_,catalog)=call(&app,"GET",&format!("/api/projects/{pid}/editions"),AUTHOR,Value::Null).await;
    assert_eq!(catalog["editions"].as_array().unwrap().iter().find(|e|e["id"]==adapted).unwrap()["sources"],saved["edition"]["sources"]);
}

use novel_codex_story_core::{InitialWorkBudgetRepository, MultimediaRepository, TasksRepository};
