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
    (status, task_fixture::work_control_response(uri, status, serde_json::from_slice(&bytes).unwrap()))
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
async fn setup_with_intent(
    author_intent: &str,
) -> (
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
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"合成深审"}),
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
    save_review_plan(&app, &root, &task).await;
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
async fn save_review_plan(app: &Router, root: &str, task: &Value) {
    let (status,result)=call(app,"POST",&format!("{root}/tasks/{}/work-plan",task["id"].as_str().unwrap()),json!({"taskId":task["id"],"threadId":task["threadId"],"expectedRevision":task["inputRevision"],"expectedPlanVersion":0,"idempotencyKey":"review-goals","plan":{"intentMode":"executeGoals","goals":[{"id":"review","title":"按作者要求评审大纲","acceptance":"完成指定视角的评审并核对报告","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"","checkScope":"当前大纲","approach":"登记指定评审并核对报告","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null}]}}),CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{result}");
}
async fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    Router,
    String,
    String,
    Value,
) {
    setup_with_intent("请从结构和人物两个视角深审现有大纲，不修改规划。").await
}

fn settle(path: &std::path::Path, app_task: &str) {
    let engine_path = path.with_extension("engine.sqlite3");
    let _engine=eternallight_engine_core::PersistentBudget::new(&engine_path,app_task,json!({"options":{"id":format!("budget-{app_task}"),"maxRequests":16,"maxTokens":200000,"startedAt":1000},"now":1000})).unwrap();
    drop(_engine);
    eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
}

fn terminal_budget(path: &std::path::Path, task: &str, status: &str) {
    let db = rusqlite::Connection::open(path).unwrap();
    let encoded: String = db
        .query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [task],
            |r| r.get(0),
        )
        .unwrap();
    let grant: Value = serde_json::from_str(&encoded).unwrap();
    let mut engine = eternallight_engine_core::PersistentBudget::new(
        &path.with_extension("engine.sqlite3"),
        task,
        json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000},"now":1000}),
    )
    .unwrap();
    engine.complete(status).unwrap();
}

