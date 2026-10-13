use super::{AUTHOR, call};
use super::{call_app, live_app};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn review_seats_share_one_work_budget_and_cannot_run_ahead_of_unsettled_usage() {
    for known in [false, true] {
        review_seats_under_actual_usage(known).await;
    }
}

async fn review_seats_under_actual_usage(known: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("review.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"审阅"}),
        AUTHOR,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let (status, outline) = call(&path,"POST",&format!("{root}/artifacts/outline"),json!({
        "payload":{"nodes":[{"id":"book","kind":"book","position":1,"title":"港城","purpose":"记忆","summary":"记录消失的城市",
        "dependencyKeys":[],"certainty":"tentative","protection":"free"}]},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"
    }),AUTHOR).await;
    assert!(status.is_success(), "{outline}");
    let (status, run) = call(
        &path,
        "POST",
        &format!("{root}/planning-reviews"),
        json!({
            "artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,
            "authorIntent":"检查结构与人物","roles":["structure","character"]
        }),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let run_id = run["id"].as_str().unwrap();
    let run_uri = format!("{root}/planning-reviews/{run_id}");
    assert_eq!(
        call(&path, "GET", &run_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, run.clone())
    );
    assert_eq!(
        call(&path, "GET", &run_uri, Value::Null, super::CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let work_id = format!("work-{run_id}");
    let work_uri = format!("{root}/work-tasks/{work_id}");
    let (_, work) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(work["steps"].as_array().unwrap().len(), 2);
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let unauthorized = json!({"threadId":"review-thread","model":"fixture","capabilityHash":"a".repeat(64),"capabilityExpiresAt":now+600});
    assert!(
        !call(
            &path,
            "POST",
            &format!("{root}/planning-reviews/{run_id}/seats/structure/tasks"),
            unauthorized.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    for resume in [false, true] {
        let mut forged = unauthorized.clone();
        forged["operation"] = json!("reviewPlanningSeat");
        forged["inputRevision"] = json!(0);
        forged["authorIntent"] = json!("检查规划");
        forged["target"] = json!({"type":"artifact","artifactKind":"outline","revisionId":outline["revisionId"],"reviewRunId":run_id,"role":"structure"});
        if resume {
            forged["target"]["resumeStepId"] = work["steps"][0]["id"].clone();
        }
        assert!(
            !call(&path, "POST", &format!("{root}/tasks"), forged, AUTHOR)
                .await
                .0
                .is_success()
        );
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_attempts_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(counts, (0, 0, 0));
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1,
        work
    );
    assert_eq!(call(&path,"POST",&format!("{work_uri}/budget-authorization"),json!({
        "expectedVersion":work["version"],"expectedProjectRevision":workspace["revision"],
        "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":3,"maxTokens":1000,"deadlineAt":now+600
    }),AUTHOR).await.0,StatusCode::OK);
    let request = json!({"threadId":"review-thread","model":"fixture","capabilityHash":"a".repeat(64),"capabilityExpiresAt":now+600});
    let seat_uri = format!("{root}/planning-reviews/{run_id}/seats");
    let (status, task) = call(
        &path,
        "POST",
        &format!("{seat_uri}/structure/tasks"),
        request.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (_, owner) = call(
        &path,
        "GET",
        &format!("{root}/tasks/{task_id}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(owner["id"], work_id);
    let (_, grant) = call(
        &path,
        "GET",
        &format!("{root}/tasks/{task_id}/provider-budget"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(grant["maxRequests"], 3);
    assert!(
        !call(
            &path,
            "POST",
            &format!("{seat_uri}/character/tasks"),
            request.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert!(
        !call(
            &path,
            "POST",
            &format!("{root}/planning-reviews/{run_id}/finalize"),
            json!({}),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, report) = call(&path,"POST",&format!("{root}/planning-reviews/{run_id}/reports"),json!({
        "taskId":task_id,"threadId":"review-thread","expectedRevision":0,
        "report":{"summary":"已检查","strengths":[],"findings":[],"coverage":[{"checkId":"structure:book","status":"checked"}]}
    }),AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{root}/tasks/{task_id}/complete"),
            json!({"threadId":"review-thread","summary":"完成"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(
        !call(
            &path,
            "POST",
            &format!("{seat_uri}/character/tasks"),
            request.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let before_finalize = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1;
    let run_before_finalize = call(&path, "GET", &run_uri, Value::Null, AUTHOR).await.1;
    assert!(
        !call(
            &path,
            "POST",
            &format!("{run_uri}/finalize"),
            json!({}),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(&path, "GET", &run_uri, Value::Null, AUTHOR).await.1,
        run_before_finalize
    );
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1,
        before_finalize
    );
    let engine_path = path.with_extension("engine.sqlite3");
    let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,task_id,
        json!({"options":{"id":format!("budget-{task_id}"),"maxRequests":3,"maxTokens":1000,"startedAt":1000},"now":1000})).unwrap();
    let reservation = engine
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":100},"now":1001}),
        )
        .unwrap();
    if known {
        engine
            .apply(
                "budget.settle",
                json!({"id":reservation,"usage":{"total_tokens":100},"now":1002}),
            )
            .unwrap();
        engine.complete("completed").unwrap();
    } else {
        drop(engine);
        eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
    }
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{work_uri}/provider-usage"),
            json!({"taskId":task_id}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let before_claim = super::unknown_usage_admission::snapshot(&path);
    let (status, second) = call(
        &path,
        "POST",
        &format!("{seat_uri}/character/tasks"),
        request.clone(),
        AUTHOR,
    )
    .await;
    if !known {
        assert_eq!(status, StatusCode::CONFLICT, "{second}");
        assert_eq!(second["error"]["code"], "work_usage_unknown");
        assert_eq!(
            super::unknown_usage_admission::snapshot(&path),
            before_claim
        );
        return;
    }
    assert_eq!(status, StatusCode::OK, "{second}");
    let second_id = second["id"].as_str().unwrap();
    let (_, grant) = call(
        &path,
        "GET",
        &format!("{root}/tasks/{second_id}/provider-budget"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        (
            grant["workTaskId"].clone(),
            grant["maxRequests"].clone(),
            grant["maxTokens"].clone()
        ),
        (json!(work_id), json!(2), json!(900))
    );
    // Reopen through HTTP after a failed, no-output seat. The same role/step
    // can be retried only after its actual persistent receipt is settled.
    assert_eq!(
        call(
            &path,
            "PATCH",
            &format!("{root}/tasks/{second_id}"),
            json!({
                "threadId":"review-thread","status":"failed","error":"interrupted"
            }),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,second_id,
        json!({"options":{"id":format!("budget-{second_id}"),"maxRequests":2,"maxTokens":900,"startedAt":1000},"now":1000})).unwrap();
    let reservation = engine
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":100},"now":1001}),
        )
        .unwrap();
    engine
        .apply(
            "budget.settle",
            json!({"id":reservation,"usage":{"total_tokens":100},"now":1002}),
        )
        .unwrap();
    engine.complete("failed").unwrap();
    let (_, blocked) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(blocked["status"], "blocked");
    // Work control alone never releases the old reservation.
    let before_resume = super::unknown_usage_admission::snapshot(&path);
    let (status, rejected) = call(
        &path,
        "POST",
        &format!("{work_uri}/control"),
        json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":blocked["threadId"],
            "command":"resume","expectedControlRevision":blocked["controlRevision"]
        }),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["code"], "work_control_rejected");
    assert_eq!(rejected["outcome"]["receipt"]["reason"], "TaskInputStale");
    assert_eq!(
        rejected["error"]["message"],
        novel_codex_story_core::RepositoryError::TaskInputStale.to_string()
    );
    super::unknown_usage_admission::assert_rejected_control_only(&path, &before_resume, &rejected);
    let (_, before_retry) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert!(
        !call(
            &path,
            "POST",
            &format!("{seat_uri}/character/tasks"),
            request.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1,
        before_retry
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{work_uri}/provider-usage"),
            json!({"taskId":second_id}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, before_retry) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    let (status, resumed) = call(
        &path,
        "POST",
        &format!("{work_uri}/control"),
        json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":before_retry["threadId"],"command":"resume","expectedControlRevision":before_retry["controlRevision"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let before_retry = resumed;
    let seats_before = call(&path, "GET", &run_uri, Value::Null, AUTHOR).await.1["seats"].clone();
    let app = live_app(&path);
    db.execute_batch("CREATE TRIGGER reject_review_binding BEFORE UPDATE OF task_id ON story_planning_review_seats_v2 BEGIN SELECT RAISE(ABORT, 'seat failure'); END;").unwrap();
    assert!(
        !call_app(
            &app,
            "POST",
            &format!("{seat_uri}/character/tasks"),
            request.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call_app(&app, "GET", &run_uri, Value::Null, AUTHOR).await.1["seats"],
        seats_before
    );
    assert_eq!(
        call_app(&app, "GET", &work_uri, Value::Null, AUTHOR)
            .await
            .1,
        before_retry
    );
    let counts:(i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_work_attempts_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(counts, (2, 2));
    db.execute_batch("DROP TRIGGER reject_review_binding;")
        .unwrap();
    let (status, retried) = call(
        &path,
        "POST",
        &format!("{seat_uri}/character/tasks"),
        request.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{retried}");
    let retried_id = retried["id"].as_str().unwrap();
    assert_ne!(retried_id, second_id);
    assert_eq!(
        retried["inputManifest"]["planningReviewAuthorIntentBinding"],
        second["inputManifest"]["planningReviewAuthorIntentBinding"]
    );
    assert_eq!(
        (
            retried["inputManifest"]["authorIntent"].clone(),
            retried["inputManifest"]["authorMessageId"].clone()
        ),
        (run["authorIntent"].clone(), run["authorMessageId"].clone())
    );
    let repo = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
    let project_id = novel_codex_story_core::ProjectId::parse(project).unwrap();
    let previous = repo.task(&project_id, second_id).unwrap().unwrap();
    repo.assert_task_inputs_current(&previous).unwrap();
    assert_ne!(
        task["inputManifest"]["planningReviewAuthorIntentBinding"]["role"],
        retried["inputManifest"]["planningReviewAuthorIntentBinding"]["role"]
    );
    let (_, grant) = call(
        &path,
        "GET",
        &format!("{root}/tasks/{retried_id}/provider-budget"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        (
            grant["workTaskId"].clone(),
            grant["maxRequests"].clone(),
            grant["maxTokens"].clone()
        ),
        (json!(work_id), json!(1), json!(800))
    );
    assert!(
        !call(
            &path,
            "POST",
            &format!("{seat_uri}/structure/tasks"),
            request,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(call(&path,"POST",&format!("{run_uri}/reports"),json!({
        "taskId":retried_id,"threadId":"review-thread","expectedRevision":0,
        "report":{"summary":"人物检查完成","strengths":[],"findings":[],"coverage":[{"checkId":"character:book","status":"checked"}]}
    }),AUTHOR).await.0,StatusCode::OK);
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{root}/tasks/{retried_id}/complete"),
            json!({
                "threadId":"review-thread","summary":"完成"
            }),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, reports_ready) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(
        (
            reports_ready["status"].clone(),
            reports_ready["resumeBlockReason"].clone()
        ),
        (json!("blocked"), json!("pendingUsage"))
    );
    let before_finalize = call(&path, "GET", &run_uri, Value::Null, AUTHOR).await.1;
    assert!(
        !call(
            &path,
            "POST",
            &format!("{run_uri}/finalize"),
            json!({}),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(&path, "GET", &run_uri, Value::Null, AUTHOR).await.1,
        before_finalize
    );
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1,
        reports_ready
    );
    let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,retried_id,
        json!({"options":{"id":format!("budget-{retried_id}"),"maxRequests":1,"maxTokens":800,"startedAt":1000},"now":1000})).unwrap();
    engine.complete("completed").unwrap();
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{work_uri}/provider-usage"),
            json!({"taskId":retried_id}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, ready) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{work_uri}/control"),
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":ready["threadId"],"command":"pause","expectedControlRevision":ready["controlRevision"]}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, paused) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert!(
        !call(
            &path,
            "POST",
            &format!("{run_uri}/finalize"),
            json!({}),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1,
        paused
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{work_uri}/control"),
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":paused["threadId"],"command":"resume","expectedControlRevision":paused["controlRevision"]}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, finished) = call(
        &path,
        "POST",
        &format!("{run_uri}/finalize"),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    assert_eq!(finished["status"], "completed");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{run_uri}/finalize"),
            json!({}),
            AUTHOR
        )
        .await,
        (StatusCode::OK, finished)
    );
    let (_, work) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(work["status"], "completed");
    assert_eq!(work["steps"][1]["status"], "completed");
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts:(i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(counts, (1, 1));
    // Exact recovery does not disappear when newer checks push a run out of
    // the bounded recent list.
    let mut newest = Value::Null;
    for _ in 0..11 {
        let (status, next) = call(
            &path,
            "POST",
            &format!("{root}/planning-reviews"),
            json!({
                "artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,
                "authorIntent":"另一轮检查","roles":["structure","character"]
            }),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        newest = next;
    }
    let (_, recovered) = call(&path, "GET", &run_uri, Value::Null, AUTHOR).await;
    assert_eq!(
        (
            recovered["id"].clone(),
            recovered["status"].clone(),
            recovered["reports"].as_array().unwrap().len()
        ),
        (json!(run_id), json!("completed"), 2)
    );
    let newest_id = newest["id"].as_str().unwrap();
    let newest_work_uri = format!("{root}/work-tasks/work-{newest_id}");
    let (_, newest_work) = call(&path, "GET", &newest_work_uri, Value::Null, AUTHOR).await;
    let control_uri = format!("{newest_work_uri}/control");
    assert_eq!(
        call(
            &path,
            "POST",
            &control_uri,
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":newest_work["threadId"],"command":"cancel","expectedControlRevision":newest_work["controlRevision"]}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let newest_uri = format!("{root}/planning-reviews/{newest_id}");
    let (_, cancelled) = call(&path, "GET", &newest_uri, Value::Null, AUTHOR).await;
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{newest_uri}/finalize"),
            json!({}),
            AUTHOR
        )
        .await,
        (StatusCode::OK, cancelled)
    );
    let (_, other) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"其他作品"}),
        AUTHOR,
    )
    .await;
    assert!(
        !call(
            &path,
            "GET",
            &format!(
                "/api/projects/{}/planning-reviews/{run_id}",
                other["project"]["id"].as_str().unwrap()
            ),
            Value::Null,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
}

use novel_codex_story_core::{TaskInputRepository, TasksRepository};
