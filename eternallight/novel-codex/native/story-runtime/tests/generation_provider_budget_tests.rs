use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, RecoveryOperationBinding, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "provider-budget-author-1234567890123456";
const CANDIDATE: &str = "provider-budget-candidate-1234567890123";
const CAPABILITY: &str = "provider-budget-capability-1234567890123456";

#[tokio::test]
async fn generation_claim_inherits_metered_authority_without_a_default_quota() {
    use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
    let directory = tempfile::tempdir().unwrap();
    let story_db = directory.path().join("metered-generation.sqlite3");
    let app = router(task_fixture::state(
        story_db.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"无额度截断的创作"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let (status, run) = call(&app, "POST", &format!("{root}/generation-runs"), Some(json!({
        "expectedProjectRevision":0,"authorIntent":"写港城故事","targetChapters":1,
        "maxTaskAttempts":null,"maxProviderRequests":null,"maxProviderTokens":null,"deadlineAt":null
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
    let (status, next) = call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status, task) = call(&app, "POST", &format!("{root}/tasks"), Some(json!({
        "operation":next["action"]["operation"],"inputRevision":0,"threadId":"thread",
        "target":next["action"]["target"],"authorIntent":"写港城故事",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+3600
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let repository = SqliteStoryRepository::open(&story_db).unwrap();
    let grant = repository
        .task_provider_budget(&ProjectId::parse(project).unwrap(), task_id, now + 1800)
        .unwrap();
    assert_eq!(
        serde_json::to_value(grant).unwrap(),
        json!({
            "taskId":task_id,"workTaskId":format!("work-{}",run["id"].as_str().unwrap()),
            "budgetId":format!("budget-{task_id}"),"maxRequests":null,"maxTokens":null,"deadlineAt":null
        })
    );
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut body = body;
    if method == "POST"
        && uri.ends_with("/generation-runs")
        && let Some(payload) = body.as_mut()
        && payload.get("expectedBaseSnapshotId").is_none()
    {
        let workspace_uri = format!(
            "{}/creative-workspace",
            uri.trim_end_matches("/generation-runs")
        );
        let (_, workspace) = Box::pin(call(app, "GET", &workspace_uri, None)).await;
        payload["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
        payload["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    }
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {AUTHOR}"))
                .header("content-type", "application/json")
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
    )
}

#[tokio::test]
async fn terminal_engine_usage_is_charged_once_and_exhausted_attempt_cannot_retry() {
    let directory = tempfile::tempdir().unwrap();
    let story_db = directory.path().join("story.sqlite3");
    let engine_db = directory.path().join("engine-budget.sqlite3");
    let app = router(
        task_fixture::state(story_db.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(engine_db.clone()),
    );
    let (status, created) =
        call(&app, "POST", "/api/projects", Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status, run) = call(
        &app,
        "POST",
        &format!("{root}/generation-runs"),
        Some(json!({
            "expectedProjectRevision":0,"authorIntent":"写港城故事","targetChapters":1,
            "maxTaskAttempts":7,"maxProviderRequests":2,"maxProviderTokens":15,"deadlineAt":now+600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{run}");
    let (status, run) = call(
        &app,
        "POST",
        &format!("{root}/generation-runs"),
        Some(json!({
            "expectedProjectRevision":0,"authorIntent":"写港城故事","targetChapters":1,
            "maxTaskAttempts":7,"maxProviderRequests":7,"maxProviderTokens":15,"deadlineAt":now+600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
    let (_, next) = call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    let claim = json!({
        "operation":next["action"]["operation"],"inputRevision":0,"threadId":"thread",
        "target":next["action"]["target"],"authorIntent":"写港城故事",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":now+600
    });
    let fault = rusqlite::Connection::open(&story_db).unwrap();
    let counts_before: (i64,i64,i64) = fault.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)", [], |row| Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?,row.get(/*idx*/2)?))).unwrap();
    let before_claim = call(&app, "GET", &run_uri, None).await.1;
    fault.execute_batch("CREATE TRIGGER fail_budget_claim BEFORE INSERT ON story_task_provider_budgets_v1 BEGIN SELECT RAISE(ABORT, 'claim failure'); END;").unwrap();
    assert!(
        !call(&app, "POST", &format!("{root}/tasks"), Some(claim.clone()))
            .await
            .0
            .is_success()
    );
    assert_eq!(call(&app, "GET", &run_uri, None).await.1, before_claim);
    let counts: (i64,i64,i64) = fault.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)", [], |row| Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?,row.get(/*idx*/2)?))).unwrap();
    assert_eq!(counts, counts_before);
    fault
        .execute_batch("DROP TRIGGER fail_budget_claim;")
        .unwrap();
    let (status, task) = call(&app, "POST", &format!("{root}/tasks"), Some(claim)).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let grant_uri = format!("{root}/tasks/{task_id}/provider-budget");
    let run_before_grant = call(&app, "GET", &run_uri, None).await.1;
    let (status, grant) = call(&app, "GET", &grant_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    assert_eq!(
        grant,
        json!({
            "taskId":task_id,"workTaskId":format!("work-{}",run["id"].as_str().unwrap()),
            "budgetId":format!("budget-{task_id}"),"maxRequests":7,"maxTokens":15,"deadlineAt":now+600
        })
    );
    assert_eq!(call(&app, "GET", &grant_uri, None).await.1, grant);
    let persisted: String = fault
        .query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&persisted).unwrap(), grant);
    let reopened = router(
        task_fixture::state(story_db.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(engine_db.clone()),
    );
    assert_eq!(call(&reopened, "GET", &grant_uri, None).await.1, grant);
    assert_eq!(call(&app, "GET", &run_uri, None).await.1, run_before_grant);
    let candidate = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .uri(&grant_uri)
                .header("authorization", format!("Bearer {CANDIDATE}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(!candidate.status().is_success());
    let foreign = format!("/api/projects/foreign/tasks/{task_id}/provider-budget");
    assert!(!call(&app, "GET", &foreign, None).await.0.is_success());
    let (status, failed) = call(
        &app,
        "PATCH",
        &format!("{root}/tasks/{task_id}"),
        Some(json!({
            "threadId":"thread","status":"failed","error":"provider disconnected"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    let (status, observed) = call(&app, "GET", &format!("{root}/tasks/{task_id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{observed}");
    assert_eq!(observed["status"], "failed");
    assert!(!call(&app, "GET", &grant_uri, None).await.0.is_success());
    let charge_uri = format!("{run_uri}/provider-usage");
    let (status, missing) = call(&app, "POST", &charge_uri, Some(json!({"taskId":task_id}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{missing}");
    let (_, blocked_without_receipt) =
        call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    let (status, uncharged_retry) = call(
        &app,
        "POST",
        &format!("{run_uri}/retry-failed-task"),
        Some(json!({"expectedVersion":blocked_without_receipt["run"]["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{uncharged_retry}");
    let (status, spoof) = call(
        &app,
        "POST",
        &charge_uri,
        Some(json!({"taskId":task_id,"receipt":{"usage":{"requests":0}}})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{spoof}");

    let mut budget = PersistentBudget::new(
        &engine_db,
        task_id,
        json!({"options":{
        "id":format!("budget-{task_id}"),"maxRequests":7,"maxTokens":15,"startedAt":1000
    },"now":1000}),
    )
    .unwrap();
    let settled = budget
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":5},"now":1001}),
        )
        .unwrap();
    budget
        .apply(
            "budget.settle",
            json!({"id":settled,"usage":{"total_tokens":7}}),
        )
        .unwrap();
    let unknown_reservation = budget
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":8},"now":1002}),
        )
        .unwrap();
    let (status, active) = call(&app, "POST", &charge_uri, Some(json!({"taskId":task_id}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{active}");
    assert_eq!(PersistentBudget::recover(&engine_db).unwrap(), json!([]));
    drop(budget);
    let recovered = PersistentBudget::recover(&engine_db).unwrap();
    assert_eq!(recovered.as_array().unwrap().len(), 1);
    assert_eq!(
        PersistentBudget::receipt(&engine_db, task_id).unwrap()["status"],
        "recovered_unknown"
    );
    let db = rusqlite::Connection::open(&story_db).unwrap();
    let before = call(&app, "GET", &run_uri, None).await.1;
    let settlement = || {
        db.query_row(
            "SELECT settled FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [task_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(settlement(), 0);
    let mut unrelated_grant = grant.clone();
    unrelated_grant["budgetId"] = json!("another-budget");
    db.execute(
        "UPDATE story_task_provider_budgets_v1 SET grant_json=?1 WHERE task_id=?2",
        rusqlite::params![unrelated_grant.to_string(), task_id],
    )
    .unwrap();
    assert!(
        !call(&app, "POST", &charge_uri, Some(json!({"taskId":task_id})))
            .await
            .0
            .is_success()
    );
    assert_eq!(call(&app, "GET", &run_uri, None).await.1, before);
    db.execute(
        "UPDATE story_task_provider_budgets_v1 SET grant_json=?1 WHERE task_id=?2",
        rusqlite::params![grant.to_string(), task_id],
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER reject_budget_write BEFORE UPDATE ON story_work_budgets_v1 BEGIN SELECT RAISE(ABORT, 'test ledger write failure'); END;").unwrap();
    let (status, _) = call(&app, "POST", &charge_uri, Some(json!({"taskId":task_id}))).await;
    assert!(!status.is_success());
    assert_eq!(call(&app, "GET", &run_uri, None).await.1, before);
    db.execute_batch("DROP TRIGGER reject_budget_write;")
        .unwrap();
    assert_eq!(settlement(), 0);
    let (status, charged) = call(&app, "POST", &charge_uri, Some(json!({"taskId":task_id}))).await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    assert_eq!(settlement(), 1);
    assert_eq!(charged["providerBudget"]["usedRequests"], 2);
    assert_eq!(charged["providerBudget"]["usedTokens"], 7);
    assert_eq!(charged["providerBudget"]["unknownTokens"], 8);
    let (stored_state, ledger): (String, String) = db.query_row(
        "SELECT r.state_json,b.ledger_json FROM story_generation_runs_v1 r JOIN story_work_budgets_v1 b ON b.work_task_id='work-' || r.run_id WHERE r.run_id=?1",
        [run["id"].as_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert!(
        serde_json::from_str::<Value>(&stored_state)
            .unwrap()
            .get("providerBudget")
            .is_none()
    );
    assert_eq!(
        serde_json::from_str::<Value>(&ledger).unwrap(),
        charged["providerBudget"]
    );
    assert_eq!(
        charged["providerBudget"]["charges"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, repeated) = call(&app, "POST", &charge_uri, Some(json!({"taskId":task_id}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated, charged);
    let (_, blocked) = call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    let (status, retry) = call(
        &app,
        "POST",
        &format!("{run_uri}/retry-failed-task"),
        Some(json!({"expectedVersion":blocked["run"]["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{retry}");
    assert_eq!(
        call(&app, "GET", &run_uri, None).await.1["providerBudget"],
        charged["providerBudget"]
    );
    let before_reconciliation = call(&app, "GET", &run_uri, None).await.1;
    PersistentBudget::reconcile_usage(&engine_db,task_id,json!({"id":unknown_reservation,"usage":{"total_tokens":3},"receiptRef":"verified-late-provider-receipt"})).unwrap();
    let (status, reconciled) = call(&app,"POST",&charge_uri,Some(json!({"taskId":task_id}))).await;
    assert_eq!(status,StatusCode::OK,"{reconciled}");
    assert_eq!(reconciled["providerBudget"]["unknownTokens"],0);
    assert_eq!(reconciled["providerBudget"]["usedTokens"],10);
    assert_eq!(reconciled["providerBudget"]["usedRequests"],2);
    assert_eq!(reconciled["status"],before_reconciliation["status"]);
    assert_eq!(reconciled["activeTaskId"],before_reconciliation["activeTaskId"]);
    let (status, replay) = call(&app,"POST",&charge_uri,Some(json!({"taskId":task_id}))).await;
    assert_eq!(status,StatusCode::OK); assert_eq!(replay,reconciled);
}

#[tokio::test]
async fn restart_recovers_an_active_task_and_unknown_budget_before_author_retry() {
    restart_recovery(/*unknown_native_call*/ false).await;
}

#[tokio::test]
async fn restart_with_a_real_unresolved_native_call_blocks_author_retry_without_refunding() {
    restart_recovery(/*unknown_native_call*/ true).await;
}

async fn restart_recovery(unknown_native_call: bool) {
    let directory = tempfile::tempdir().unwrap();
    let story_db = directory.path().join("story.sqlite3");
    let engine_db = directory.path().join("engine-budget.sqlite3");
    let app = router(
        task_fixture::state(story_db.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(engine_db.clone()),
    );
    let (status, created) =
        call(&app, "POST", "/api/projects", Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let root = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status, run) = call(
        &app,
        "POST",
        &format!("{root}/generation-runs"),
        Some(json!({
            "expectedProjectRevision":0,"authorIntent":"写港城故事","targetChapters":1,
            "maxTaskAttempts":8,"maxProviderRequests":8,"maxProviderTokens":100,
            "deadlineAt":now+600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
    let (_, next) = call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    let (status, task) = call(
        &app,
        "POST",
        &format!("{root}/tasks"),
        Some(json!({
            "operation":next["action"]["operation"],"inputRevision":0,"threadId":"thread",
            "target":next["action"]["target"],"authorIntent":"写港城故事",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":now+600
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let project = ProjectId::parse(created["project"]["id"].as_str().unwrap()).unwrap();
    let (status, grant) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task_id}/provider-budget"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    assert_eq!(grant["budgetId"], format!("budget-{task_id}"));
    assert_eq!(grant["maxRequests"], 8);
    assert_eq!(grant["maxTokens"], 100);
    assert_eq!(grant["deadlineAt"], now + 600);
    let unresolved_binding = if unknown_native_call {
        let (status, started) = call(
            &app,
            "PATCH",
            &format!("{root}/tasks/{task_id}"),
            Some(json!({"threadId":"thread","status":"running","error":null})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{started}");
        assert_eq!(started["status"], "running");
        let (status, turn_bound) = call(
            &app,
            "POST",
            &format!("{root}/tasks/{task_id}/turn"),
            Some(json!({"threadId":"thread","turnId":"interrupted-provider-turn"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{turn_bound}");
        assert_eq!(turn_bound["turnId"], "interrupted-provider-turn");
        let mut repo = SqliteStoryRepository::open(&story_db).unwrap();
        let record = repo.task(&project, task_id).unwrap().unwrap();
        assert_eq!(record.status, "running");
        assert_eq!(record.operation, "advisory");
        assert_eq!(record.thread_id, "thread");
        assert_eq!(record.turn_id.as_deref(), Some("interrupted-provider-turn"));
        let mut decision = repo.task_work_decision(&record, None).unwrap();
        let mut cursor = decision["pagination"]["nextCursor"].clone();
        let mut seen_cursors = std::collections::BTreeSet::new();
        while let Some(current) = cursor.as_str() {
            assert!(seen_cursors.insert(current.to_owned()));
            let page = repo.task_work_decision(&record, Some(current)).unwrap();
            assert_eq!(page["version"], decision["version"]);
            assert_eq!(page["sourceDigest"], decision["sourceDigest"]);
            decision["readyActions"]
                .as_array_mut()
                .unwrap()
                .extend(page["readyActions"].as_array().unwrap().iter().cloned());
            cursor = page["pagination"]["nextCursor"].clone();
        }
        let action = decision["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["operation"] == "generateBrief")
            .unwrap();
        let arguments = json!({"projectId":project.as_str(),"taskId":task_id,"threadId":"thread",
            "expectedRevision":record.input_revision,"expectedWorkVersion":decision["version"],
            "expectedSourceDigest":decision["sourceDigest"],"actionKey":action["actionKey"],
            "instruction":"按原作者要求整理港城构想","idempotencyKey":"interrupted-work-action"});
        let binding = RecoveryOperationBinding {
            project_id: project.clone(),
            task_id: task_id.to_owned(),
            thread_id: record.thread_id.clone(),
            turn_id: "interrupted-provider-turn".into(),
            call_id: "interrupted-work-action-call".into(),
            tool_name: "request_work_action".into(),
            request_hash: format!(
                "{:x}",
                Sha256::digest(
                    json!({"tool":"request_work_action","arguments":arguments})
                        .to_string()
                        .as_bytes()
                )
            ),
            idempotency_ref: Some(format!(
                "sha256:{:x}",
                Sha256::digest(b"interrupted-work-action")
            )),
            now,
        };
        // Crash after real Host admission and before dispatch/receipt. No fake
        // INSERT or guess about whether the original call ultimately committed.
        repo.bind_recovery_operation(binding.clone()).unwrap();
        let operation = repo
            .read_recovery_operation(&binding, Some(&record.capability_hash))
            .unwrap()
            .unwrap();
        assert_eq!(operation["state"], "accepted");
        assert_eq!(operation["receipt_ref"], Value::Null);
        assert_eq!(operation["run_id"], task_id);
        assert_eq!(operation["thread_id"], "thread");
        assert_eq!(operation["turn_id"], binding.turn_id);
        assert_eq!(operation["tool_call_id"], binding.call_id);
        assert_eq!(operation["tool_name"], "request_work_action");
        assert_eq!(operation["request_hash"], binding.request_hash);
        assert_eq!(operation["idempotency_ref"], json!(binding.idempotency_ref));
        Some((binding, operation))
    } else {
        None
    };
    let mut budget = PersistentBudget::new(
        &engine_db,
        task_id,
        json!({"options":{
            "id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000
        },"now":1000}),
    )
    .unwrap();
    budget
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":20},"now":1001}),
        )
        .unwrap();
    drop(budget);
    assert_eq!(
        PersistentBudget::recover(&engine_db)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, recovered) =
        call(&app, "POST", "/api/tasks/recover-orphans", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{recovered}");
    assert_eq!(recovered["taskIds"], json!([task_id]));
    let (_, task) = call(&app, "GET", &format!("{root}/tasks/{task_id}"), None).await;
    assert_eq!(task["status"], "failed");
    let (_, blocked) = call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    assert_eq!(blocked["run"]["status"], "blocked");
    let (status, uncharged) = call(
        &app,
        "POST",
        &format!("{run_uri}/retry-failed-task"),
        Some(json!({"expectedVersion":blocked["run"]["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{uncharged}");
    let (status, charged) = call(
        &app,
        "POST",
        &format!("{run_uri}/provider-usage"),
        Some(json!({"taskId":task_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    assert_eq!(charged["providerBudget"]["usedRequests"], 1);
    assert_eq!(charged["providerBudget"]["usedTokens"], 0);
    assert_eq!(charged["providerBudget"]["unknownTokens"], 20);
    assert_eq!(
        charged["providerBudget"]["charges"][0]["status"],
        "recovered_unknown"
    );
    let repo = SqliteStoryRepository::open(&story_db).unwrap();
    let failed_task = repo.task(&project, task_id).unwrap().unwrap();
    let work_before = repo.work_task_for_attempt(&project, task_id).unwrap();
    assert_eq!(failed_task.status, "failed");
    assert_eq!(work_before.thread_id, "thread");
    let connection = rusqlite::Connection::open(&story_db).unwrap();
    let counts = || {
        connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .unwrap()
    };
    let owner_rows = || {
        let mut statement = connection
            .prepare("SELECT work_task_id,host_session_id,lease_id,expires_at FROM story_work_execution_owners_v1 ORDER BY work_task_id")
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let counts_before = counts();
    let owners_before = owner_rows();
    let (status, retried) = call(
        &app,
        "POST",
        &format!("{run_uri}/retry-failed-task"),
        Some(json!({"expectedVersion":charged["version"]})),
    )
    .await;
    if let Some((binding, original_operation)) = unresolved_binding {
        assert_eq!(status, StatusCode::CONFLICT, "{retried}");
        assert_eq!(
            retried,
            json!({"error":{"code":"conflict","details":{"origin":"storyRuntime","phase":"domainValidation","instancePath":"/taskId","reason":"task_not_writable","nextAction":"read_task_status"},"message":"TaskNotWritable","recovery":null}})
        );
        let (status, stopped) =
            call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{stopped}");
        assert_eq!(stopped["run"], charged);
        assert_eq!(stopped["run"]["status"], "blocked");
        // Terminal settlement already records the initial execution failure.
        // A later /next must preserve it while refusing the unresolved write.
        assert_eq!(stopped["run"]["blockedReason"], "executionBlocked");
        assert_eq!(stopped["action"], Value::Null);
        assert_eq!(stopped["run"]["providerBudget"], charged["providerBudget"]);
        assert_eq!(repo.task(&project, task_id).unwrap().unwrap(), failed_task);
        assert_eq!(
            repo.work_task_for_attempt(&project, task_id).unwrap(),
            work_before
        );
        assert_eq!(counts(), counts_before);
        assert_eq!(owner_rows(), owners_before);
        assert_eq!(
            repo.read_recovery_operation(&binding, Some(&failed_task.capability_hash))
                .unwrap()
                .unwrap(),
            original_operation
        );
        return;
    }
    assert_eq!(status, StatusCode::OK, "{retried}");
    assert_eq!(retried["status"], "running");
    assert_eq!(retried["providerBudget"], charged["providerBudget"]);
    // Only provider cost was unknown; it has been conservatively charged. The
    // author explicitly retries a read/decision Step, with no Native write call.
    let (status, unresolved) =
        call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{unresolved}");
    assert_eq!(unresolved["run"]["status"], "running");
    assert_eq!(unresolved["stopReason"], Value::Null);
    assert_eq!(unresolved["action"]["kind"], "agentTask");
    assert_eq!(unresolved["action"]["operation"], "advisory");
    assert_eq!(
        unresolved["action"]["target"]["resumeStepId"],
        next["action"]["target"]["resumeStepId"]
    );
    assert_eq!(
        unresolved["action"]["target"]["workDecision"]["workTaskId"],
        work_before.id
    );
    assert_eq!(unresolved["run"]["activeTaskId"], Value::Null);
    assert_eq!(
        unresolved["run"]["usedTaskAttempts"],
        charged["usedTaskAttempts"]
    );
    assert_eq!(unresolved["run"]["retryAttempts"], 1);
    assert_eq!(repo.task(&project, task_id).unwrap().unwrap(), failed_task);
    assert_eq!(
        repo.work_task_for_attempt(&project, task_id)
            .unwrap()
            .thread_id,
        work_before.thread_id
    );
    assert_eq!(counts(), counts_before);
    assert_eq!(owner_rows(), owners_before);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM story_tool_operations_v1 WHERE task_id=?1",
                [task_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        unresolved["run"]["providerBudget"],
        charged["providerBudget"]
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

#[tokio::test]
async fn generation_settles_only_proven_unstarted_attempts_without_an_engine_run() {
    use novel_codex_story_core::TaskUpdate;
    for case in ["unstarted", "active", "turn", "output", "operation", "engineActive", "engineCorrupt"] {
    let directory = tempfile::tempdir().unwrap();
    let story_db = directory.path().join("metered-generation.sqlite3");
    let engine_db = directory.path().join("engine.sqlite3");
    let mut seed = PersistentBudget::new(&engine_db,"seed",json!({"options":{"id":"budget-seed","maxRequests":null,"maxTokens":null,"maxDurationMs":null},"now":1000})).unwrap();
    seed.complete("completed").unwrap();
    let app = router(task_fixture::state(
        story_db.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).with_engine_budget_database(engine_db.clone()));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"无额度截断的创作"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let (status, run) = call(&app, "POST", &format!("{root}/generation-runs"), Some(json!({
        "expectedProjectRevision":0,"authorIntent":"写港城故事","targetChapters":1,
        "maxTaskAttempts":null,"maxProviderRequests":null,"maxProviderTokens":null,"deadlineAt":null
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
    let (status, next) = call(&app, "POST", &format!("{run_uri}/next"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status, task) = call(&app, "POST", &format!("{root}/tasks"), Some(json!({
        "operation":next["action"]["operation"],"inputRevision":0,"threadId":"thread",
        "target":next["action"]["target"],"authorIntent":"写港城故事",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+3600
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();

    let mut repo = SqliteStoryRepository::open(&story_db).unwrap();
    let project_id = ProjectId::parse(project).unwrap();
    if case == "turn" { repo.bind_task_turn(&project_id,task_id,"thread","turn-started").unwrap(); }
    if case != "active" { repo.update_task(&project_id,task_id,"thread",TaskUpdate{status:"failed".into(),error:Some("host setup failed".into()),result_summary:None,now}).unwrap(); }
    let db = rusqlite::Connection::open(&story_db).unwrap();
    if case == "output" { db.execute("UPDATE story_tasks_v2 SET output_refs_json='[{}]' WHERE task_id=?1",[task_id]).unwrap(); }
    if case == "operation" { db.execute("INSERT INTO story_tool_operations_v1 VALUES('op',?1,?2,'thread','turn','call','read_context','hash',NULL,'accepted',NULL,1,1)",rusqlite::params![project,task_id]).unwrap(); }
    let _actual = if case.starts_with("engine") {
        let engine=PersistentBudget::new(&engine_db,task_id,json!({"options":{"id":format!("budget-{task_id}"),"maxRequests":null,"maxTokens":null,"maxDurationMs":null},"now":1000})).unwrap();
        if case == "engineCorrupt" { rusqlite::Connection::open(&engine_db).unwrap().execute("UPDATE engine_budgets SET status='failed',state_json='corrupt' WHERE run_id=?1",[task_id]).unwrap(); }
        Some(engine)
    } else { None };
    let (status,result)=call(&app,"POST",&format!("{run_uri}/provider-usage"),Some(json!({"taskId":task_id}))).await;
    if case == "unstarted" {
        assert_eq!(status,StatusCode::OK,"{result}");
        let settled:i64=db.query_row("SELECT settled FROM story_task_provider_budgets_v1 WHERE task_id=?1",[task_id],|r|r.get(0)).unwrap();
        assert_eq!(settled,1);
        assert_eq!(call(&app,"POST",&format!("{run_uri}/provider-usage"),Some(json!({"taskId":task_id}))).await.0,StatusCode::OK);
    } else { assert_ne!(status,StatusCode::OK,"{case}: {result}"); }
    }
}

use novel_codex_story_core::{RecoveryOperationReadRepository, RecoveryOperationsRepository, TaskProviderBudgetRepository, TasksRepository, WorkDecisionRepository, WorkTasksRepository};
