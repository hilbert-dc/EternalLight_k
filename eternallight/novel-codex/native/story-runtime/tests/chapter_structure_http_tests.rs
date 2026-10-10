use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
#[path = "support/settle_task_fixture.rs"]
mod settle;

const AUTHOR: &str = "structure-author";
const CANDIDATE: &str = "structure-candidate";
const CAP: &str = "structure-secret-capability";
const THREAD: &str = "structure-thread";
const BODY: &str = "前🙂句。灯塔的钥匙交给林澄。\n最后一句。";
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    actor: &str,
    body: Value,
    tool: &str,
    caller: &str,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {actor}"))
        .header("content-type", "application/json")
        .header("x-novel-task-capability", CAP);
    if !tool.is_empty() {
        req = req
            .header("x-novel-tool-name", tool)
            .header("x-novel-root-thread", THREAD)
            .header("x-novel-caller-thread", caller)
            .header("x-novel-turn", "turn")
            .header(
                "x-novel-tool-call",
                format!(
                    "call-{:x}",
                    Sha256::digest(json!([path, tool, body]).to_string().as_bytes())
                ),
            )
            .header(
                "x-novel-request-hash",
                format!("{:x}", Sha256::digest(body.to_string().as_bytes())),
            );
    }
    let response = app
        .clone()
        .oneshot(
            req.body(if body.is_null() {
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
        serde_json::from_slice(&bytes).unwrap_or(json!({"nonJson":true})),
    )
}
async fn author(app: &Router, method: &str, path: &str, body: Value) -> Value {
    let (status, v) = call(app, method, path, AUTHOR, body, "", "").await;
    assert!(status.is_success(), "{method} {path}: {status}: {v}");
    v
}
async fn workspace(app: &Router, root: &str) -> Value {
    author(
        app,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
    )
    .await
}
async fn make_chapter(app: &Router, root: &str, key: &str) -> Value {
    let ws = workspace(app, root).await;
    author(
        app,
        "POST",
        &format!("{root}/chapters/materialize-standalone"),
        json!({"title":key,"expectedRevision":ws["revision"],"idempotencyKey":key}),
    )
    .await
}
async fn save(
    app: &Router,
    root: &str,
    chapter: &Value,
    previous: Value,
    key: &str,
    body: &str,
) -> Value {
    let ws = workspace(app, root).await;
    author(app,"POST",&format!("{root}/drafts/checkpoints"),json!({"payload":{"chapterId":chapter["chapterId"],"chapterNumber":chapter["chapterNumber"],"title":"精确来源","body":body,"sceneGroups":[]},"expectedRevision":ws["revision"],"expectedCandidateRevisionId":previous,"idempotencyKey":key})).await
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn task_request(ws: &Value, event: &Value, key: &str) -> Value {
    json!({"operation":"assessChapterStructure","inputRevision":ws["revision"],"threadId":THREAD,"target":{"type":"project","structureChangeId":event["id"]},"authorIntent":"删除后自动只读检查，不修改故事","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now()+600,
    "initialBudget":{"authorizationId":format!("authorization-{key}"),"authorMessageId":format!("message-{key}"),"expectedBaseSnapshotId":ws["headSnapshotId"],"expectedSourceDigest":ws["sourceDigest"],"maxRequests":3,"maxTokens":2000,"deadlineAt":now()+300}})
}
fn source_request(task: &Value, role: &str, cursor: Value) -> Value {
    json!({"threadId":THREAD,"expectedRevision":task["inputRevision"],"sourceRole":role,"cursor":cursor,"limit":5})
}
async fn read_task_source(app: &Router, uri: &str, entry: &Value) -> Value {
    let mut cursor = String::new();
    let mut payload = String::new();
    loop {
        let (status, page) = call(
            app,
            "GET",
            &format!("{uri}{cursor}"),
            CANDIDATE,
            Value::Null,
            "read_task_source",
            THREAD,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], entry["sourceId"]);
        assert_eq!(page["sectionPath"], entry["sectionPath"]);
        assert_eq!(page["contentHash"], entry["contentHash"]);
        assert_eq!(page["payloadEncoding"], "jsonUtf8");
        assert_eq!(page["byteRange"]["start"], payload.len());
        assert_eq!(page["byteRange"]["total"], entry["payloadBytes"]);
        let part = page["payloadPart"].as_str().unwrap();
        assert!(!part.is_empty() && part.len() <= 2000);
        assert!(page.to_string().len() <= 6500);
        payload.push_str(part);
        assert_eq!(page["byteRange"]["end"], payload.len());
        if page["nextCursor"].is_null() {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(entry["payloadBytes"], payload.len());
            break;
        }
        assert_eq!(page["endOfInputFromCursor"], false);
        cursor = format!("&cursor={}", page["nextCursor"].as_str().unwrap());
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(payload.as_bytes())),
        entry["contentHash"].as_str().unwrap()
    );
    serde_json::from_str(&payload).unwrap()
}
fn counts(path: &std::path::Path) -> (i64, i64, i64) {
    let db = rusqlite::Connection::open(path).unwrap();
    (
        db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| {
            r.get(/*idx*/ 0)
        })
        .unwrap(),
        db.query_row("SELECT COUNT(*) FROM story_work_budgets_v1", [], |r| {
            r.get(/*idx*/ 0)
        })
        .unwrap(),
        db.query_row("SELECT COUNT(*) FROM story_task_outputs_v2", [], |r| {
            r.get(/*idx*/ 0)
        })
        .unwrap(),
    )
}
#[tokio::test]
async fn deletion_captures_unpublished_and_published_sources_and_funds_one_readonly_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("structure.sqlite3");
    let app = novel_codex_story_runtime::router(
        novel_codex_story_runtime::RuntimeState::new(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    );
    let project = author(&app, "POST", "/api/projects", json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"结构变化"})).await;
    let root = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    let chapter = make_chapter(&app, &root, "first").await;
    let accepted = save(
        &app,
        &root,
        &chapter,
        Value::Null,
        "old-save",
        "已发布旧句。",
    )
    .await;
    let object = format!("draft-{}", chapter["chapterId"].as_str().unwrap());
    let review=author(&app,"POST",&format!("{root}/reviews"),json!({"artifactKind":"draft","objectId":object,"revisionId":accepted["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"review"})).await;
    let ws = workspace(&app, &root).await;
    author(&app,"POST",&format!("{root}/reviews/{}/accept",review["id"].as_str().unwrap()),json!({"expectedRevision":ws["revision"],"selections":{"noConfirmedChanges":true},"idempotencyKey":"accept"})).await;
    author(&app,"PUT",&format!("{root}/working-baselines/draft"),json!({"objectId":chapter["chapterId"],"revisionId":accepted["revisionId"],"expectedSelectionRevision":0})).await;
    let saved = save(
        &app,
        &root,
        &chapter,
        accepted["revisionId"].clone(),
        "new-save",
        BODY,
    )
    .await;
    let selected = workspace(&app, &root).await;
    let baseline = selected["workingBaselines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["objectId"] == chapter["chapterId"])
        .unwrap();
    author(&app,"PUT",&format!("{root}/working-baselines/draft"),json!({"objectId":chapter["chapterId"],"revisionId":accepted["revisionId"],"expectedSelectionRevision":baseline["selectionRevision"]})).await;
    let remaining = make_chapter(&app, &root, "remaining").await;
    let retained = save(
        &app,
        &root,
        &remaining,
        Value::Null,
        "retained",
        "林澄仍记得那把钥匙。",
    )
    .await;
    let ws = workspace(&app, &root).await;
    let delete_input = json!({"expectedRevision":ws["revision"],"idempotencyKey":"delete"});
    let delete_uri = format!(
        "{root}/chapters/{}/delete",
        chapter["chapterId"].as_str().unwrap()
    );
    let receipt = author(&app, "POST", &delete_uri, delete_input.clone()).await;
    assert_eq!(
        author(&app, "POST", &delete_uri, delete_input).await,
        receipt
    );
    let event_uri = format!(
        "{root}/chapter-structure-changes/{}",
        receipt["structureChangeId"].as_str().unwrap()
    );
    let event = author(&app, "GET", &event_uri, Value::Null).await;
    assert_eq!(
        event["source"],
        json!({"objectId":object,"savedRevisionId":saved["revisionId"],"acceptedRevisionId":accepted["revisionId"],"workingRevisionId":accepted["revisionId"],"knowledgeBatchId":null})
    );
    assert_eq!(
        event["assessment"],
        json!({"state":"pending","skipReason":null,"taskId":null,"workTaskId":null})
    );
    assert_eq!(
        event["nextChapter"],
        json!({"id":remaining["chapterId"],"number":2})
    );
    let ws = workspace(&app, &root).await;
    let request = task_request(&ws, &event, "one");
    let mut wrong = request.clone();
    wrong["target"]["structureChangeId"] = json!("foreign-event");
    assert!(
        !call(
            &app,
            "POST",
            &format!("{root}/tasks"),
            AUTHOR,
            wrong,
            "",
            ""
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(counts(&path), (0, 0, 0));
    let task = author(&app, "POST", &format!("{root}/tasks"), request.clone()).await;
    assert_eq!(
        author(&app, "POST", &format!("{root}/tasks"), request.clone()).await,
        task
    );
    let mut other = request.clone();
    other["initialBudget"]["authorizationId"] = json!("other-authorization");
    other["initialBudget"]["authorMessageId"] = json!("other-message");
    assert!(
        !call(
            &app,
            "POST",
            &format!("{root}/tasks"),
            AUTHOR,
            other,
            "",
            ""
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(counts(&path), (1, 1, 0));
    let bound = author(&app, "GET", &event_uri, Value::Null).await;
    assert_eq!(bound["assessment"]["taskId"], task["id"]);
    let pending = author(
        &app,
        "GET",
        &format!("{root}/chapter-structure-changes?assessmentState=pending"),
        Value::Null,
    )
    .await;
    assert_eq!(pending["items"], json!([]));
    let task_uri = format!("{root}/tasks/{}", task["id"].as_str().unwrap());
    let source_uri = format!("{task_uri}/chapter-structure-source");
    let context_uri = format!(
        "{root}/context?purpose=critic&taskId={}&threadId={THREAD}&projectRevision={}",
        task["id"].as_str().unwrap(),
        ws["revision"]
    );
    let (status, context) = call(
        &app,
        "GET",
        &context_uri,
        CANDIDATE,
        Value::Null,
        "read_context",
        THREAD,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let metadata_entry = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == "/operationContext/chapterStructureChange")
        .unwrap();
    assert_eq!(
        context["operationContext"]["chapterStructureChange"],
        json!({"sourceId":metadata_entry["sourceId"],"textDelivered":false})
    );
    let metadata_uri = format!(
        "{task_uri}/task-source?threadId={THREAD}&expectedRevision={}&sourceId={}",
        task["inputRevision"],
        metadata_entry["sourceId"].as_str().unwrap()
    );
    for (tool, caller) in [
        ("read_source_content", THREAD),
        ("read_task_source", "child"),
    ] {
        assert!(
            !call(
                &app,
                "GET",
                &metadata_uri,
                CANDIDATE,
                Value::Null,
                tool,
                caller
            )
            .await
            .0
            .is_success()
        );
    }
    let metadata = read_task_source(&app, &metadata_uri, metadata_entry).await;
    let mut expected_metadata = event.clone();
    expected_metadata
        .as_object_mut()
        .unwrap()
        .remove("assessment");
    expected_metadata["authority"] = json!("removedHistoricalSource");
    expected_metadata["coverage"] =
        json!({"globalImpactComplete":false,"sourceTextDelivered":false});
    assert_eq!(metadata, expected_metadata);
    let mut cursor = Value::Null;
    let mut text = String::new();
    let mut first = Value::Null;
    loop {
        let (status, page) = call(
            &app,
            "POST",
            &source_uri,
            CANDIDATE,
            source_request(&task, "saved", cursor),
            "read_chapter_structure_source",
            THREAD,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["start"], text.chars().count());
        if first.is_null() {
            first = page.clone();
        }
        text.push_str(page["quote"].as_str().unwrap());
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(text, BODY);
    let(status,old)=call(&app,"POST",&source_uri,CANDIDATE,json!({"threadId":THREAD,"expectedRevision":ws["revision"],"sourceRole":"working","limit":1000}),"read_chapter_structure_source",THREAD).await;
    assert_eq!(status, StatusCode::OK, "{old}");
    assert_eq!(old["quote"], "已发布旧句。");
    for bad in [
        source_request(&task, "working", first["nextCursor"].clone()),
        source_request(&task, "saved", json!("5.bad-proof")),
    ] {
        assert!(
            !call(
                &app,
                "POST",
                &source_uri,
                CANDIDATE,
                bad,
                "read_chapter_structure_source",
                THREAD
            )
            .await
            .0
            .is_success()
        );
    }
    assert!(
        !call(
            &app,
            "POST",
            &source_uri,
            CANDIDATE,
            source_request(&task, "saved", Value::Null),
            "read_chapter_structure_source",
            "child"
        )
        .await
        .0
        .is_success()
    );
    let evidence = json!({"threadId":THREAD,"expectedRevision":ws["revision"],"sourceObjectId":format!("draft-{}",remaining["chapterId"].as_str().unwrap()),"sourceRevisionId":retained["revisionId"],"quote":"林澄"});
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{task_uri}/source-evidence"),
            CANDIDATE,
            evidence,
            "resolve_story_evidence",
            THREAD
        )
        .await
        .0,
        StatusCode::OK
    );
    let impact = json!({"threadId":THREAD,"expectedRevision":ws["revision"],"entityId":"unknown-entity","limit":1,"maxHops":1});
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{task_uri}/impact-analysis"),
            CANDIDATE,
            impact,
            "analyze_story_impact",
            THREAD
        )
        .await
        .0,
        StatusCode::OK
    );
    let query = json!({"threadId":THREAD,"expectedRevision":ws["revision"],"plan":{"question":"剩余章节人物状态","byteBudget":4000,"ranking":{"type":"literal"},"steps":[{"id":"state","operation":{"type":"state","entityId":"unknown-entity","perspective":"author"}}]}});
    let (status, report) = call(
        &app,
        "POST",
        &format!("{task_uri}/query"),
        CANDIDATE,
        query,
        "query_story",
        THREAD,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["taskId"], task["id"]);
    let baseline = workspace(&app, &root).await;
    for (path, tool, body) in [
        (
            format!("{task_uri}/actions"),
            "request_candidate_task",
            json!({"taskId":task["id"],"threadId":THREAD,"expectedRevision":ws["revision"],"expectedIntentRevision":0,"operation":"generateOutline","target":{"type":"artifact"},"instruction":"越权","idempotencyKey":"action"}),
        ),
        (
            format!("{root}/artifacts/brief"),
            "save_brief",
            json!({"taskId":task["id"],"threadId":THREAD,"expectedRevision":ws["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"brief","payload":{"premise":"越权"}}),
        ),
        (
            format!("{task_uri}/planning-reviews"),
            "request_planning_review",
            json!({"threadId":THREAD,"expectedRevision":ws["revision"],"expectedIntentRevision":0,"artifactKind":"outline","revisionId":"missing","targetNodeId":null,"roles":["structure","character"],"focus":"越权","idempotencyKey":"review"}),
        ),
    ] {
        assert!(
            !call(&app, "POST", &path, CANDIDATE, body, tool, THREAD)
                .await
                .0
                .is_success()
        );
    }
    assert_eq!(workspace(&app, &root).await, baseline);
    assert_eq!(counts(&path), (1, 1, 0));
    let(status,done)=call(&app,"POST",&format!("{task_uri}/complete"),CANDIDATE,json!({"taskId":task["id"],"threadId":THREAD,"expectedRevision":ws["revision"],"summary":"检查结束：需要作者决定，不改正文"}),"complete_task",THREAD).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    let work = author(
        &app,
        "GET",
        &format!("{root}/work-tasks/work-{}", task["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(work["steps"][0]["expectedOutputKind"], "completionReport");
    assert_eq!(
        work["deliveryReport"]["evidence"][0]["kind"],
        "completionReport"
    );
    assert_eq!(work["deliveryReport"]["evidence"][0]["state"], "recorded");
    assert_eq!(counts(&path), (1, 1, 0));
    assert!(
        !call(
            &app,
            "GET",
            &metadata_uri,
            CANDIDATE,
            Value::Null,
            "read_task_source",
            THREAD
        )
        .await
        .0
        .is_success()
    );
    assert!(
        !call(
            &app,
            "POST",
            &source_uri,
            CANDIDATE,
            source_request(&task, "saved", Value::Null),
            "read_chapter_structure_source",
            THREAD
        )
        .await
        .0
        .is_success()
    );
}

#[tokio::test]
async fn empty_change_skip_and_bound_reader_refuses_freshness_drift_without_new_budget() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("stale.sqlite3");
    let app = novel_codex_story_runtime::router(
        novel_codex_story_runtime::RuntimeState::new(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    );
    let p = author(&app, "POST", "/api/projects", json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"边界"})).await;
    let root = format!("/api/projects/{}", p["project"]["id"].as_str().unwrap());
    let empty = make_chapter(&app, &root, "empty").await;
    let original = save(
        &app,
        &root,
        &empty,
        Value::Null,
        "source-before-empty",
        BODY,
    )
    .await;
    let project = p["project"]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let object = format!("draft-{}", empty["chapterId"].as_str().unwrap());
    // A valid, source-matched ready fixture is invalidated through the actual Author save path.
    db.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,created_at,published_at) VALUES('ready-before-empty',?1,?2,?3,0,'fixture','ready',1,1)",rusqlite::params![project,object,original["revisionId"].as_str().unwrap()]).unwrap();
    db.execute("INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES(?1,?2,'ready-before-empty')",rusqlite::params![project,object]).unwrap();
    let saved_empty = save(
        &app,
        &root,
        &empty,
        original["revisionId"].clone(),
        "empty-save",
        "",
    )
    .await;
    assert_eq!(
        db.query_row(
            "SELECT status FROM story_modeling_batches_v2 WHERE batch_id='ready-before-empty'",
            [],
            |r| r.get::<_, String>(/*idx*/ 0)
        )
        .unwrap(),
        "stale"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_knowledge_heads_v2 WHERE object_id=?1",
            [object],
            |r| r.get::<_, i64>(/*idx*/ 0)
        )
        .unwrap(),
        0
    );
    let ws = workspace(&app, &root).await;
    let receipt = author(
        &app,
        "POST",
        &format!(
            "{root}/chapters/{}/delete",
            empty["chapterId"].as_str().unwrap()
        ),
        json!({"expectedRevision":ws["revision"],"idempotencyKey":"empty-delete"}),
    )
    .await;
    let event = author(
        &app,
        "GET",
        &format!(
            "{root}/chapter-structure-changes/{}",
            receipt["structureChangeId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(event["assessment"]["state"], "skipped");
    assert_eq!(
        event["source"]["savedRevisionId"],
        saved_empty["revisionId"]
    );
    assert!(
        !call(
            &app,
            "POST",
            &format!("{root}/tasks"),
            AUTHOR,
            task_request(&workspace(&app, &root).await, &event, "skip"),
            "",
            ""
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(counts(&path), (0, 0, 0));
    let planned = make_chapter(&app, &root, "planned-empty").await;
    let ws = workspace(&app, &root).await;
    let plan=author(&app,"POST",&format!("{root}/artifacts/plan"),json!({"payload":{"chapters":[{"chapterPlanId":"empty-plan","chapterNumber":planned["chapterNumber"],"goal":"衔接","scenes":[{"id":"planned-scene","goal":"发现线索","beats":[],"expectedEffects":[{"id":"effect","dimension":"knowledge","description":"解释线索","commitmentRefs":[]}]}]}]},"expectedRevision":ws["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"plan"})).await;
    let review=author(&app,"POST",&format!("{root}/reviews"),json!({"artifactKind":"plan","objectId":"artifact-plan","revisionId":plan["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"plan-review"})).await;
    let ws = workspace(&app, &root).await;
    author(&app,"POST",&format!("{root}/reviews/{}/accept",review["id"].as_str().unwrap()),json!({"expectedRevision":ws["revision"],"selections":{"noConfirmedChanges":true},"idempotencyKey":"plan-accept"})).await;
    let ws = workspace(&app, &root).await;
    author(&app,"POST",&format!("{root}/chapters/associate-plan"),json!({"chapterId":planned["chapterId"],"chapterPlanId":"empty-plan","expectedRevision":ws["revision"],"idempotencyKey":"associate"})).await;
    let ws = workspace(&app, &root).await;
    let deleted_plan = author(
        &app,
        "POST",
        &format!(
            "{root}/chapters/{}/delete",
            planned["chapterId"].as_str().unwrap()
        ),
        json!({"expectedRevision":ws["revision"],"idempotencyKey":"planned-delete"}),
    )
    .await;
    let plan_event = author(
        &app,
        "GET",
        &format!(
            "{root}/chapter-structure-changes/{}",
            deleted_plan["structureChangeId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(
        (
            plan_event["assessment"]["state"].clone(),
            plan_event["source"]["savedRevisionId"].clone()
        ),
        (json!("pending"), Value::Null)
    );
    let chapter = make_chapter(&app, &root, "last").await;
    save(&app, &root, &chapter, Value::Null, "last-save", BODY).await;
    let ws = workspace(&app, &root).await;
    let receipt = author(
        &app,
        "POST",
        &format!(
            "{root}/chapters/{}/delete",
            chapter["chapterId"].as_str().unwrap()
        ),
        json!({"expectedRevision":ws["revision"],"idempotencyKey":"last-delete"}),
    )
    .await;
    let event = author(
        &app,
        "GET",
        &format!(
            "{root}/chapter-structure-changes/{}",
            receipt["structureChangeId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(event["assessment"]["state"], "pending");
    let ws = workspace(&app, &root).await;
    let task = author(
        &app,
        "POST",
        &format!("{root}/tasks"),
        task_request(&ws, &event, "last"),
    )
    .await;
    let source_uri = format!(
        "{root}/tasks/{}/chapter-structure-source",
        task["id"].as_str().unwrap()
    );
    let (status, initial_page) = call(
        &app,
        "POST",
        &source_uri,
        CANDIDATE,
        source_request(&task, "saved", Value::Null),
        "read_chapter_structure_source",
        THREAD,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{initial_page}");
    assert!(initial_page["nextCursor"].is_string());
    author(
        &app,
        "PATCH",
        &format!("{root}/tasks/{}", task["id"].as_str().unwrap()),
        json!({"threadId":THREAD,"status":"failed","error":"Interrupted"}),
    )
    .await;
    settle::settle(&path, project, task["id"].as_str().unwrap());
    let work_uri = format!("{root}/work-tasks/work-{}", task["id"].as_str().unwrap());
    let work = author(&app, "GET", &work_uri, Value::Null).await;
    let resumed = author(
        &app,
        "POST",
        &format!("{work_uri}/control"),
        json!({"expectedVersion":work["version"],"command":"resume"}),
    )
    .await;
    let mut target = task["target"].clone();
    target["resumeStepId"] = resumed["steps"][0]["id"].clone();
    let fresh = workspace(&app, &root).await;
    let next=author(&app,"POST",&format!("{root}/tasks"),json!({"operation":"assessChapterStructure","inputRevision":fresh["revision"],"threadId":THREAD,"target":target,"authorIntent":"显式恢复原检查","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now()+600})).await;
    assert_eq!(counts(&path), (2, 1, 0));
    let next_work = author(&app, "GET", &work_uri, Value::Null).await;
    assert_eq!(next_work["steps"][0]["currentAttemptId"], next["id"]);
    let next_uri = format!(
        "{root}/tasks/{}/chapter-structure-source",
        next["id"].as_str().unwrap()
    );
    assert!(
        !call(
            &app,
            "POST",
            &next_uri,
            CANDIDATE,
            source_request(&next, "saved", initial_page["nextCursor"].clone()),
            "read_chapter_structure_source",
            THREAD
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &next_uri,
            CANDIDATE,
            source_request(&next, "saved", Value::Null),
            "read_chapter_structure_source",
            THREAD
        )
        .await
        .0,
        StatusCode::OK
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_workspaces_v2 SET revision=revision+1 WHERE project_id=?1",
        [project],
    )
    .unwrap();
    assert!(
        !call(
            &app,
            "POST",
            &next_uri,
            CANDIDATE,
            source_request(&next, "saved", Value::Null),
            "read_chapter_structure_source",
            THREAD
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(counts(&path), (2, 1, 0));
    assert!(
        !call(
            &app,
            "GET",
            &format!(
                "{root}/chapter-structure-changes?cursor=all:structure-test&assessmentState=pending"
            ),
            AUTHOR,
            Value::Null,
            "",
            ""
        )
        .await
        .0
        .is_success()
    );
}
