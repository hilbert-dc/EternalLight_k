use super::{AUTHOR, CANDIDATE, call, followup_claim, ready_continuation};
use axum::http::StatusCode;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository, TaskProviderBudget};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

const ADJUSTMENT: &str = "  保留港城，先分析新的大纲方向 🕯️\n不要自动改稿。  ";

fn charge_one_request(path: &Path, base: &str, task: &Value) {
    let id = task["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let grant: TaskProviderBudget = serde_json::from_str(&stored).unwrap();
    let engine_path = path.with_extension("engine.sqlite3");
    let mut budget = eternallight_engine_core::PersistentBudget::new(&engine_path, id, json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"startedAt":1000},"now":1000})).unwrap();
    budget
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":100},"now":1001}),
        )
        .unwrap();
    budget.complete("completed").unwrap();
    let receipt = eternallight_engine_core::PersistentBudget::receipt(&engine_path, id).unwrap();
    SqliteStoryRepository::open(path)
        .unwrap()
        .charge_work_provider_usage(
            &ProjectId::parse(base.trim_start_matches("/api/projects/")).unwrap(),
            &grant.work_task_id,
            id,
            &receipt,
        )
        .unwrap();
}

async fn prepared(path: &Path) -> (String, Value, Value, Value) {
    let (base, source, child, permit) =
        ready_continuation(path, /*rounds*/ 2, /*attempts*/ 4).await;
    charge_one_request(path, &base, &child);
    let (status, queued) = call(
        path,
        "POST",
        &format!(
            "{base}/work-tasks/{}/execution-scope/continue",
            permit["scope"]["workTaskId"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{queued}");
    let step = queued["steps"].as_array().unwrap().last().unwrap();
    let (status, task) = call(
        path,
        "POST",
        &format!("{base}/tasks"),
        followup_claim(&permit, step),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    (base, source, task, permit)
}

async fn append(path: &Path, base: &str, task: &Value, revision: u8) -> Value {
    let (status, intent) = call(
        path,
        "POST",
        &format!("{base}/tasks/{}/intents", task["id"].as_str().unwrap()),
        json!({"expectedIntentRevision":revision,"authorText":ADJUSTMENT}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{intent}");
    intent
}

async fn deliver(path: &Path, base: &str, task: &Value, revision: u8) {
    let (status, intent) = call(
        path,
        "POST",
        &format!(
            "{base}/tasks/{}/intents/{revision}/delivered",
            task["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{intent}");
}

fn context_uri(base: &str, task: &Value) -> String {
    format!(
        "{base}/context?taskId={}&threadId=scope-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()
    )
}

#[tokio::test]
async fn delivered_adjustment_resumes_analysis_but_requires_a_new_author_write_scope() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("adjustment.sqlite3");
    let (base, source, task, old) = prepared(&path).await;
    let context = context_uri(&base, &task);
    let budget_uri = format!(
        "{base}/tasks/{}/provider-budget",
        task["id"].as_str().unwrap()
    );
    assert_eq!(
        call(&path, "GET", &context, Value::Null, CANDIDATE).await.0,
        StatusCode::OK
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let sibling: String = db.query_row("SELECT result_task_id FROM story_task_action_requests_v1 WHERE source_task_id=?1 AND status='completed'", [source["id"].as_str().unwrap()], |row| row.get(0)).unwrap();
    let mut wrong_step = task["inputManifest"].clone();
    wrong_step["authorIntentBinding"]["sourceTaskId"] = json!(sibling);
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        rusqlite::params![wrong_step.to_string(), task["id"].as_str().unwrap()],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &context, Value::Null, CANDIDATE)
            .await
            .0
            .is_success()
    );
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        rusqlite::params![
            task["inputManifest"].to_string(),
            task["id"].as_str().unwrap()
        ],
    )
    .unwrap();
    let old_action = super::propose(&path, &base, &task, "old-covered", "generateBrief").await;
    assert_eq!(old_action["status"], "queued");
    let budget = call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await;
    assert_eq!(budget.0, StatusCode::OK);
    assert_eq!(
        json!({"requests":budget.1["maxRequests"],"tokens":budget.1["maxTokens"],"deadline":budget.1["deadlineAt"]}),
        json!({"requests":15,"tokens":199900,"deadline":old["scope"]["deadlineAt"]})
    );
    let intent = append(&path, &base, &task, 0).await;
    assert!(
        !call(&path, "GET", &context, Value::Null, CANDIDATE)
            .await
            .0
            .is_success()
    );
    deliver(&path, &base, &task, 1).await;
    let (status, evidence) = call(&path, "GET", &context, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{evidence}");
    assert_eq!(
        call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await,
        budget
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT status,superseded_by_author_message_id FROM story_execution_scopes_v1 WHERE permit_id=?1", [old["id"].as_str().unwrap()], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).unwrap(), ("superseded".into(), intent["authorMessageId"].as_str().unwrap().into()));
    let task_id = task["id"].as_str().unwrap();
    let (status, action) = call(&path, "POST", &format!("{base}/tasks/{task_id}/actions"), json!({"taskId":task_id,"threadId":"scope-thread","expectedRevision":0,"expectedIntentRevision":1,"idempotencyKey":"adjusted-outline","operation":"generateOutline","target":{},"instruction":"依据作者新要求拟定候选大纲"}), CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(action["status"], "proposed");
    let scope_uri = format!("{base}/tasks/{task_id}/execution-scope");
    let request = json!({"actionIds":[action["id"]],"maxActions":1,"maxTaskAttempts":8,"maxAdvisoryRounds":0,"expectedIntentRevision":1});
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
        preview["authorIntent"],
        json!(format!(
            "{}\n\n作者追加要求：\n{ADJUSTMENT}",
            source["inputManifest"]["authorIntent"].as_str().unwrap()
        ))
    );
    assert_eq!(
        json!({"work":preview["workTaskId"],"deadline":preview["deadlineAt"]}),
        json!({"work":old["scope"]["workTaskId"],"deadline":old["scope"]["deadlineAt"]})
    );
    let confirmation = json!({"request":request,"previewDigest":preview["previewDigest"]});
    assert_eq!(
        call(&path, "POST", &scope_uri, confirmation.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, permit) = call(&path, "POST", &scope_uri, confirmation, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{permit}");
    assert_eq!(permit["status"], "active");
    assert_ne!(permit["id"], old["id"]);
    assert_eq!(
        call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await,
        budget
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/tasks/{task_id}/complete"),
            json!({"threadId":"scope-thread","summary":"新的分析已完成"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    charge_one_request(&path, &base, &task);
    let claim = json!({"operation":"generateOutline","inputRevision":0,"threadId":"scope-thread","authorIntent":"客户端替换不生效","target":{"type":"artifact","requestedByTaskId":task_id,"requestedActionId":action["id"]},"capabilityHash":format!("{:x}",Sha256::digest(super::CAPABILITY.as_bytes())),"capabilityExpiresAt":old["scope"]["deadlineAt"]});
    let mut old_claim = claim.clone();
    old_claim["operation"] = json!("generateBrief");
    old_claim["target"]["requestedActionId"] = old_action["id"].clone();
    assert!(
        !call(&path, "POST", &format!("{base}/tasks"), old_claim, AUTHOR)
            .await
            .0
            .is_success()
    );
    let (status, child) = call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    assert_eq!(
        child["inputManifest"]["authorIntent"],
        preview["authorIntent"]
    );
    let (status, grant) = call(
        &path,
        "GET",
        &format!(
            "{base}/tasks/{}/provider-budget",
            child["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    assert_eq!(
        json!({"work":grant["workTaskId"],"requests":grant["maxRequests"],"tokens":grant["maxTokens"],"deadline":grant["deadlineAt"]}),
        json!({"work":preview["workTaskId"],"requests":14,"tokens":199800,"deadline":preview["deadlineAt"]})
    );
    let (status, output) = call(&path, "POST", &format!("{base}/artifacts/outline"), json!({"taskId":child["id"],"threadId":"scope-thread","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"adjusted-outline-output","payload":{"nodes":[{"id":"book","kind":"book","position":1,"title":"港城","purpose":"记忆","summary":"记录消失的城市","dependencyKeys":[],"certainty":"tentative","protection":"free"}]}}), CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{output}");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/tasks/{}/complete", child["id"].as_str().unwrap()),
            json!({"threadId":"scope-thread","summary":"候选大纲已保存"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM story_execution_scopes_v1 WHERE permit_id=?1",
            [old["id"].as_str().unwrap()],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "superseded"
    );
}

#[tokio::test]
async fn adjusted_retry_freezes_all_delivered_history_and_rejects_forged_invalidation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("adjusted-retry.sqlite3");
    let (base, source, task, old) = prepared(&path).await;
    let cause = append(&path, &base, &task, 0).await;
    deliver(&path, &base, &task, 1).await;
    append(&path, &base, &task, 1).await;
    deliver(&path, &base, &task, 2).await;
    let task_id = task["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "PATCH",
            &format!("{base}/tasks/{task_id}"),
            json!({"threadId":"scope-thread","status":"failed","error":"interrupted"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let work_uri = format!(
        "{base}/work-tasks/{}",
        old["scope"]["workTaskId"].as_str().unwrap()
    );
    let (_, blocked) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    let (status, resumed) = call(
        &path,
        "POST",
        &format!("{work_uri}/control"),
        json!({"expectedVersion":blocked["version"],"command":"resume"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let step = resumed["steps"].as_array().unwrap().last().unwrap();
    let claim = followup_claim(&old, step);
    assert!(
        !call(
            &path,
            "POST",
            &format!("{base}/tasks"),
            claim.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    charge_one_request(&path, &base, &task);
    let (status, retry) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        claim.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{retry}");
    assert_eq!(
        retry["inputManifest"]["authorIntentBinding"],
        json!({"sourceTaskId":task_id,"intentRevision":2,"actionId":null})
    );
    assert_eq!(
        retry["inputManifest"]["authorIntent"],
        json!(format!(
            "{}\n\n作者追加要求：\n{ADJUSTMENT}\n\n作者追加要求：\n{ADJUSTMENT}",
            source["inputManifest"]["authorIntent"].as_str().unwrap()
        ))
    );
    let context = context_uri(&base, &retry);
    assert_eq!(
        call(&path, "GET", &context, Value::Null, CANDIDATE).await.0,
        StatusCode::OK
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let (foreign_base, _, foreign_task, _) = prepared(&path).await;
    let foreign_cause = append(&path, &foreign_base, &foreign_task, 0).await;
    deliver(&path, &foreign_base, &foreign_task, 1).await;
    db.execute("UPDATE story_execution_scopes_v1 SET superseded_by_author_message_id=?1 WHERE permit_id=?2", rusqlite::params![foreign_cause["authorMessageId"].as_str().unwrap(),old["id"].as_str().unwrap()]).unwrap();
    assert!(
        !call(&path, "GET", &context, Value::Null, CANDIDATE)
            .await
            .0
            .is_success()
    );
    db.execute("UPDATE story_execution_scopes_v1 SET superseded_by_author_message_id=?1 WHERE permit_id=?2", rusqlite::params![source["inputManifest"]["authorMessageId"].as_str().unwrap(),old["id"].as_str().unwrap()]).unwrap();
    assert!(
        !call(&path, "GET", &context, Value::Null, CANDIDATE)
            .await
            .0
            .is_success()
    );
    db.execute("UPDATE story_execution_scopes_v1 SET superseded_by_author_message_id=?1 WHERE permit_id=?2", rusqlite::params![cause["authorMessageId"].as_str().unwrap(),old["id"].as_str().unwrap()]).unwrap();
    let original = retry["inputManifest"].clone();
    let mut omitted = original.clone();
    omitted["authorIntentBinding"]["intentRevision"] = json!(0);
    omitted["authorIntent"] = source["inputManifest"]["authorIntent"].clone();
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        rusqlite::params![omitted.to_string(), retry["id"].as_str().unwrap()],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &context, Value::Null, CANDIDATE)
            .await
            .0
            .is_success()
    );
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        rusqlite::params![original.to_string(), retry["id"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        call(&path, "GET", &context, Value::Null, CANDIDATE).await.0,
        StatusCode::OK
    );
    let (status, grant) = call(
        &path,
        "GET",
        &format!(
            "{base}/tasks/{}/provider-budget",
            retry["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    assert_eq!(
        json!({"work":grant["workTaskId"],"requests":grant["maxRequests"],"tokens":grant["maxTokens"],"deadline":grant["deadlineAt"]}),
        json!({"work":old["scope"]["workTaskId"],"requests":14,"tokens":199800,"deadline":old["scope"]["deadlineAt"]})
    );
    let retry_id = retry["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "PATCH",
            &format!("{base}/tasks/{retry_id}"),
            json!({"threadId":"scope-thread","status":"failed","error":"interrupted again"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    charge_one_request(&path, &base, &retry);
    let (_, blocked) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    let (status, resumed) = call(
        &path,
        "POST",
        &format!("{work_uri}/control"),
        json!({"expectedVersion":blocked["version"],"command":"resume"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    // Superseding permission does not erase the previously confirmed cumulative limit.
    assert!(
        !call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, resumed)
    );
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1", [old["scope"]["workTaskId"].as_str().unwrap()], |row| row.get::<_, i64>(0)).unwrap(), 4);
}

#[tokio::test]
async fn author_adjustment_and_scope_invalidation_rollback_together_on_late_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("adjustment-atomic.sqlite3");
    let (base, _, task, old) = prepared(&path).await;
    let queued = super::propose(&path, &base, &task, "retain-on-failure", "generateBrief").await;
    assert_eq!(queued["status"], "queued");
    let db = rusqlite::Connection::open(&path).unwrap();
    let snapshot = || {
        db.query_row("SELECT (SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_task_intents_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT status FROM story_execution_scopes_v1 WHERE permit_id=?1),(SELECT status FROM story_task_action_requests_v1 WHERE action_id=?2),(SELECT version FROM story_work_tasks_v1 WHERE id=?3)", rusqlite::params![old["id"].as_str().unwrap(),queued["id"].as_str().unwrap(),old["scope"]["workTaskId"].as_str().unwrap()], |row| Ok((row.get::<_, i64>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?,row.get::<_, String>(3)?,row.get::<_, String>(4)?,row.get::<_, i64>(5)?))).unwrap()
    };
    let before = snapshot();
    db.execute_batch("CREATE TRIGGER fail_adjustment_version BEFORE UPDATE OF version ON story_work_tasks_v1 BEGIN SELECT RAISE(ABORT,'late version failure'); END;").unwrap();
    let (status, _) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{}/intents", task["id"].as_str().unwrap()),
        json!({"expectedIntentRevision":0,"authorText":ADJUSTMENT}),
        AUTHOR,
    )
    .await;
    assert!(!status.is_success());
    assert_eq!(snapshot(), before);
    db.execute_batch("DROP TRIGGER fail_adjustment_version;")
        .unwrap();
    append(&path, &base, &task, 0).await;
    deliver(&path, &base, &task, 1).await;
    assert_eq!(
        call(
            &path,
            "GET",
            &context_uri(&base, &task),
            Value::Null,
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
}
