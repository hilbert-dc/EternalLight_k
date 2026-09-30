use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository, TaskUpdate};
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

const AUTHOR: &str = "work-budget-author-12345678901234567890";
const CANDIDATE: &str = "work-budget-candidate-12345678901234567";

#[path = "work_budget/delivery_tests.rs"]
mod delivery;
#[path = "work_budget/initial_tests.rs"]
mod initial;
#[path = "work_budget/review_tests.rs"]
mod review;
#[path = "work_budget/review_author_source_tests.rs"]
mod review_author_source;

#[tokio::test]
async fn ordinary_retry_reserves_atomically_and_cannot_run_ahead_of_unsettled_usage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ordinary-retry.sqlite3");
    let (project, task, work, mut authorization) = setup(&path).await;
    let project_id = ProjectId::parse(&project).unwrap();
    let root = format!("/api/projects/{project}");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.update_task(
        &project_id,
        &task,
        "budget-thread",
        TaskUpdate {
            status: "failed".into(),
            error: Some("interrupted".into()),
            result_summary: None,
            now: 1,
        },
    )
    .unwrap();
    let before = repo.work_task(&project_id, &work).unwrap();
    authorization["expectedVersion"] = json!(before.version);
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{root}/work-tasks/{work}/budget-authorization"),
            authorization.clone(),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let confirmed = repo.work_task(&project_id, &work).unwrap();
    let resumed = repo
        .control_work_task(&project_id, &work, "resume", confirmed.version, 2)
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut target = resumed.steps[0].target.clone();
    target["resumeStepId"] = json!(resumed.steps[0].id);
    let request = json!({"operation":"advisory","inputRevision":0,"threadId":"budget-thread",
        "target":target,"authorIntent":"继续分析人物","capabilityHash":"a".repeat(64),"capabilityExpiresAt":now+600});
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_ordinary_grant BEFORE INSERT ON story_task_provider_budgets_v1 BEGIN SELECT RAISE(ABORT, 'grant failure'); END;").unwrap();
    assert!(
        !call(
            &path,
            "POST",
            &format!("{root}/tasks"),
            request.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(repo.work_task(&project_id, &work).unwrap(), resumed);
    assert_eq!(repo.tasks(&project_id).unwrap().len(), 1);
    db.execute_batch("DROP TRIGGER reject_ordinary_grant;")
        .unwrap();
    let (status, next) = call(
        &path,
        "POST",
        &format!("{root}/tasks"),
        request.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{next}");
    let next_id = next["id"].as_str().unwrap();
    let usage_uri = format!("{root}/work-tasks/{work}/provider-usage");
    assert_eq!(
        call(&path, "GET", &usage_uri, Value::Null, AUTHOR).await,
        (
            StatusCode::OK,
            json!({"pending":{"taskId":next_id,"status":"queued"}})
        )
    );
    assert_eq!(
        call(&path, "GET", &usage_uri, Value::Null, CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let budget_uri = format!("{root}/tasks/{next_id}/provider-budget");
    let expected = json!({"taskId":next_id,"workTaskId":work,"budgetId":format!("budget-{next_id}"),
        "maxRequests":10,"maxTokens":1000,"deadlineAt":authorization["deadlineAt"]});
    assert_eq!(
        call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, expected.clone())
    );
    assert_eq!(
        call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, expected)
    );
    assert_eq!(
        call(&path, "GET", &budget_uri, Value::Null, CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        !call(
            &path,
            "GET",
            &format!("{root}/tasks/{task}/provider-budget"),
            Value::Null,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    repo.update_task(
        &project_id,
        next_id,
        "budget-thread",
        TaskUpdate {
            status: "failed".into(),
            error: Some("interrupted again".into()),
            result_summary: None,
            now: 3,
        },
    )
    .unwrap();
    let failed = repo.work_task(&project_id, &work).unwrap();
    let resumed = repo
        .control_work_task(&project_id, &work, "resume", failed.version, 4)
        .unwrap();
    assert!(
        !call(
            &path,
            "POST",
            &format!("{root}/tasks"),
            request.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(repo.work_task(&project_id, &work).unwrap(), resumed);
    assert_eq!(repo.tasks(&project_id).unwrap().len(), 2);
    let usage_request = json!({"taskId":next_id});
    assert_eq!(
        call(&path, "GET", &usage_uri, Value::Null, AUTHOR).await,
        (
            StatusCode::OK,
            json!({"pending":{"taskId":next_id,"status":"failed"}})
        )
    );
    assert_eq!(repo.work_task(&project_id, &work).unwrap(), resumed);
    assert!(
        !call(&path, "POST", &usage_uri, usage_request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let engine_path = path.with_extension("engine.sqlite3");
    let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,next_id,
        json!({"options":{"id":format!("budget-{next_id}"),"maxRequests":10,"maxTokens":1000,"startedAt":1000},"now":1000})).unwrap();
    engine
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":100},"now":1001}),
        )
        .unwrap();
    assert!(
        !call(&path, "POST", &usage_uri, usage_request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    drop(engine);
    eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
    assert_eq!(
        call(&path, "POST", &usage_uri, usage_request.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &usage_uri,
            json!({"taskId":next_id,"usedTokens":0}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    db.execute_batch("CREATE TRIGGER reject_usage BEFORE UPDATE ON story_work_budgets_v1 BEGIN SELECT RAISE(ABORT, 'ledger failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &usage_uri, usage_request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let unsettled: i64 = db
        .query_row(
            "SELECT settled FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [next_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unsettled, 0);
    assert_eq!(repo.work_task(&project_id, &work).unwrap(), resumed);
    db.execute_batch("DROP TRIGGER reject_usage;").unwrap();
    let (status, charged) = call(&path, "POST", &usage_uri, usage_request.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    assert_eq!(
        (
            charged["usedRequests"].clone(),
            charged["unknownTokens"].clone()
        ),
        (json!(1), json!(100))
    );
    let after_charge = repo.work_task(&project_id, &work).unwrap();
    assert_eq!(
        call(&path, "GET", &usage_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"pending":null}))
    );
    assert_eq!(
        call(&path, "POST", &usage_uri, usage_request, AUTHOR).await,
        (StatusCode::OK, charged)
    );
    assert_eq!(repo.work_task(&project_id, &work).unwrap(), after_charge);
    let (status, retry) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{retry}");
    let retry_id = retry["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{root}/tasks/{retry_id}/provider-budget"),
            Value::Null,
            AUTHOR
        )
        .await,
        (
            StatusCode::OK,
            json!({"taskId":retry_id,"workTaskId":work,"budgetId":format!("budget-{retry_id}"),
            "maxRequests":9,"maxTokens":900,"deadlineAt":authorization["deadlineAt"]})
        )
    );
}

#[tokio::test]
async fn unfunded_existing_work_cannot_consume_a_resumable_step() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unfunded-step.sqlite3");
    let (project, task, work, _) = setup(&path).await;
    let project_id = ProjectId::parse(&project).unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.update_task(
        &project_id,
        &task,
        "budget-thread",
        TaskUpdate {
            status: "failed".into(),
            error: Some("fixture interrupted before execution".into()),
            result_summary: None,
            now: 1,
        },
    )
    .unwrap();
    let before = repo.work_task(&project_id, &work).unwrap();
    let resumed = repo
        .control_work_task(&project_id, &work, "resume", before.version, 2)
        .unwrap();
    let mut target = resumed.steps[0].target.clone();
    target["resumeStepId"] = json!(resumed.steps[0].id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status, _) = call(
        &path,
        "POST",
        &format!("/api/projects/{project}/tasks"),
        json!({
            "operation":"advisory","inputRevision":0,"threadId":"budget-thread","target":target,
            "authorIntent":"继续分析","capabilityHash":"a".repeat(64),"capabilityExpiresAt":now+600,
        }),
        AUTHOR,
    )
    .await;
    assert!(!status.is_success());
    assert_eq!(repo.work_task(&project_id, &work).unwrap(), resumed);
    assert_eq!(repo.tasks(&project_id).unwrap().len(), 1);
    let db = rusqlite::Connection::open(path).unwrap();
    let counts: (i64,i64) = db.query_row(
        "SELECT (SELECT COUNT(*) FROM story_work_attempts_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(counts, (1, 0));
}

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let app = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
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
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}

async fn setup(path: &std::path::Path) -> (String, String, String, Value) {
    let (status, created) = call(
        path,
        "POST",
        "/api/projects",
        json!({"title":"额度确认"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap().to_owned();
    let root = format!("/api/projects/{project}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Seed repository-only state to exercise approval of future attempts. The
    // raw HTTP boundary is tested separately and must never create this state.
    let task = format!("task-{}", uuid::Uuid::new_v4().simple());
    let mut repo = SqliteStoryRepository::open(path).unwrap();
    repo.create_task(novel_codex_story_core::CreateTaskRequest {
        task_id: task.clone(),
        project_id: ProjectId::parse(&project).unwrap(),
        thread_id: "budget-thread".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"type":"project"}).to_string(),
        input_manifest_json: json!({"operation":"advisory","projectId":project,
            "projectRevision":0,"target":{"type":"project"},"authorIntent":"分析人物"})
        .to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: (now + 600) as i64,
    })
    .unwrap();
    let work = repo
        .work_task_for_attempt(&ProjectId::parse(&project).unwrap(), &task)
        .unwrap();
    let (status, workspace) = call(
        path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    (
        project,
        task,
        work.id,
        json!({"expectedVersion":work.version,"expectedProjectRevision":0,
        "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":10,"maxTokens":1000,"deadlineAt":now+600}),
    )
}

fn finish(path: &std::path::Path, project: &str, task: &str) -> i64 {
    let mut repo = SqliteStoryRepository::open(path).unwrap();
    let project = ProjectId::parse(project).unwrap();
    repo.complete_task(&project, task, "budget-thread", "分析完成".into(), 1)
        .unwrap();
    repo.work_task_for_attempt(&project, task).unwrap().version
}

#[tokio::test]
async fn author_confirmation_is_frozen_durable_and_does_not_restart_work() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite3");
    let (project, task, work, mut request) = setup(&path).await;
    let uri = format!("/api/projects/{project}/work-tasks/{work}/budget-authorization");
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"authorization":null}))
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        !call(&path, "POST", &uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    request["expectedVersion"] = json!(finish(&path, &project, &task));
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let before = SqliteStoryRepository::open(&path)
        .unwrap()
        .work_task(&ProjectId::parse(&project).unwrap(), &work)
        .unwrap();
    let expected = json!({"maxRequests":10,"maxTokens":1000,"usedRequests":0,"usedTokens":0,
        "unknownTokens":0,"overrun":false,"charges":[]});
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), AUTHOR).await,
        (StatusCode::OK, expected.clone())
    );
    // Each call opens a new repository: retry after reconnect must not allocate twice.
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), AUTHOR).await,
        (StatusCode::OK, expected)
    );
    let mut expected_work = before;
    expected_work.version += 1;
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"authorization":request}))
    );
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .work_task(&ProjectId::parse(&project).unwrap(), &work)
            .unwrap(),
        expected_work
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let saved:(String,i64,i64) = db.query_row(
        "SELECT authorization_json,after_attempt_sequence,(SELECT MAX(sequence) FROM story_work_attempts_v1) FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",
        [&work],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&saved.0).unwrap(), request);
    assert_eq!(saved.1, saved.2);
    // A missing ledger is corruption, never an absent authorization permitting fallback.
    db.execute(
        "UPDATE story_work_budgets_v1 SET ledger_json='{}' WHERE work_task_id=?1",
        [&work],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
    db.execute("UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",rusqlite::params![json!({"maxRequests":10,"maxTokens":1000,"usedRequests":0,"usedTokens":0,"unknownTokens":0,"overrun":false,"charges":[]}).to_string(),work]).unwrap();
    request["maxTokens"] = json!(2000);
    assert!(
        !call(&path, "POST", &uri, request, AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .work_task(&ProjectId::parse(&project).unwrap(), &work)
            .unwrap(),
        expected_work
    );
}

#[tokio::test]
async fn invalid_confirmation_and_late_storage_failure_leave_no_allowance() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rejected.sqlite3");
    let (project, task, work, mut request) = setup(&path).await;
    request["expectedVersion"] = json!(finish(&path, &project, &task));
    let uri = format!("/api/projects/{project}/work-tasks/{work}/budget-authorization");
    for (field, value) in [
        ("expectedVersion", json!(-1)),
        ("expectedProjectRevision", json!(1)),
        ("expectedSourceDigest", json!("stale")),
        ("maxRequests", json!(0)),
        ("maxTokens", json!(16_000_001)),
        ("deadlineAt", json!(0)),
        ("usedTokens", json!(0)),
    ] {
        let mut invalid = request.clone();
        invalid[field] = value;
        assert!(
            !call(&path, "POST", &uri, invalid, AUTHOR)
                .await
                .0
                .is_success(),
            "{field}"
        );
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_work_version BEFORE UPDATE ON story_work_tasks_v1 BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let state:(i64,i64,i64) = db.query_row(
        "SELECT (SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1),version FROM story_work_tasks_v1 WHERE id=?1",
        [&work],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(state, (0, 0, request["expectedVersion"].as_i64().unwrap()));
    db.execute_batch("DROP TRIGGER reject_work_version;")
        .unwrap();
    assert_eq!(
        call(&path, "POST", &uri, request, AUTHOR).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn confirmation_cannot_cross_project_or_replace_generation_or_cancelled_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scope.sqlite3");
    let (project, task, work, mut request) = setup(&path).await;
    request["expectedVersion"] = json!(finish(&path, &project, &task));
    let uri = format!("/api/projects/{project}/work-tasks/{work}/budget-authorization");
    let (_, other) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"title":"另一个作品"}),
        AUTHOR,
    )
    .await;
    let foreign = uri.replace(&project, other["project"]["id"].as_str().unwrap());
    assert!(
        !call(&path, "POST", &foreign, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    // A generation account already has its own frozen authorization and must not be reissued.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let root = format!("/api/projects/{project}");
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (status,run) = call(&path,"POST",&format!("{root}/generation-runs"),json!({
        "expectedProjectRevision":0,"expectedBaseSnapshotId":workspace["headSnapshotId"],
        "expectedSourceDigest":workspace["sourceDigest"],"authorIntent":"继续港城故事","targetChapters":1,
        "maxTaskAttempts":7,"maxProviderRequests":10,"maxProviderTokens":1000,"deadlineAt":now+600
    }),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let generation_uri = format!(
        "{root}/work-tasks/work-{}/budget-authorization",
        run["id"].as_str().unwrap()
    );
    assert!(
        !call(&path, "POST", &generation_uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let (cancelled_project, _, cancelled_work, mut cancelled_request) = setup(&path).await;
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let cancelled = repo
        .control_work_task(
            &ProjectId::parse(&cancelled_project).unwrap(),
            &cancelled_work,
            "cancel",
            cancelled_request["expectedVersion"].as_i64().unwrap(),
            now as i64,
        )
        .unwrap();
    cancelled_request["expectedVersion"] = json!(cancelled.version);
    let cancelled_uri = format!(
        "/api/projects/{cancelled_project}/work-tasks/{cancelled_work}/budget-authorization"
    );
    assert!(
        !call(&path, "POST", &cancelled_uri, cancelled_request, AUTHOR)
            .await
            .0
            .is_success()
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts:(i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_work_budget_authorizations_v1),(SELECT COUNT(*) FROM story_work_budgets_v1 WHERE work_task_id=?1)",
        [&work],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(counts, (0, 0));
}