#[tokio::test]
async fn output_limit_planning_seat_uses_dedicated_resume_without_bypassing_zero_write_proof() {
    for unknown_write in [false, true] {
        let (_dir, path, app, root, source_task, body) = setup().await;
        let project = root.trim_start_matches("/api/projects/");
        let (code, run) = register(
            &app,
            &format!("{root}/tasks/{source_task}/planning-reviews"),
            body,
            "planning-thread",
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{run}");
        let run_id = run["id"].as_str().unwrap();
        let work_uri = format!("{root}/work-tasks/{}", run["workTaskId"].as_str().unwrap());
        let run_uri = format!("{root}/planning-reviews/{run_id}");
        let seat_uri = format!("{run_uri}/seats/structure/tasks");
        let (code, completed) = call(
            &app,
            "POST",
            &format!("{root}/tasks/{source_task}/complete"),
            json!({"threadId":"planning-thread","summary":"已登记指定视角深审，交给专门席位执行"}),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{completed}");
        terminal_budget(&path, &source_task, "completed");
        assert_eq!(
            call(
                &app,
                "POST",
                &format!("{work_uri}/provider-usage"),
                json!({"taskId":source_task}),
                AUTHOR
            )
            .await
            .0,
            StatusCode::OK
        );
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let request = json!({"threadId":"seat-thread","capabilityHash":"a".repeat(64),"capabilityExpiresAt":expires});
        let (code, seat) = call(&app, "POST", &seat_uri, request.clone(), AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{seat}");
        let failed_id = seat["id"].as_str().unwrap();
        let frozen = call(&app, "GET", &run_uri, Value::Null, AUTHOR).await.1;
        let (code, failed) = call(
            &app,
            "PATCH",
            &format!("{root}/tasks/{failed_id}"),
            json!({"threadId":"seat-thread","status":"failed","error":"provider_output_limit"}),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{failed}");
        let before = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1;
        assert_eq!(before["status"], "blocked");
        let control = json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":before["threadId"],"command":"resume","expectedControlRevision":before["controlRevision"]});
        let (code, refused) = call(
            &app,
            "POST",
            &format!("{work_uri}/control"),
            control.clone(),
            AUTHOR,
        )
        .await;
        assert!(
            code.is_client_error(),
            "unsettled seat must not resume: {refused}"
        );
        assert_eq!(
            call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1,
            before
        );
        terminal_budget(&path, failed_id, "failed");
        let (code, settled) = call(
            &app,
            "POST",
            &format!("{work_uri}/provider-usage"),
            json!({"taskId":failed_id}),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{settled}");
        let before = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1;
        let control = json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":before["threadId"],"command":"resume","expectedControlRevision":before["controlRevision"]});
        let run_before = call(&app, "GET", &run_uri, Value::Null, AUTHOR).await.1;
        let db = rusqlite::Connection::open(&path).unwrap();
        if unknown_write {
            // An accepted operation with no outcome is not a proved zero-write failure.
            db.execute("INSERT INTO story_tool_operations_v1 VALUES ('seat-unknown-write',?1,?2,'seat-thread','turn','call','save_planning_review_report','hash',NULL,'accepted',NULL,1,1)", rusqlite::params![project,failed_id]).unwrap();
            let unknown_before = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1;
            assert_eq!(unknown_before["version"], before["version"]);
            assert_eq!(unknown_before["steps"], before["steps"]);
            assert_eq!(unknown_before["resumeBlockReason"], "pendingRecovery");
            assert!(unknown_before["resumeMode"].is_null());
            let (code, refused) = call(
                &app,
                "POST",
                &format!("{work_uri}/control"),
                control,
                AUTHOR,
            )
            .await;
            assert!(
                code.is_client_error(),
                "unknown seat write must not resume: {refused}"
            );
            assert_eq!(
                call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1,
                unknown_before
            );
            assert_eq!(
                call(&app, "GET", &run_uri, Value::Null, AUTHOR).await.1,
                run_before
            );
            assert!(
                call(&app, "POST", &seat_uri, request, AUTHOR)
                    .await
                    .0
                    .is_client_error()
            );
            assert_eq!(
                call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1,
                unknown_before
            );
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM story_work_attempts_v1 WHERE step_id=?1",
                    [format!("review-seat-{run_id}-structure")],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            assert_eq!(
                db.query_row("SELECT state FROM story_tool_operations_v1 WHERE operation_id='seat-unknown-write'", [], |r| r.get::<_,String>(0)).unwrap(),
                "accepted"
            );
            continue;
        }
        let (code, resumed) = call(
            &app,
            "POST",
            &format!("{work_uri}/control"),
            control,
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{resumed}");
        let step_id = format!("review-seat-{run_id}-structure");
        let step = resumed["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == step_id)
            .unwrap();
        assert_eq!(step["status"], "queued");
        assert!(step["currentAttemptId"].is_null());
        assert_eq!(step["attempts"].as_array().unwrap().len(), 1);
        let (code, replacement) = call(&app, "POST", &seat_uri, request, AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{replacement}");
        assert_ne!(replacement["id"], seat["id"]);
        assert_eq!(replacement["operation"], "reviewPlanningSeat");
        assert_eq!(replacement["authorIntent"], seat["authorIntent"]);
        assert_eq!(
            replacement["inputManifest"]["target"],
            seat["inputManifest"]["target"]
        );
        assert_eq!(
            replacement["inputManifest"]["planningReviewAuthorIntentBinding"],
            seat["inputManifest"]["planningReviewAuthorIntentBinding"]
        );
        let current_run = call(&app, "GET", &run_uri, Value::Null, AUTHOR).await.1;
        for field in [
            "revisionId",
            "sourcePacketHash",
            "authorIntent",
            "checksByRole",
        ] {
            assert_eq!(
                current_run[field], frozen[field],
                "frozen {field} must survive retry"
            );
        }
        let work = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1;
        let step = work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == step_id)
            .unwrap();
        assert_eq!(step["currentAttemptId"], replacement["id"]);
        assert_eq!(step["attempts"].as_array().unwrap().len(), 2);
        assert_eq!(
            call(
                &app,
                "GET",
                &format!("{root}/tasks/{failed_id}"),
                Value::Null,
                AUTHOR
            )
            .await
            .1["status"],
            "failed"
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}

#[tokio::test]
async fn planning_review_registration_does_not_grant_dispatch_authority() {
    let (_dir, path, app, root, _planned, body) = setup().await;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (code, task) = call(&app,"POST",&format!("{root}/tasks"),json!({"operation":"advisory","inputRevision":0,"threadId":"planning-thread","target":{"type":"project"},"authorIntent":"评审当前大纲","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":expires}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let uri = format!("{root}/tasks/{task_id}/planning-reviews");
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts = || -> Vec<i64> {
        [
            "story_tasks_v2",
            "story_work_attempts_v1",
            "story_task_provider_budgets_v1",
            "story_author_messages_v1",
            "story_work_budget_authorizations_v1",
        ]
        .iter()
        .map(|table| {
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
        })
        .collect()
    };
    let before_registration = counts();
    let (code, registered) = register(&app, &uri, body.clone(), "planning-thread").await;
    assert_eq!(code, StatusCode::OK, "{registered}");
    assert_eq!(registered["registrationOnly"], true);
    assert_eq!(registered["reportCount"], 0);
    assert_eq!(registered["sourceTaskId"], task_id);
    assert_eq!(registered["workTaskId"], format!("work-{task_id}"));
    assert_eq!(counts(), before_registration);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_planning_review_runs_v2 WHERE source_task_id=?1",
            [task_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_model_work_plans_v1 WHERE work_task_id=?1",
            [registered["workTaskId"].as_str().unwrap()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let work_uri = format!(
        "{root}/work-tasks/{}",
        registered["workTaskId"].as_str().unwrap()
    );
    let seat_uri = format!(
        "{root}/planning-reviews/{}/seats/structure/tasks",
        registered["id"].as_str().unwrap()
    );
    let request = json!({"threadId":"seat-thread","capabilityHash":"a".repeat(64),"capabilityExpiresAt":expires});
    for stage in [
        "active-source-without-method",
        "active-source-with-method",
        "completed-source-unsettled",
    ] {
        if stage == "active-source-with-method" {
            save_review_plan(&app, &root, &task).await;
            let (code, replay) = register(&app, &uri, body.clone(), "planning-thread").await;
            assert_eq!(code, StatusCode::OK, "{replay}");
            assert_eq!(replay, registered);
        } else if stage == "completed-source-unsettled" {
            let (code, completed) = call(
                &app,
                "POST",
                &format!("{root}/tasks/{task_id}/complete"),
                json!({"threadId":"planning-thread","summary":"已登记评审席位；评审未执行"}),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{completed}");
            assert_eq!(completed["status"], "completed");
        }
        let (code, before_work) = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{before_work}");
        let before_counts = counts();
        let before_ledger: String = db
            .query_row(
                "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
                [registered["workTaskId"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        let (code, rejected) = call(&app, "POST", &seat_uri, request.clone(), AUTHOR).await;
        assert!(code.is_client_error(), "{stage}: {rejected}");
        assert_eq!(code, StatusCode::CONFLICT, "{stage}: {rejected}");
        assert_eq!(rejected["error"]["code"], "work_usage_unsettled", "{stage}: {rejected}");
        assert_eq!(counts(), before_counts, "{stage}");
        let after_ledger: String = db
            .query_row(
                "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
                [registered["workTaskId"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after_ledger, before_ledger, "{stage}");
        assert_eq!(
            call(&app, "GET", &work_uri, Value::Null, AUTHOR).await,
            (StatusCode::OK, before_work)
        );
        assert_eq!(db.query_row("SELECT COUNT(*) FROM story_planning_review_seats_v2 WHERE run_id=?1 AND task_id IS NOT NULL",[registered["id"].as_str().unwrap()],|row|row.get::<_,i64>(0)).unwrap(),0);
    }
    terminal_budget(&path, task_id, "completed");
    let (code, usage) = call(
        &app,
        "POST",
        &format!("{work_uri}/provider-usage"),
        json!({"taskId":task_id}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{usage}");
    let after_settlement = counts();
    let (code, seat) = call(&app, "POST", &seat_uri, request, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{seat}");
    assert_eq!(seat["operation"], "reviewPlanningSeat");
    assert_eq!(seat["target"]["reviewRunId"], registered["id"]);
    assert_eq!(db.query_row("SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1",[seat["id"].as_str().unwrap()],|row|row.get::<_,String>(0)).unwrap(),registered["workTaskId"].as_str().unwrap());
    let after_dispatch = counts();
    assert_eq!(
        after_dispatch,
        vec![
            after_settlement[0] + 1,
            after_settlement[1] + 1,
            after_settlement[2] + 1,
            after_settlement[3],
            after_settlement[4]
        ]
    );
    let (code, after_work) = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{after_work}");
    assert_eq!(after_work["id"], registered["workTaskId"]);
    let after_ledger: String = db
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [registered["workTaskId"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&after_ledger).unwrap(), usage);
}

#[tokio::test]
async fn admitted_planning_review_keeps_author_delivery_pending_after_all_seats_close() {
    let (_dir, path, app, root, task, body) = setup().await;
    let uri = format!("{root}/tasks/{task}/planning-reviews");
    let (status, run) = register(&app, &uri, body, "planning-thread").await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let work = run["workTaskId"].as_str().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let obligation: (String, i64) = db
        .query_row(
            "SELECT origin_task_id,finished FROM story_work_decisions_v1 WHERE work_task_id=?1",
            [work],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(obligation, (task.clone(), 0));
    assert_eq!(
        call(
            &app,
            "POST",
            &format!(
                "{root}/planning-reviews/{}/cancel",
                run["id"].as_str().unwrap()
            ),
            json!({}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/tasks/{task}/complete"),
            json!({"threadId":"planning-thread","summary":"已关闭本轮深审席位，交还作者目标决策"}),
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
            &format!("{root}/work-tasks/{work}/provider-usage"),
            json!({"taskId":task}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (code, current) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{current}");
    assert_eq!(current["status"], "running");
    let workspace = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await
    .1;
    let (code,decision)=call(&app,"POST",&format!("{root}/work-tasks/{work}/decision"),json!({"expectedWorkVersion":current["version"],"expectedRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}),AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{decision}");
    assert_eq!(decision["step"]["operation"], "advisory");
    assert_eq!(decision["workTaskId"], work);
    assert_eq!(
        db.query_row(
            "SELECT finished FROM story_work_decisions_v1 WHERE work_task_id=?1",
            [work],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
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
        "登记完成".into(),
        None,
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
    repo.control_work_task(&project, &work.id, "cancel", work.control_revision, /*now*/ 7)
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
    let (_dir, path, app, root, task, mut body) =
        setup_with_intent("只检查人物动机，不改规划。").await;
    let uri = format!("{root}/tasks/{task}/planning-reviews");
    for roles in [
        json!([]),
        json!(["unknown"]),
        json!(["character", "character"]),
        json!(["structure", "character", "experience", "structure"]),
    ] {
        let mut invalid = body.clone();
        invalid["roles"] = roles;
        assert!(
            !register(&app, &uri, invalid, "planning-thread")
                .await
                .0
                .is_success()
        );
    }
    body["roles"] = json!(["character"]);
    let (code, registered) = register(&app, &uri, body.clone(), "planning-thread").await;
    assert_eq!(code, StatusCode::OK, "{registered}");
    assert_eq!(registered["roles"], json!(["character"]));
    let (_, run) = call(
        &app,
        "GET",
        &format!(
            "{root}/planning-reviews/{}",
            registered["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(run["seats"].as_array().unwrap().len(), 1);
    assert!(run["checksByRole"]["character"].is_array());
    assert!(run["checksByRole"].get("structure").is_none());
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_work_tasks_v1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let request = json!({"artifactKind":"outline","revisionId":body["revisionId"],"targetNodeId":null,"authorIntent":"仅检查阅读体验","roles":["experience"]});
    let (code, direct) = call(
        &app,
        "POST",
        &format!("{root}/planning-reviews"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{direct}");
    assert_eq!(direct["roles"], json!(["experience"]));
    assert_eq!(direct["seats"].as_array().unwrap().len(), 1);
}
