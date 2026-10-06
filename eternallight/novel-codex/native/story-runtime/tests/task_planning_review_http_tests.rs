use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
const AUTHOR: &str = "planning-author-12345678901234567890";
const CANDIDATE: &str = "planning-candidate-12345678901234567";
const CAP: &str = "planning-capability-123456789012345";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    actor: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {actor}"))
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
async fn register(app: &Router, uri: &str, body: Value, caller: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", format!("Bearer {CANDIDATE}"))
                .header("content-type", "application/json")
                .header("x-novel-task-capability", CAP)
                .header("x-novel-tool-name", "request_planning_review")
                .header("x-novel-root-thread", "planning-thread")
                .header("x-novel-caller-thread", caller)
                .header("x-novel-turn", "turn-register")
                .header(
                    "x-novel-tool-call",
                    format!("call-{}", uuid::Uuid::new_v4().simple()),
                )
                .header(
                    "x-novel-request-hash",
                    format!("{:x}", Sha256::digest(body.to_string())),
                )
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn setup_with_intent(author_intent: &str) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    Router,
    String,
    String,
    Value,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("planning.sqlite3");
    let app = novel_codex_story_runtime::router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        json!({"title":"合成深审"}),
        AUTHOR,
    )
    .await;
    let root = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let (status,outline)=call(&app,"POST",&format!("{root}/artifacts/outline"),json!({
        "payload":{"nodes":[{"id":"book","kind":"book","position":1,"title":"港城","purpose":"记忆","summary":"城市消失","dependencyKeys":[],"certainty":"tentative","protection":"free"}]},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{outline}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status,task)=call(&app,"POST",&format!("{root}/tasks"),json!({"operation":"advisory","inputRevision":0,"threadId":"planning-thread","target":{"type":"project"},
        "authorIntent":author_intent,"capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":expires}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let body = json!({"threadId":"planning-thread","expectedRevision":0,"expectedIntentRevision":0,"artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,"roles":["structure","character"],"focus":"检查记忆动机","idempotencyKey":"review-one"});
    (
        dir,
        path,
        app,
        root,
        task["id"].as_str().unwrap().to_owned(),
        body,
    )
}
async fn setup() -> (tempfile::TempDir, std::path::PathBuf, Router, String, String, Value) {
    setup_with_intent("请从结构和人物两个视角深审现有大纲，不修改规划。").await
}

fn settle(path: &std::path::Path, app_task: &str) {
    let engine_path = path.with_extension("engine.sqlite3");
    let _engine=eternallight_engine_core::PersistentBudget::new(&engine_path,app_task,json!({"options":{"id":format!("budget-{app_task}"),"maxRequests":16,"maxTokens":200000,"startedAt":1000},"now":1000})).unwrap();
    drop(_engine);
    eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
}

#[tokio::test]
async fn task_registration_reuses_work_author_source_and_close_isolates_runs_and_reports() {
    let (_dir, path, app, root, task, body) = setup().await;
    let uri = format!("{root}/tasks/{task}/planning-reviews");
    let (_, before) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert!(
        call(&app, "GET", &uri, Value::Null, AUTHOR).await.1["runs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (status, first) = register(&app, &uri, body.clone(), "planning-thread").await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(
        (
            first["workTaskId"].clone(),
            first["sourceTaskId"].clone(),
            first["sourceIntentRevision"].clone(),
            first["registrationOnly"].clone()
        ),
        (before["id"].clone(), json!(task), json!(0), json!(true))
    );
    let (_, full) = call(
        &app,
        "GET",
        &format!("{root}/planning-reviews/{}", first["id"].as_str().unwrap()),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        full["authorIntent"],
        "请从结构和人物两个视角深审现有大纲，不修改规划。"
    );
    assert!(full["checksByRole"].is_object());
    for key in [
        "authorIntent",
        "focus",
        "checksByRole",
        "reports",
        "sourcePacketHash",
    ] {
        assert!(first.get(key).is_none());
    }
    assert!(first.to_string().len() < 1000);
    let envelope = json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":json!({"content":[{"type":"text","text":first.to_string()}]}).to_string()});
    assert!(
        eternallight_engine_core::dispatch("tokens.count", json!(envelope.to_string()))
            .unwrap()
            .as_u64()
            .unwrap()
            < 1000
    );
    assert_eq!(
        register(&app, &uri, body.clone(), "planning-thread").await,
        (StatusCode::OK, first.clone())
    );
    let mut changed = body.clone();
    changed["focus"] = json!("不同问题");
    assert!(
        !register(&app, &uri, changed, "planning-thread")
            .await
            .0
            .is_success()
    );
    let mut next = body.clone();
    next["idempotencyKey"] = json!("review-two");
    let (_, second) = register(&app, &uri, next, "planning-thread").await;
    let mut third_request = body.clone();
    third_request["idempotencyKey"] = json!("review-three");
    let (status, third) = register(&app, &uri, third_request, "planning-thread").await;
    assert_eq!(status, StatusCode::OK, "{third}");
    let mut fourth_request = body.clone();
    fourth_request["idempotencyKey"] = json!("review-four");
    assert!(
        !register(&app, &uri, fourth_request, "planning-thread")
            .await
            .0
            .is_success()
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    // Even an existing saved report is not echoed into the registration tool's context.
    // This adversarial persisted-row fixture only checks the wire projection; real seat report storage is exercised below.
    db.execute("INSERT INTO story_planning_review_reports_v2(report_id,run_id,role,task_id,report_json,status,created_at) VALUES ('synthetic-old-report',?1,'structure',?2,?3,'partial',1)",
        rusqlite::params![first["id"].as_str().unwrap(),task,json!({"summary":"never echo report text".repeat(/*n*/1000)}).to_string()]).unwrap();
    let (status, replayed) = register(&app, &uri, body.clone(), "planning-thread").await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed["reportCount"], 1);
    assert!(replayed.get("reports").is_none());
    assert!(!replayed.to_string().contains("never echo report text"));
    db.execute(
        "DELETE FROM story_planning_review_reports_v2 WHERE report_id='synthetic-old-report'",
        [],
    )
    .unwrap();
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1)",[],|r|Ok((r.get::<_,i64>(/*idx*/0)?,r.get::<_,i64>(/*idx*/1)?,r.get::<_,i64>(/*idx*/2)?))).unwrap(),(1,1,1));
    let first_uri = format!("{root}/planning-reviews/{}", first["id"].as_str().unwrap());
    let (status, cancelled) = call(
        &app,
        "POST",
        &format!("{first_uri}/cancel"),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    let (_, work) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["status"], "running");
    assert_eq!(work["steps"][0], before["steps"][0]);
    let second_id = second["id"].as_str().unwrap();
    assert!(
        work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["target"]["reviewRunId"] == second_id)
            .all(|s| s["status"] == "queued")
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/tasks/{task}/complete"),
            json!({"threadId":"planning-thread","summary":"已登记深审"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    settle(&path, &task);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!(
                "{root}/work-tasks/{}/provider-usage",
                before["id"].as_str().unwrap()
            ),
            json!({"taskId":task}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let second_uri = format!("{root}/planning-reviews/{second_id}");
    let (status,seat)=call(&app,"POST",&format!("{second_uri}/seats/structure/tasks"),json!({"threadId":"seat-thread","capabilityHash":"a".repeat(/*n*/64),"capabilityExpiresAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600}),AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{seat}");
    let seat_id = seat["id"].as_str().unwrap();
    assert_eq!(
        seat["inputManifest"]["planningReviewAuthorIntentBinding"]["sourceTaskId"],
        task
    );
    let (status,report)=call(&app,"POST",&format!("{second_uri}/reports"),json!({"taskId":seat_id,"threadId":"seat-thread","expectedRevision":0,"report":{"summary":"已检查结构","strengths":[],"findings":[],"coverage":[{"checkId":"structure:book","status":"checked"}]}}),AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    call(
        &app,
        "POST",
        &format!("{root}/tasks/{seat_id}/complete"),
        json!({"threadId":"seat-thread","summary":"结构完成"}),
        AUTHOR,
    )
    .await;
    settle(&path, seat_id);
    call(
        &app,
        "POST",
        &format!(
            "{root}/work-tasks/{}/provider-usage",
            before["id"].as_str().unwrap()
        ),
        json!({"taskId":seat_id}),
        AUTHOR,
    )
    .await;
    let (status, finalized) = call(
        &app,
        "POST",
        &format!("{second_uri}/finalize"),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finalized}");
    assert_eq!(finalized["status"], "partial");
    let (_, work) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["status"], "running");
    assert!(
        work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["target"]["reviewRunId"] == third["id"])
            .all(|s| s["status"] == "queued")
    );
    assert!(
        work["deliveryReport"]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "planningReview"
                && e["referenceId"] == report["id"]
                && e["state"] == "recorded")
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_acceptances_v2", [], |r| r
            .get::<_, i64>(/*idx*/ 0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT revision FROM story_workspaces_v2", [], |r| r
            .get::<_, i64>(/*idx*/ 0))
            .unwrap(),
        0
    );
    // Historical canonical evidence is tied to the saved report, not a replacement artifact head.
    db.execute(
        "UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE object_kind='outline'",
        [],
    )
    .unwrap();
    let (_, historical) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        historical["deliveryReport"]["evidence"],
        work["deliveryReport"]["evidence"]
    );
}

#[tokio::test]
async fn registration_rejects_stale_intent_wrong_root_parent_stop_and_rolls_back_seats() {
    let (_dir, path, app, root, task, body) = setup().await;
    let uri = format!("{root}/tasks/{task}/planning-reviews");
    let db = rusqlite::Connection::open(&path).unwrap();
    assert!(
        !register(&app, &uri, body.clone(), "delegated-thread")
            .await
            .0
            .is_success()
    );
    for (field, value) in [
        ("expectedRevision", json!(1)),
        ("revisionId", json!("unknown")),
        ("roles", json!(["structure", "structure"])),
        ("focus", json!("文".repeat(/*n*/ 1001))),
    ] {
        let mut invalid = body.clone();
        invalid[field] = value;
        assert!(
            !register(&app, &uri, invalid, "planning-thread")
                .await
                .0
                .is_success()
        );
    }
    db.execute_batch("CREATE TRIGGER fail_seats BEFORE INSERT ON story_planning_review_seats_v2 BEGIN SELECT RAISE(ABORT,'seat failure'); END;").unwrap();
    assert!(
        !register(&app, &uri, body.clone(), "planning-thread")
            .await
            .0
            .is_success()
    );
    db.execute_batch("DROP TRIGGER fail_seats;").unwrap();
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_planning_review_runs_v2),(SELECT COUNT(*) FROM story_work_steps_v1)",[],|r|Ok((r.get::<_,i64>(/*idx*/0)?,r.get::<_,i64>(/*idx*/1)?))).unwrap(),(0,1));
    let project = ProjectId::parse(root.trim_start_matches("/api/projects/")).unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.append_task_intent(
        &project,
        &task,
        /*expected_intent_revision*/ 0,
        "优先人物",
        /*now*/ 2,
    )
    .unwrap();
    assert!(
        !register(&app, &uri, body.clone(), "planning-thread")
            .await
            .0
            .is_success()
    );
    repo.deliver_task_intent(&project, &task, /*intent_revision*/ 1, /*now*/ 3)
        .unwrap();
    let mut updated = body.clone();
    updated["expectedIntentRevision"] = json!(1);
    let (status, run) = register(&app, &uri, updated.clone(), "planning-thread").await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["sourceIntentRevision"], 1);
    let packet: Value = serde_json::from_str(
        &db.query_row(
            "SELECT source_packet_json FROM story_planning_review_runs_v2 WHERE run_id=?1",
            [run["id"].as_str().unwrap()],
            |r| r.get::<_, String>(/*idx*/ 0),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        packet["authorHistory"]
            .as_str()
            .unwrap()
            .contains("作者追加要求：\n优先人物")
    );
    assert_eq!(packet["focus"], body["focus"]);
    repo.append_task_intent(
        &project,
        &task,
        /*expected_intent_revision*/ 1,
        "改为阅读体验优先",
        /*now*/ 4,
    )
    .unwrap();
    repo.deliver_task_intent(&project, &task, /*intent_revision*/ 2, /*now*/ 5)
        .unwrap();
    repo.complete_task(
        &project,
        &task,
        "planning-thread",
        "登记完成".into(), None,
        /*now*/ 6,
    )
    .unwrap();
    settle(&path, &task);
    let owner = repo.work_task_for_attempt(&project, &task).unwrap();
    call(
        &app,
        "POST",
        &format!("{root}/work-tasks/{}/provider-usage", owner.id),
        json!({"taskId":task}),
        AUTHOR,
    )
    .await;
    let (status,_)=call(&app,"POST",&format!("{root}/planning-reviews/{}/seats/structure/tasks",run["id"].as_str().unwrap()),json!({"threadId":"seat-thread","capabilityHash":"a".repeat(/*n*/64),"capabilityExpiresAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600}),AUTHOR).await;
    assert!(
        !status.is_success(),
        "an old-intent run must not claim a seat after a steer"
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r
            .get::<_, i64>(/*idx*/ 0))
            .unwrap(),
        1
    );
    let work = repo.work_task_for_attempt(&project, &task).unwrap();
    repo.control_work_task(&project, &work.id, "cancel", work.version, /*now*/ 7)
        .unwrap();
    updated["idempotencyKey"] = json!("after-cancel");
    assert!(
        !register(&app, &uri, updated, "planning-thread")
            .await
            .0
            .is_success()
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_planning_review_runs_v2",
            [],
            |r| r.get::<_, i64>(/*idx*/ 0)
        )
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_single_needed_view_keeps_the_original_work_and_direct_review_contract() {
    let (_dir,path,app,root,task,mut body)=setup_with_intent("只检查人物动机，不改规划。").await;
    let uri=format!("{root}/tasks/{task}/planning-reviews");
    for roles in [json!([]),json!(["unknown"]),json!(["character","character"]),json!(["structure","character","experience","structure"])] {
        let mut invalid=body.clone(); invalid["roles"]=roles;
        assert!(!register(&app,&uri,invalid,"planning-thread").await.0.is_success());
    }
    body["roles"]=json!(["character"]);
    let (code,registered)=register(&app,&uri,body.clone(),"planning-thread").await;
    assert_eq!(code,StatusCode::OK,"{registered}");
    assert_eq!(registered["roles"],json!(["character"]));
    let (_,run)=call(&app,"GET",&format!("{root}/planning-reviews/{}",registered["id"].as_str().unwrap()),Value::Null,AUTHOR).await;
    assert_eq!(run["seats"].as_array().unwrap().len(),1);
    assert!(run["checksByRole"]["character"].is_array());
    assert!(run["checksByRole"].get("structure").is_none());
    let db=rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_work_tasks_v1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_work_budget_authorizations_v1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    let request=json!({"artifactKind":"outline","revisionId":body["revisionId"],"targetNodeId":null,"authorIntent":"仅检查阅读体验","roles":["experience"]});
    let (code,direct)=call(&app,"POST",&format!("{root}/planning-reviews"),request,AUTHOR).await;
    assert_eq!(code,StatusCode::OK,"{direct}");
    assert_eq!(direct["roles"],json!(["experience"]));
    assert_eq!(direct["seats"].as_array().unwrap().len(),1);
}
