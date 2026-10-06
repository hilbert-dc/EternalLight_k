use super::{AUTHOR, CANDIDATE, call, finish};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

pub(super) async fn request(path: &std::path::Path) -> (String, Value) {
    let (status, created) = call(
        path,
        "POST",
        "/api/projects",
        json!({"title":"首轮额度"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let (status, workspace) = call(
        path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (
        root,
        json!({"operation":"advisory","inputRevision":0,"threadId":"budget-thread",
        "target":{"type":"project"},"authorIntent":"分析灯塔线索","capabilityHash":"a".repeat(64),"capabilityExpiresAt":now+600,
        "initialBudget":{"authorizationId":"author-confirmation-one","authorMessageId":"author-message-one","expectedBaseSnapshotId":workspace["headSnapshotId"],
            "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":3,"maxTokens":700,"deadlineAt":now+600}}),
    )
}

#[tokio::test]
async fn metered_authority_is_persisted_and_usage_can_cross_former_limits() {
    use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("metered.sqlite3");
    let (root, mut request) = request(&path).await;
    for field in ["maxRequests", "maxTokens", "deadlineAt"] {
        request["initialBudget"][field] = Value::Null;
    }
    let now = request["capabilityExpiresAt"].as_i64().unwrap() - 600;
    request["capabilityExpiresAt"] = json!(now + 3600);
    let (status, task) = call(
        &path,
        "POST",
        &format!("{root}/tasks"),
        request.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let project = ProjectId::parse(root.trim_start_matches("/api/projects/")).unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        serde_json::to_value(
            repository
                .task_provider_budget(&project, task_id, now + 1800)
                .unwrap()
        )
        .unwrap(),
        json!({"taskId":task_id,"workTaskId":format!("work-{task_id}"),"budgetId":format!("budget-{task_id}"),
            "maxRequests":null,"maxTokens":null,"deadlineAt":null})
    );
    finish(&path, project.as_str(), task_id);
    let receipt = json!({"runId":task_id,"budgetId":format!("budget-{task_id}"),"status":"completed",
        "limits":{"maxRequests":null,"maxTokens":null,"maxDurationMs":null},
        "usage":{"requests":70,"tokens":1380000,"unknownTokens":20000,"overrun":false}});
    let owner = format!("work-{task_id}");
    let metering = repository
        .charge_work_provider_usage(&project, &owner, task_id, &receipt)
        .unwrap();
    assert_eq!(
        (
            metering.used_requests,
            metering.used_tokens,
            metering.unknown_tokens,
            metering.exhausted()
        ),
        (70, 1380000, 20000, false)
    );
    assert_eq!(
        repository
            .charge_work_provider_usage(&project, &owner, task_id, &receipt)
            .unwrap(),
        metering
    );
    for field in ["maxRequests", "maxTokens", "deadlineAt"] {
        let mut malformed = request.clone();
        malformed["initialBudget"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(
            call(&path, "POST", &format!("{root}/tasks"), malformed, AUTHOR)
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[tokio::test]
async fn initial_confirmation_creates_one_budgeted_root_and_exact_retry_never_reopens_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("initial.sqlite3");
    let (root, request) = request(&path).await;
    let uri = format!("{root}/tasks");
    let (status, task) = call(&path, "POST", &uri, request.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    assert_eq!(
        task["inputManifest"]["authorMessageId"],
        "author-message-one"
    );
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), AUTHOR).await,
        (StatusCode::CREATED, task.clone())
    );
    let task_id = task["id"].as_str().unwrap();
    let work_id = format!("work-{task_id}");
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{root}/tasks/{task_id}/provider-budget"),
            Value::Null,
            AUTHOR
        )
        .await,
        (
            StatusCode::OK,
            json!({"taskId":task_id,"workTaskId":work_id,"budgetId":format!("budget-{task_id}"),
            "maxRequests":3,"maxTokens":700,"deadlineAt":request["initialBudget"]["deadlineAt"]})
        )
    );
    for (pointer, value) in [
        ("/authorIntent", json!("扩大范围")),
        ("/capabilityHash", json!("b".repeat(64))),
        ("/initialBudget/authorMessageId", json!("different-message")),
        ("/initialBudget/maxTokens", json!(1000)),
        (
            "/initialBudget/deadlineAt",
            json!(request["initialBudget"]["deadlineAt"].as_u64().unwrap() + 1),
        ),
    ] {
        let mut changed = request.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            !call(&path, "POST", &uri, changed, AUTHOR)
                .await
                .0
                .is_success()
        );
    }
    finish(&path, root.trim_start_matches("/api/projects/"), task_id);
    let (status, completed) = call(
        &path,
        "GET",
        &format!("{uri}/{task_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), AUTHOR).await,
        (StatusCode::CREATED, completed)
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let message:(String,String,String,String,String,String) = db.query_row(
        "SELECT message_id,project_id,work_task_id,task_id,thread_id,author_text FROM story_author_messages_v1",
        [], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
    ).unwrap();
    assert_eq!(
        message,
        (
            "author-message-one".into(),
            root.trim_start_matches("/api/projects/").into(),
            work_id,
            task_id.into(),
            "budget-thread".into(),
            "分析灯塔线索".into()
        )
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        1
    );
    let counts:(i64,i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1),(SELECT after_attempt_sequence FROM story_work_budget_authorizations_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
    assert_eq!(counts, (1, 1, 1, 0));
    let mut reused_message = request;
    reused_message["initialBudget"]["authorizationId"] = json!("another-confirmation");
    assert!(
        !call(&path, "POST", &uri, reused_message, AUTHOR)
            .await
            .0
            .is_success()
    );
    let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(counts, (1, 1, 1));
}

#[tokio::test]
async fn initial_confirmation_rejects_unreviewed_inputs_and_rolls_back_late_reservation_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rollback.sqlite3");
    let (root, request) = request(&path).await;
    let uri = format!("{root}/tasks");
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for (pointer, value) in [
        ("/initialBudget/expectedBaseSnapshotId", json!("stale")),
        ("/initialBudget/expectedSourceDigest", json!("stale")),
        ("/initialBudget/maxRequests", json!(0)),
        ("/initialBudget/authorMessageId", json!("")),
        (
            "/initialBudget/authorMessageId",
            json!("author-confirmation-one"),
        ),
        ("/initialBudget/deadlineAt", json!(0)),
        ("/inputRevision", json!(1)),
    ] {
        let mut changed = request.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            !call(&path, "POST", &uri, changed, AUTHOR)
                .await
                .0
                .is_success()
        );
    }
    for field in ["runId", "resumeStepId", "sourceTaskId", "requestedActionId"] {
        let mut changed = request.clone();
        changed["target"][field] = json!("parent");
        assert!(
            !call(&path, "POST", &uri, changed, AUTHOR)
                .await
                .0
                .is_success()
        );
    }
    let mut spoof = request.clone();
    spoof["initialBudget"]["usedTokens"] = json!(0);
    assert_eq!(
        call(&path, "POST", &uri, spoof, AUTHOR).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut typo = request.clone();
    let budget = typo
        .as_object_mut()
        .unwrap()
        .remove("initialBudget")
        .unwrap();
    typo["initialBduget"] = budget;
    assert_eq!(
        call(&path, "POST", &uri, typo, AUTHOR).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut no_message = request.clone();
    no_message["initialBudget"]
        .as_object_mut()
        .unwrap()
        .remove("authorMessageId");
    assert_eq!(
        call(&path, "POST", &uri, no_message, AUTHOR).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_first_grant BEFORE INSERT ON story_task_provider_budgets_v1 BEGIN SELECT RAISE(ABORT, 'first grant failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let counts:(i64,i64,i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0));
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_first_grant;")
        .unwrap();
    db.execute_batch("CREATE TRIGGER reject_author_message BEFORE INSERT ON story_author_messages_v1 BEGIN SELECT RAISE(ABORT, 'message failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(counts, (0, 0, 0));
    db.execute_batch("DROP TRIGGER reject_author_message;")
        .unwrap();
    assert_eq!(
        call(&path, "POST", &uri, request, AUTHOR).await.0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn raw_task_creation_cannot_allocate_unfunded_roots_or_forged_derivatives() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unfunded.sqlite3");
    let (root, approved) = request(&path).await;
    let uri = format!("{root}/tasks");
    let mut unapproved = approved.clone();
    unapproved.as_object_mut().unwrap().remove("initialBudget");
    let mut null_budget = unapproved.clone();
    null_budget["initialBudget"] = Value::Null;
    let mut requests = vec![unapproved.clone(), null_budget];
    for field in [
        "runId",
        "resumeStepId",
        "requestedActionId",
        "requestedByTaskId",
        "sourceTaskId",
    ] {
        let mut forged = unapproved.clone();
        forged["target"][field] = json!("does-not-own-authority");
        requests.push(forged);
    }
    for body in requests {
        assert!(!call(&path, "POST", &uri, body, AUTHOR).await.0.is_success());
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts: (i64, i64, i64, i64, i64, i64) = db
        .query_row(
            "SELECT (SELECT COUNT(*) FROM story_tasks_v2),
         (SELECT COUNT(*) FROM story_work_tasks_v1),
         (SELECT COUNT(*) FROM story_work_steps_v1),
         (SELECT COUNT(*) FROM story_work_attempts_v1),
         (SELECT COUNT(*) FROM story_work_budgets_v1),
         (SELECT COUNT(*) FROM story_task_provider_budgets_v1)",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0, 0));
    assert_eq!(
        call(&path, "POST", &uri, approved, AUTHOR).await.0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn resumed_attempts_keep_the_original_author_and_reject_broken_lineage_before_execution() {
    use novel_codex_story_core::{ProjectId, SqliteStoryRepository, TaskUpdate};

    for operation in ["advisory", "generateBrief"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resume-author.sqlite3");
        let (root, mut initial) = request(&path).await;
        initial["operation"] = json!(operation);
        initial["target"] = if operation == "advisory" {
            json!({"type":"project"})
        } else {
            json!({"type":"artifact"})
        };
        let (status, source) = call(
            &path,
            "POST",
            &format!("{root}/tasks"),
            initial.clone(),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{source}");
        let task_id = source["id"].as_str().unwrap();
        let project = ProjectId::parse(root.trim_start_matches("/api/projects/")).unwrap();
        let mut repo = SqliteStoryRepository::open(&path).unwrap();
        repo.update_task(
            &project,
            task_id,
            "budget-thread",
            TaskUpdate {
                status: "failed".into(),
                error: Some("interrupted".into()),
                result_summary: None,
                now: 1,
            },
        )
        .unwrap();
        let engine_path = path.with_extension("engine.sqlite3");
        let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path, task_id,
            json!({"options":{"id":format!("budget-{task_id}"),"maxRequests":3,"maxTokens":700,"startedAt":1000},"now":1000})).unwrap();
        engine
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":100},"now":1001}),
            )
            .unwrap();
        drop(engine);
        eternallight_engine_core::PersistentBudget::recover(&engine_path).unwrap();
        let work = repo.work_task_for_attempt(&project, task_id).unwrap();
        let (status, charged) = call(
            &path,
            "POST",
            &format!("{root}/work-tasks/{}/provider-usage", work.id),
            json!({"taskId":task_id}),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{charged}");
        let work = repo.work_task_for_attempt(&project, task_id).unwrap();
        let resumed = repo
            .control_work_task(&project, &work.id, "resume", work.version, 2)
            .unwrap();
        let mut target = resumed.steps[0].target.clone();
        target["resumeStepId"] = json!(resumed.steps[0].id);
        let (status, retry) = call(
            &path,
            "POST",
            &format!("{root}/tasks"),
            json!({
                "operation":operation,"inputRevision":0,"threadId":"retry-thread","target":target,
                "authorIntent":"客户端伪造的新作者要求","capabilityHash":"a".repeat(64),
                "capabilityExpiresAt":initial["capabilityExpiresAt"]
            }),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{retry}");
        let retry_id = retry["id"].as_str().unwrap();
        assert_eq!(retry["inputManifest"]["authorIntent"], "分析灯塔线索");
        assert_eq!(
            retry["inputManifest"]["authorIntentBinding"],
            json!({"sourceTaskId":task_id,"intentRevision":0,"actionId":null})
        );
        let budget_uri = format!("{root}/tasks/{retry_id}/provider-budget");
        let expected = (
            StatusCode::OK,
            json!({"taskId":retry_id,"workTaskId":work.id,
            "budgetId":format!("budget-{retry_id}"),"maxRequests":2,"maxTokens":600,
            "deadlineAt":initial["initialBudget"]["deadlineAt"]}),
        );
        assert_eq!(
            call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await,
            expected
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        let original = retry["inputManifest"].to_string();
        for broken in [
            json!({"sourceTaskId":retry_id,"intentRevision":0,"actionId":null}),
            json!({"sourceTaskId":"different-work","intentRevision":0,"actionId":null}),
            Value::Null,
        ] {
            let mut manifest = retry["inputManifest"].clone();
            manifest["authorIntentBinding"] = broken;
            db.execute(
                "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
                rusqlite::params![manifest.to_string(), retry_id],
            )
            .unwrap();
            assert!(
                !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
                    .await
                    .0
                    .is_success()
            );
        }
        db.execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
            rusqlite::params![original, retry_id],
        )
        .unwrap();
        assert_eq!(
            call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await,
            expected
        );
        db.execute(
            "UPDATE story_author_messages_v1 SET author_text='模型伪造的作者要求'",
            [],
        )
        .unwrap();
        assert!(
            !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(counts, (1, 1, 1));
    }
}

#[tokio::test]
async fn runtime_without_engine_receipt_storage_cannot_create_an_execution_attempt() {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use novel_codex_story_runtime::{RuntimeState, router};
    use tower::ServiceExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("no-engine.sqlite3");
    let (root, approved) = request(&path).await;
    let mut unapproved = approved.clone();
    unapproved.as_object_mut().unwrap().remove("initialBudget");
    let app = router(RuntimeState::new(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    for payload in [approved, unapproved] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("{root}/tasks"))
                    .header("authorization", format!("Bearer {AUTHOR}"))
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["error"]["code"], "engine_budget_unavailable");
    }
    assert_eq!(
        rusqlite::Connection::open(path)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |row| row
                .get::<_, i64>(0),)
            .unwrap(),
        0
    );
}
