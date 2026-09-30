use axum::{
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
#[path = "support/settle_task_fixture.rs"]
mod settlement;

const AUTHOR: &str = "scope-author-token-12345678901234567890";
const CANDIDATE: &str = "scope-candidate-token-12345678901234567";
const CAPABILITY: &str = "scope-task-capability-1234567890123456";

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    body: Value,
    actor: &str,
) -> (StatusCode, Value) {
    let app = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let request = Request::builder()
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
        .unwrap();
    let response = app.author_fixture_request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn setup(path: &std::path::Path) -> (String, Value, Value) {
    let (status, value) = call(
        path,
        "POST",
        "/api/projects",
        json!({"title":"有限范围"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let base = format!("/api/projects/{}", value["project"]["id"].as_str().unwrap());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,task) = call(path,"POST",&format!("{base}/tasks"),json!({
        "operation":"advisory","threadId":"scope-thread","inputRevision":0,"authorIntent":"拟定故事定位，保留港城设定",
        "target":{"type":"project"},"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600
    }),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let action = propose(path, &base, &task, "first", "generateBrief").await;
    (base, task, action)
}

async fn propose(
    path: &std::path::Path,
    base: &str,
    task: &Value,
    key: &str,
    operation: &str,
) -> Value {
    let (status,value) = call(path,"POST",&format!("{base}/tasks/{}/actions",task["id"].as_str().unwrap()),json!({
        "taskId":task["id"],"threadId":"scope-thread","expectedRevision":0,"expectedIntentRevision":0,
        "idempotencyKey":key,"operation":operation,"target":{},"instruction":"生成保留港城设定的候选"
    }),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value
}

#[path = "execution_scope/continuation_tests.rs"]
mod continuation;

#[tokio::test]
async fn finite_scope_queues_covered_actions_once_without_enlarging_budget_or_authority() {
    for limit in [1, 2] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scope.sqlite3");
        let (base, task, first) = setup(&path).await;
        let scope_uri = format!(
            "{base}/tasks/{}/execution-scope",
            task["id"].as_str().unwrap()
        );
        let request = json!({"actionIds":[first["id"]],"maxActions":limit,"maxTaskAttempts":2,"maxAdvisoryRounds":0,"expectedIntentRevision":0});
        assert_eq!(
            call(
                &path,
                "POST",
                &format!("{scope_uri}/preview"),
                request.clone(),
                CANDIDATE
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, preview) = call(
            &path,
            "POST",
            &format!("{scope_uri}/preview"),
            request.clone(),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        assert_eq!(
            preview["authorMessageId"],
            task["inputManifest"]["authorMessageId"]
        );
        let confirm = json!({"request":request,"previewDigest":preview["previewDigest"]});
        assert_eq!(
            call(&path, "POST", &scope_uri, confirm.clone(), CANDIDATE)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, permit) = call(&path, "POST", &scope_uri, confirm.clone(), AUTHOR).await;
        assert_eq!(status, StatusCode::OK, "{permit}");
        assert_eq!(
            call(&path, "POST", &scope_uri, confirm.clone(), AUTHOR).await,
            (StatusCode::OK, permit.clone())
        );
        let second = propose(&path, &base, &task, "second", "generateBrief").await;
        assert_eq!(
            second["status"],
            if limit == 2 { "queued" } else { "proposed" }
        );
        assert_eq!(
            propose(&path, &base, &task, "third", "generateOutline").await["status"],
            "proposed"
        );
        let permission_uri = format!(
            "{base}/tasks/{}/actions/{}/permission",
            task["id"].as_str().unwrap(),
            first["id"].as_str().unwrap()
        );
        let (status, permission) = call(&path, "GET", &permission_uri, Value::Null, AUTHOR).await;
        assert_eq!(status, StatusCode::OK, "{permission}");
        assert_eq!(
            (
                permission["permission"]["kind"].clone(),
                permission["permission"]["scopePermitId"].clone()
            ),
            (json!("finiteScope"), permit["id"].clone())
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        let counts:(i64,i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_execution_scopes_v1),(SELECT COUNT(*) FROM story_action_permits_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
        assert_eq!(counts, (1, limit, 1, 1));
        let source_id = task["id"].as_str().unwrap();
        assert_eq!(
            call(
                &path,
                "POST",
                &format!("{base}/tasks/{source_id}/complete"),
                json!({"threadId":"scope-thread","summary":"有限方案已提出"}),
                CANDIDATE
            )
            .await
            .0,
            StatusCode::OK
        );
        let engine_path = path.with_extension("engine.sqlite3");
        let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,source_id,
            json!({"options":{"id":format!("budget-{source_id}"),"maxRequests":16,"maxTokens":200_000,"startedAt":1000},"now":1000})).unwrap();
        engine
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":100},"now":1001}),
            )
            .unwrap();
        drop(engine);
        eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
        assert_eq!(
            call(
                &path,
                "POST",
                &format!(
                    "{base}/work-tasks/{}/provider-usage",
                    preview["workTaskId"].as_str().unwrap()
                ),
                json!({"taskId":source_id}),
                AUTHOR
            )
            .await
            .0,
            StatusCode::OK
        );
        let claim = json!({"operation":"generateBrief",
            "inputRevision":0,"threadId":"child-thread","authorIntent":"不可替换作者原话",
            "target":{"type":"artifact","requestedByTaskId":source_id,"requestedActionId":first["id"]},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600
        });
        let (status, child) = call(
            &path,
            "POST",
            &format!("{base}/tasks"),
            claim.clone(),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{child}");
        let budget_uri = format!(
            "{base}/tasks/{}/provider-budget",
            child["id"].as_str().unwrap()
        );
        let (status, budget) = call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await;
        assert_eq!(status, StatusCode::OK, "{budget}");
        assert_eq!(
            (
                budget["workTaskId"].clone(),
                budget["maxRequests"].clone(),
                budget["maxTokens"].clone()
            ),
            (preview["workTaskId"].clone(), json!(15), json!(199900))
        );
        db.execute("UPDATE story_execution_scopes_v1 SET scope_json=json_set(scope_json,'$.request.maxActions',3)",[]).unwrap();
        assert!(
            !call(&path, "GET", &scope_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert!(
            !call(&path, "GET", &permission_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert!(
            !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        db.execute(
            "UPDATE story_execution_scopes_v1 SET scope_json=?1",
            [permit["scope"].to_string()],
        )
        .unwrap();
        assert_eq!(
            call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await.0,
            StatusCode::OK
        );
        let child_id = child["id"].as_str().unwrap();
        assert_eq!(
            call(
                &path,
                "PATCH",
                &format!("{base}/tasks/{child_id}"),
                json!({"threadId":"child-thread","status":"failed","error":"interrupted"}),
                AUTHOR
            )
            .await
            .0,
            StatusCode::OK
        );
        let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,child_id,
            json!({"options":{"id":format!("budget-{child_id}"),"maxRequests":15,"maxTokens":199900,"startedAt":1000},"now":1000})).unwrap();
        engine
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":100},"now":1001}),
            )
            .unwrap();
        drop(engine);
        eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
        assert_eq!(
            call(
                &path,
                "POST",
                &format!(
                    "{base}/work-tasks/{}/provider-usage",
                    preview["workTaskId"].as_str().unwrap()
                ),
                json!({"taskId":child_id}),
                AUTHOR
            )
            .await
            .0,
            StatusCode::OK
        );
        let project =
            novel_codex_story_core::ProjectId::parse(base.trim_start_matches("/api/projects/"))
                .unwrap();
        let mut repo = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
        let work = repo.work_task_for_attempt(&project, child_id).unwrap();
        repo.control_work_task(&project, &work.id, "resume", work.version, 2)
            .unwrap();
        let (status, rejected) = call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR).await;
        assert!(!status.is_success(), "{rejected}");
        assert!(rejected.to_string().contains("attempt limit"), "{rejected}");
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
}

#[tokio::test]
async fn stale_scope_preview_and_late_queue_failure_never_leave_partial_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rollback.sqlite3");
    let (base, task, first) = setup(&path).await;
    let scope_uri = format!(
        "{base}/tasks/{}/execution-scope",
        task["id"].as_str().unwrap()
    );
    let request = json!({"actionIds":[first["id"]],"maxActions":2,"maxTaskAttempts":8,"maxAdvisoryRounds":0,"expectedIntentRevision":0});
    let (_, old) = call(
        &path,
        "POST",
        &format!("{scope_uri}/preview"),
        request.clone(),
        AUTHOR,
    )
    .await;
    propose(&path, &base, &task, "second", "generateOutline").await;
    let old_confirm = json!({"request":request,"previewDigest":old["previewDigest"]});
    assert!(
        !call(&path, "POST", &scope_uri, old_confirm, AUTHOR)
            .await
            .0
            .is_success()
    );
    let (_, preview) = call(
        &path,
        "POST",
        &format!("{scope_uri}/preview"),
        request.clone(),
        AUTHOR,
    )
    .await;
    let confirm = json!({"request":request,"previewDigest":preview["previewDigest"]});
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_scope_queue BEFORE INSERT ON story_work_steps_v1 BEGIN SELECT RAISE(ABORT,'scope queue failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &scope_uri, confirm.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_execution_scopes_v1),(SELECT COUNT(*) FROM story_action_permits_v1),(SELECT COUNT(*) FROM story_work_steps_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(counts, (0, 0, 1));
    db.execute_batch("DROP TRIGGER fail_scope_queue;").unwrap();
    assert_eq!(
        call(&path, "POST", &scope_uri, confirm, AUTHOR).await.0,
        StatusCode::OK
    );
    let project =
        novel_codex_story_core::ProjectId::parse(base.trim_start_matches("/api/projects/"))
            .unwrap();
    let mut repo = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
    repo.append_task_intent(
        &project,
        task["id"].as_str().unwrap(),
        0,
        "改为只讨论，不生成",
        1,
    )
    .unwrap();
    assert_eq!(
        call(&path, "GET", &scope_uri, Value::Null, AUTHOR).await.1["scope"]["status"],
        "superseded"
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM story_work_steps_v1 WHERE id=?1",
            [first["id"].as_str().unwrap()],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "superseded"
    );
}

#[tokio::test]
async fn finite_scope_cannot_cross_project_or_reactivate_paused_or_cancelled_work() {
    for command in ["pause", "cancel"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("control.sqlite3");
        let (base, task, first) = setup(&path).await;
        let source = task["id"].as_str().unwrap();
        let uri = format!("{base}/tasks/{source}/execution-scope");
        let request = json!({"actionIds":[first["id"]],"maxActions":2,"maxTaskAttempts":8,"maxAdvisoryRounds":0,"expectedIntentRevision":0});
        let (status, preview) = call(
            &path,
            "POST",
            &format!("{uri}/preview"),
            request.clone(),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        let (status, other) = call(
            &path,
            "POST",
            "/api/projects",
            json!({"title":"另一个作品"}),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let other_uri = format!(
            "/api/projects/{}/tasks/{source}/execution-scope",
            other["project"]["id"].as_str().unwrap()
        );
        assert!(
            !call(
                &path,
                "POST",
                &format!("{other_uri}/preview"),
                request.clone(),
                AUTHOR
            )
            .await
            .0
            .is_success()
        );
        assert_eq!(
            call(&path, "GET", &other_uri, Value::Null, AUTHOR).await.1,
            json!({"scope":null})
        );
        let project =
            novel_codex_story_core::ProjectId::parse(base.trim_start_matches("/api/projects/"))
                .unwrap();
        let mut repo = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
        let work = repo.work_task_for_attempt(&project, source).unwrap();
        let stopped = repo
            .control_work_task(&project, &work.id, command, work.version, 2)
            .unwrap();
        let confirm = json!({"request":request,"previewDigest":preview["previewDigest"]});
        assert!(
            !call(&path, "POST", &uri, confirm, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert!(
            !call(&path, "POST", &format!("{uri}/preview"), request, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert_eq!(
            repo.work_task_for_attempt(&project, source).unwrap(),
            stopped
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_execution_scopes_v1),(SELECT COUNT(*) FROM story_action_permits_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(counts, (0, 0, 1));
    }
}
