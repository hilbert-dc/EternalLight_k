use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[path = "support/author_task_fixture.rs"]
mod fixture;
use fixture::AuthorTaskFixture;

enum Actor {
    Author,
    Candidate,
}
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    actor: Actor,
    body: Value,
) -> (StatusCode, Value) {
    let token = match actor {
        Actor::Author => "catalog-author-token",
        Actor::Candidate => "catalog-candidate-token",
    };
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"rejected":true})),
    )
}
async fn project(app: &Router) -> String {
    let (status, value) = call(
        app,
        "POST",
        "/api/projects",
        Actor::Author,
        json!({"title":"目录验收","logline":"独立分卷"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value["project"]["id"].as_str().unwrap().into()
}
async fn read(app: &Router, path: &str) -> Value {
    let (status, value) = call(app, "GET", path, Actor::Author, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
fn volume(title: &str) -> Value {
    json!({"type":"createVolume","title":title})
}
async fn write(app: &Router, base: &str, revision: i64, key: &str, operation: Value) -> Value {
    let (status, value) = call(
        app,
        "POST",
        &format!("{base}/catalog"),
        Actor::Author,
        json!({"expectedRevision":revision,"idempotencyKey":key,"operation":operation}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

#[tokio::test]
async fn author_catalog_preserves_planning_authority_and_atomic_volume_chapter_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.sqlite3");
    let app = router(fixture::state(
        path.clone(),
        "catalog-author-token".into(),
        "catalog-candidate-token".into(),
    ));
    let base = format!("/api/projects/{}", project(&app).await);
    assert_eq!(
        read(&app, &format!("{base}/catalog")).await,
        json!({"revision":0,"volumes":[],"placements":[]})
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_catalogs_v1", [], |row| row
            .get::<_, i64>(/*idx*/ 0))
            .unwrap(),
        0
    );
    drop(db);
    let nodes = json!([{"clientKey":"first-volume","kind":"volume","position":0,"title":"启程","purpose":"收到来信","summary":"离开港城","writerGuidance":"先保留谜团"},
        {"id":"second-volume","kind":"volume","position":1,"title":"启程","purpose":"另一个同名卷","summary":"","writerGuidance":""}]);
    let (_,outline) = call(&app,"POST",&format!("{base}/artifacts/outline"),Actor::Author,
        json!({"payload":{"nodes":nodes},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"})).await;
    let before = read(&app, &format!("{base}/creative-workspace")).await;
    let source = json!({"outlineRevisionId":outline["revisionId"],"nodeId":"first-volume"});
    let first = write(
        &app,
        &base,
        /*revision*/ 0,
        "reuse-volume",
        json!({"type":"createVolume","title":"不能代替来源","source":source}),
    )
    .await;
    assert_eq!(
        first["catalog"],
        json!({"revision":1,"volumes":[{"id":first["volumeId"],"title":"启程","position":0,"source":source,"outlineStatus":"linked"}],"placements":[]})
    );
    assert_eq!(
        read(&app, &format!("{base}/creative-workspace")).await,
        before
    );
    let (status,_) = call(&app,"POST",&format!("{base}/artifacts/outline"),Actor::Author,json!({"payload":{"nodes":[
        {"clientKey":"first-volume","kind":"volume","position":0,"title":"新版卷纲","purpose":"新的安排","summary":"","writerGuidance":""}]},
        "expectedRevision":0,"expectedCandidateRevisionId":outline["revisionId"],"idempotencyKey":"outline-new"})).await;
    assert_eq!(status, StatusCode::CREATED);
    let mut current_catalog = first["catalog"].clone();
    current_catalog["volumes"][0]["title"] = json!("新版卷纲");
    assert_eq!(
        read(&app, &format!("{base}/catalog")).await,
        current_catalog
    );
    let (status, error) = call(
        &app,
        "POST",
        &format!("{base}/catalog"),
        Actor::Author,
        json!({"expectedRevision":1,"idempotencyKey":"old-source",
        "operation":{"type":"createVolume","title":"x","source":source}}),
    )
    .await;
    assert_eq!(
        (status, error["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("catalog_source_changed"))
    );
    let planning = read(&app, &format!("{base}/creative-workspace")).await;
    let second = write(
        &app,
        &base,
        /*revision*/ 1,
        "manual-volume",
        volume("启程"),
    )
    .await;
    let chapter_request =
        json!({"title":"独立章","expectedRevision":0,"idempotencyKey":"standalone"});
    let (status, chapter) = call(
        &app,
        "POST",
        &format!("{base}/chapters/materialize-standalone"),
        Actor::Author,
        chapter_request,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chapter}");
    let chapter_id = &chapter["chapterId"];
    let first_id = &first["volumeId"];
    let second_id = &second["volumeId"];
    let placed = write(
        &app,
        &base,
        /*revision*/ 2,
        "place",
        json!({"type":"placeChapter","chapterId":chapter_id,"volumeId":first_id}),
    )
    .await;
    assert_eq!(
        placed["catalog"]["placements"],
        json!([{"chapterId":chapter_id,"volumeId":first_id}])
    );
    let moved = write(
        &app,
        &base,
        /*revision*/ 3,
        "move",
        json!({"type":"placeChapter","chapterId":chapter_id,"volumeId":second_id}),
    )
    .await;
    assert_eq!(
        moved["catalog"]["placements"],
        json!([{"chapterId":chapter_id,"volumeId":second_id}])
    );
    let unplaced = write(
        &app,
        &base,
        /*revision*/ 4,
        "unplace",
        json!({"type":"placeChapter","chapterId":chapter_id,"volumeId":null}),
    )
    .await;
    assert_eq!(unplaced["catalog"]["placements"], json!([]));
    let other = format!("/api/projects/{}", project(&app).await);
    let foreign = write(
        &app,
        &other,
        /*revision*/ 0,
        "foreign",
        volume("别作品卷"),
    )
    .await;
    let current = read(&app, &format!("{base}/creative-workspace")).await;
    let bad = json!({"title":"不能留下半章","expectedRevision":1,"volumeId":foreign["volumeId"],"idempotencyKey":"wrong-volume"});
    let (status, _) = call(
        &app,
        "POST",
        &format!("{base}/chapters/materialize-standalone"),
        Actor::Author,
        bad,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        read(&app, &format!("{base}/creative-workspace")).await,
        current
    );
    assert_eq!(
        read(&app, &format!("{base}/catalog")).await,
        unplaced["catalog"]
    );
    let request = json!({"title":"","expectedRevision":1,"volumeId":first_id,"idempotencyKey":"volume-chapter"});
    let saved = call(
        &app,
        "POST",
        &format!("{base}/chapters/materialize-standalone"),
        Actor::Author,
        request.clone(),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK, "{}", saved.1);
    assert_eq!(
        (
            saved.1["catalogRevision"].clone(),
            saved.1["volumeId"].clone()
        ),
        (json!(6), first_id.clone())
    );
    let catalog = write(
        &app,
        &base,
        /*revision*/ 6,
        "later-volume",
        volume("后续卷"),
    )
    .await;
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/chapters/materialize-standalone"),
            Actor::Author,
            request
        )
        .await,
        saved
    );
    assert_eq!(
        read(&app, &format!("{base}/catalog")).await,
        catalog["catalog"]
    );
    assert_eq!(
        write(
            &app,
            &base,
            /*revision*/ 0,
            "reuse-volume",
            json!({"type":"createVolume","title":"不能代替来源","source":source})
        )
        .await,
        first
    );
    assert_eq!(
        read(&app, &format!("{base}/catalog")).await,
        catalog["catalog"]
    );
    let final_state = read(&app, &format!("{base}/creative-workspace")).await;
    assert_eq!(
        (
            final_state["artifacts"].clone(),
            final_state["chapterPlanBindings"].clone(),
            final_state["commits"].clone()
        ),
        (
            planning["artifacts"].clone(),
            json!([]),
            before["commits"].clone()
        )
    );
    assert_eq!(final_state["chapters"].as_array().unwrap().len(), 2);
    assert_eq!(
        final_state["chapters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|chapter| json!({"id":chapter["id"],"number":chapter["number"],"title":chapter["title"]}))
            .collect::<Vec<_>>(),
        vec![
            json!({"id":chapter_id,"number":1,"title":"独立章"}),
            json!({"id":saved.1["chapterId"],"number":saved.1["chapterNumber"],"title":"第 2 章"}),
        ]
    );
    assert!(
        final_state["chapters"]
            .as_array()
            .unwrap()
            .iter()
            .all(|chapter| chapter["candidate"].is_null() && chapter["accepted"].is_null())
    );
}

#[tokio::test]
async fn catalog_rejects_stale_cross_project_source_candidate_and_active_task_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog-guards.sqlite3");
    let app = router(fixture::state(
        path.clone(),
        "catalog-author-token".into(),
        "catalog-candidate-token".into(),
    ));
    let base = format!("/api/projects/{}", project(&app).await);
    let catalog_path = format!("{base}/catalog");
    let first = write(&app, &base, /*revision*/ 0, "first", volume("手写卷")).await;
    let body = json!({"expectedRevision":0,"idempotencyKey":"first","operation":volume("手写卷")});
    assert_eq!(
        call(&app, "POST", &catalog_path, Actor::Author, body.clone()).await,
        (StatusCode::OK, first.clone())
    );
    let mut changed = body.clone();
    changed["operation"]["title"] = json!("换参数");
    assert_eq!(
        call(&app, "POST", &catalog_path, Actor::Author, changed)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut stale = body;
    stale["idempotencyKey"] = json!("stale");
    assert_eq!(
        call(&app, "POST", &catalog_path, Actor::Author, stale.clone())
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(&app, "POST", &catalog_path, Actor::Candidate, stale)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "GET", &catalog_path, Actor::Candidate, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for payload in [
        json!({"expectedRevision":1,"idempotencyKey":"bad","operation":{"type":"placeChapter","chapterId":"foreign","volumeId":first["volumeId"]}}),
        json!({"expectedRevision":1,"idempotencyKey":"missing-destination","operation":{"type":"placeChapter","chapterId":"foreign"}}),
        json!({"expectedRevision":1,"idempotencyKey":"bad-source","operation":{"type":"createVolume","title":"x","source":{"outlineRevisionId":"foreign","nodeId":"node"}}}),
    ] {
        let status = call(&app, "POST", &catalog_path, Actor::Author, payload)
            .await
            .0;
        assert!(status.is_client_error());
        assert_eq!(read(&app, &catalog_path).await, first["catalog"]);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,task) = call(&app,"POST",&format!("{base}/tasks"),Actor::Author,json!({"operation":"advisory","inputRevision":0,"threadId":"conversation","target":{"type":"project"},
        "authorIntent":"只讨论","capabilityHash":"a".repeat(64),"capabilityExpiresAt":now+900})).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let blocked = call(
        &app,
        "POST",
        &catalog_path,
        Actor::Author,
        json!({"expectedRevision":1,"idempotencyKey":"blocked","operation":volume("不能新增")}),
    )
    .await;
    assert_eq!(
        (blocked.0, blocked.1["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("catalog_task_active"))
    );
    assert_eq!(read(&app, &catalog_path).await, first["catalog"]);
    let db = rusqlite::Connection::open(path).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM story_revisions_v2", [], |row| {
            row.get(/*idx*/ 0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn catalog_follows_only_exact_current_outline_identity_and_author_relinks_without_formal_writes()
 {
    let directory = tempfile::tempdir().unwrap();
    let app = router(fixture::state(
        directory.path().join("single-source.sqlite3"),
        "catalog-author-token".into(),
        "catalog-candidate-token".into(),
    ));
    let base = format!("/api/projects/{}", project(&app).await);
    let path = format!("{base}/catalog");
    let (status, saved) = call(&app, "POST", &format!("{base}/artifacts/outline"), Actor::Author,
        json!({"payload":{"nodes":[{"clientKey":"bound-node","kind":"volume","title":"原卷名","purpose":"唯一正文来源"}]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline-original"})).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, review) = call(&app, "POST", &format!("{base}/reviews"), Actor::Author,
        json!({"artifactKind":"outline","revisionId":saved["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"accept-source-review"})).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(
        &app,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        Actor::Author,
        json!({"expectedRevision":0,"idempotencyKey":"accept-source","selections":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let source = json!({"outlineRevisionId":saved["revisionId"],"nodeId":"bound-node"});
    let original = write(
        &app,
        &base,
        /*revision*/ 0,
        "bind-volume",
        json!({"type":"createVolume","title":"目录备用名","source":source}),
    )
    .await;
    let id = original["volumeId"].clone();
    let duplicate = call(&app, "POST", &path, Actor::Author, json!({"expectedRevision":1,"idempotencyKey":"duplicate", "operation":{"type":"createVolume","title":"同名卷","source":source}})).await;
    assert_eq!(
        (duplicate.0, duplicate.1["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("catalog_node_already_linked"))
    );
    assert_eq!(read(&app, &path).await, original["catalog"]);

    let mut previous = read(&app, &format!("{base}/creative-workspace")).await["artifacts"]["outline"]["candidate"]["id"].clone();
    for (index, nodes, expected_title, expected_status) in [
        (
            0,
            json!([{"clientKey":"bound-node","kind":"volume","title":"新卷名","summary":"当前唯一故事安排"}]),
            "新卷名",
            "linked",
        ),
        (
            1,
            json!([{"id":"other-node","kind":"volume","title":"原卷名"}]),
            "原卷名",
            "unavailable",
        ),
        (
            2,
            json!([{"id":"bound-node","kind":"volume","title":"重复一"},{"clientKey":"bound-node","kind":"volume","title":"重复二"}]),
            "原卷名",
            "unavailable",
        ),
        (
            3,
            json!([{"id":"bound-node","kind":"unit","title":"错误类型"}]),
            "原卷名",
            "unavailable",
        ),
        (
            4,
            json!([{"id":"replacement-node","kind":"volume","title":"原卷名"}]),
            "原卷名",
            "unavailable",
        ),
    ] {
        let (status, saved) = call(&app, "POST", &format!("{base}/artifacts/outline"), Actor::Author,
            json!({"payload":{"nodes":nodes},"expectedRevision":1,"expectedCandidateRevisionId":previous,"idempotencyKey":format!("outline-{index}")})).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        previous = saved["revisionId"].clone();
        let mut expected = original["catalog"].clone();
        expected["volumes"][0]["title"] = json!(expected_title);
        expected["volumes"][0]["outlineStatus"] = json!(expected_status);
        assert_eq!(read(&app, &path).await, expected);
        if index == 1 {
            let workspace_before = read(&app, &format!("{base}/creative-workspace")).await;
            for (key, operation) in [
                (
                    "old-accepted-create",
                    json!({"type":"createVolume","title":"不能关联旧稿","source":source}),
                ),
                (
                    "old-accepted-link",
                    json!({"type":"linkOutline","volumeId":id,"source":source}),
                ),
            ] {
                let rejected = call(
                    &app,
                    "POST",
                    &path,
                    Actor::Author,
                    json!({"expectedRevision":1,"idempotencyKey":key,"operation":operation}),
                )
                .await;
                assert_eq!(
                    (rejected.0, rejected.1["error"]["code"].clone()),
                    (StatusCode::CONFLICT, json!("catalog_source_changed"))
                );
                assert_eq!(read(&app, &path).await, expected);
            }
            assert_eq!(
                read(&app, &format!("{base}/creative-workspace")).await,
                workspace_before
            );
        }
    }
    let independent = write(
        &app,
        &base,
        /*revision*/ 1,
        "independent",
        volume("独立卷"),
    )
    .await;
    assert_eq!(
        independent["catalog"]["volumes"][1],
        json!({"id":independent["volumeId"],"title":"独立卷","position":1,"source":null,"outlineStatus":"independent"})
    );
    let before = read(&app, &format!("{base}/creative-workspace")).await;
    let new_source = json!({"outlineRevisionId":previous,"nodeId":"replacement-node"});
    let operation = json!({"type":"linkOutline","volumeId":id,"source":new_source});
    let linked = write(
        &app,
        &base,
        /*revision*/ 2,
        "relink-once",
        operation.clone(),
    )
    .await;
    assert_eq!(
        linked["catalog"]["volumes"][0],
        json!({"id":id,"title":"原卷名","position":0,"source":new_source,"outlineStatus":"linked"})
    );
    let other_link =
        json!({"type":"linkOutline","volumeId":independent["volumeId"],"source":new_source});
    let rejected = call(
        &app,
        "POST",
        &path,
        Actor::Author,
        json!({"expectedRevision":3,"idempotencyKey":"other-link","operation":other_link}),
    )
    .await;
    assert_eq!(
        (rejected.0, rejected.1["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("catalog_node_already_linked"))
    );
    assert_eq!(read(&app, &path).await, linked["catalog"]);
    let other = format!("/api/projects/{}", project(&app).await);
    let foreign = call(&app, "POST", &format!("{other}/catalog"), Actor::Author,
        json!({"expectedRevision":0,"idempotencyKey":"foreign-source","operation":{"type":"createVolume","title":"不能跨作品","source":new_source}})).await;
    assert_eq!(foreign.0, StatusCode::CONFLICT);
    let foreign_volume = call(
        &app,
        "POST",
        &format!("{other}/catalog"),
        Actor::Author,
        json!({"expectedRevision":0,"idempotencyKey":"foreign-volume","operation":operation}),
    )
    .await;
    assert_eq!(foreign_volume.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        read(&app, &format!("{other}/catalog")).await,
        json!({"revision":0,"volumes":[],"placements":[]})
    );
    let latest = write(
        &app,
        &base,
        /*revision*/ 3,
        "another-volume",
        volume("后续独立卷"),
    )
    .await;
    assert_eq!(
        write(
            &app,
            &base,
            /*revision*/ 2,
            "relink-once",
            operation.clone()
        )
        .await,
        linked
    );
    let stale = call(
        &app,
        "POST",
        &path,
        Actor::Author,
        json!({"expectedRevision":2,"idempotencyKey":"stale-link","operation":operation}),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    let denied = call(
        &app,
        "POST",
        &path,
        Actor::Candidate,
        json!({"expectedRevision":4,"idempotencyKey":"candidate-link","operation":operation}),
    )
    .await;
    assert_eq!(denied.0, StatusCode::UNAUTHORIZED);
    assert_eq!(read(&app, &path).await, latest["catalog"]);
    assert_eq!(
        read(&app, &format!("{base}/creative-workspace")).await,
        before
    );
}
