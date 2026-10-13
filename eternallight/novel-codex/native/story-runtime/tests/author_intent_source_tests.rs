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

const AUTHOR: &str = "intent-source-author-token-1234567890";
const CANDIDATE: &str = "intent-source-candidate-token-1234567";
const CAPABILITY: &str = "intent-source-capability-1234567890";

#[path = "author_intent_source/final_reply_tests.rs"]
mod final_reply_tests;

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    body: Value,
    actor: &str,
) -> (StatusCode, Value) {
    let app = router(
        task_fixture::state(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    call_app(app, method, uri, body, actor).await
}

async fn call_app(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
    actor: &str,
) -> (StatusCode, Value) {
    let response = app
        .author_fixture_request(
            Request::builder()
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

async fn setup(path: &std::path::Path) -> (String, Value) {
    let (status, created) = call(
        path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"追加消息来源"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let (status, workspace) = call(
        path,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,task)=call(path,"POST",&format!("{base}/tasks"),json!({"operation":"advisory","inputRevision":0,"threadId":"intent-source-thread","target":{"type":"project"},"authorIntent":"讨论白塔，不改正文",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600,
        "initialBudget":{"authorizationId":"intent-source-budget","authorMessageId":"intent-source-root","expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"maxRequests":6,"maxTokens":1000,"deadlineAt":now+600}}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    (base, task)
}

fn append() -> Value {
    json!({"expectedIntentRevision":0,"authorText":"只调整人物动机，保留结局🌙"})
}

#[tokio::test]
async fn accumulated_author_history_is_complete_and_paged_beyond_single_message_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("long-history.sqlite3");
    let (base, task) = setup(&path).await;
    let id = task["id"].as_str().unwrap();
    let uri = format!("{base}/tasks/{id}/intents");
    let mut expected = task["inputManifest"]["authorIntent"]
        .as_str()
        .unwrap()
        .to_owned();
    for revision in 0..6 {
        let text = format!("要求{revision}：{}", "保留".repeat(2400));
        let (status, _) = call(
            &path,
            "POST",
            &uri,
            json!({"expectedIntentRevision":revision,"authorText":text}),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = call(
            &path,
            "POST",
            &format!("{uri}/{}/delivered", revision + 1),
            json!({}),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        expected.push_str("\n\n作者追加要求：\n");
        expected.push_str(&text);
    }
    assert!(expected.len() > 80_000);
    let context_uri = format!(
        "{base}/context?taskId={id}&threadId={}&projectRevision={}&purpose={}&byteBudget=24000",
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        task["purpose"].as_str().unwrap()
    );
    let (status, context) = call(&path, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert!(context.to_string().len() <= 24_000);
    assert!(context["activeIntentRefs"][0].get("text").is_none());
    assert_eq!(
        context["authorRequirementsSource"]["rootReadComplete"],
        false
    );
    let source = context["authorRequirementsSource"]["sourceId"]
        .as_str()
        .unwrap();
    let mut cursor = None::<String>;
    let mut raw = String::new();
    let mut pages = 0;
    loop {
        let suffix = cursor
            .as_ref()
            .map(|c| format!("&cursor={c}"))
            .unwrap_or_default();
        let (status, page) = call(&path, "GET", &format!("{base}/tasks/{id}/task-source?threadId={}&expectedRevision={}&sourceId={source}{suffix}", task["threadId"].as_str().unwrap(), task["inputRevision"]), Value::Null, CANDIDATE).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        raw.push_str(page["payloadPart"].as_str().unwrap());
        pages += 1;
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
        let (_, partial) = call(&path, "GET", &context_uri, Value::Null, CANDIDATE).await;
        assert_eq!(
            partial["authorRequirementsSource"]["rootReadComplete"],
            false
        );
    }
    assert!(pages > 1);
    let full: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(full["authorText"], expected);
    let (_, complete) = call(&path, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(
        complete["authorRequirementsSource"]["rootReadComplete"],
        true
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_append_and_reads_return_a_consistent_author_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("concurrent.sqlite3");
    let (base, task) = setup(&path).await;
    let task_id = task["id"].as_str().unwrap();
    let uri = format!("{base}/tasks/{task_id}/intents");
    let write_path = path.clone();
    let write_uri = uri.clone();
    let writer =
        tokio::spawn(async move { call(&write_path, "POST", &write_uri, append(), AUTHOR).await });
    let read_path = path.clone();
    let read_uri = uri.clone();
    let reader = tokio::spawn(async move {
        let mut results = Vec::new();
        for _ in 0..8 {
            results.push(call(&read_path, "GET", &read_uri, Value::Null, AUTHOR).await);
        }
        results
    });
    let (status, pending) = writer.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{pending}");
    for (status, result) in reader.await.unwrap() {
        assert_eq!(status, StatusCode::OK, "{result}");
        assert!(
            result == json!({"intents":[]}) || result == json!({"intents":[pending.clone()]}),
            "{result}"
        );
    }
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"intents":[pending]}))
    );
}

#[tokio::test]
async fn author_append_has_independent_durable_identity_and_delivery_replay_keeps_the_same_source()
{
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("identity.sqlite3");
    let (base, task) = setup(&path).await;
    let task_id = task["id"].as_str().unwrap();
    let uri = format!("{base}/tasks/{task_id}/intents");
    assert_eq!(
        call(&path, "POST", &uri, append(), CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    let mut injected = append();
    injected["authorMessageId"] = json!("forged-source");
    assert!(
        !call(&path, "POST", &uri, injected, AUTHOR)
            .await
            .0
            .is_success()
    );
    let (status, pending) = call(&path, "POST", &uri, append(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    assert_eq!(
        call(&path, "POST", &uri, append(), AUTHOR).await,
        (StatusCode::OK, pending.clone())
    );
    let mut different = append();
    different["authorText"] = json!("重复身份不得改写要求");
    assert!(
        !call(&path, "POST", &uri, different, AUTHOR)
            .await
            .0
            .is_success()
    );
    let message = pending["authorMessageId"].as_str().unwrap();
    assert!(message.starts_with("author-message-"));
    assert_ne!(message, task["inputManifest"]["authorMessageId"]);
    let db = rusqlite::Connection::open(&path).unwrap();
    let stored: (String,String,String,String,i64,String,i64)=db.query_row("SELECT message_id,project_id,work_task_id,thread_id,intent_revision,author_text,created_at FROM story_author_messages_v1 WHERE task_id=?1 AND intent_revision=1",[task_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).unwrap();
    assert_eq!(
        stored,
        (
            message.into(),
            base.trim_start_matches("/api/projects/").into(),
            format!("work-{task_id}"),
            "intent-source-thread".into(),
            1,
            append()["authorText"].as_str().unwrap().into(),
            pending["createdAt"].as_i64().unwrap()
        )
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"intents":[pending.clone()]}))
    );
    let delivered_uri = format!("{uri}/1/delivered");
    assert_eq!(
        call(&path, "POST", &delivered_uri, json!({}), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, delivered) = call(&path, "POST", &delivered_uri, json!({}), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{delivered}");
    assert_eq!(delivered["authorMessageId"], pending["authorMessageId"]);
    assert_eq!(delivered["authorText"], pending["authorText"]);
    assert_eq!(
        call(&path, "POST", &delivered_uri, json!({}), AUTHOR).await,
        (StatusCode::OK, delivered.clone())
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"intents":[delivered]}))
    );
    let action_uri = format!("{base}/tasks/{task_id}/actions");
    let (status,action)=call(&path,"POST",&action_uri,json!({"taskId":task_id,"threadId":"intent-source-thread","expectedRevision":0,"expectedIntentRevision":1,"idempotencyKey":"delivered-action","operation":"generateBrief","target":{"type":"artifact"},"instruction":"拟定候选简报"}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let action_id = action["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{action_uri}/{action_id}/approve"),
            json!({"expectedIntentRevision":1}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, permission) = call(
        &path,
        "GET",
        &format!("{action_uri}/{action_id}/permission"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{permission}");
    assert_eq!(
        permission["permission"]["evidence"]["authorMessageId"],
        task["inputManifest"]["authorMessageId"]
    );
    assert_eq!(
        permission["permission"]["evidence"]["authorIntent"],
        format!(
            "讨论白塔，不改正文\n\n作者追加要求：\n{}",
            append()["authorText"].as_str().unwrap()
        )
    );
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_task_intents_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap(),(2,1,1,1));
    db.execute_batch("UPDATE story_author_messages_v1 SET author_text='被替换的追加要求' WHERE intent_revision=1").unwrap();
    assert!(
        !call(
            &path,
            "GET",
            &format!("{action_uri}/{action_id}/permission"),
            Value::Null,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,claimed)=call(&path,"POST",&format!("{base}/tasks"),json!({"operation":"generateBrief","inputRevision":0,"threadId":"derived-thread","target":{"type":"artifact","requestedByTaskId":task_id,"requestedActionId":action_id},"authorIntent":"客户端不可替换原话","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}),AUTHOR).await;
    assert!(!status.is_success(), "{claimed}");
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_task_provider_budgets_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?))).unwrap(),(1,1,2));
    db.execute(
        "UPDATE story_author_messages_v1 SET author_text=?1 WHERE intent_revision=1",
        [append()["authorText"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{action_uri}/{action_id}/permission"),
            Value::Null,
            AUTHOR
        )
        .await,
        (StatusCode::OK, permission)
    );
}

#[tokio::test]
async fn late_append_failure_rolls_back_message_intent_action_and_work_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rollback.sqlite3");
    let (base, task) = setup(&path).await;
    let task_id = task["id"].as_str().unwrap();
    let task_uri = format!("{base}/tasks/{task_id}");
    let (status,action)=call(&path,"POST",&format!("{task_uri}/actions"),json!({"taskId":task_id,"threadId":"intent-source-thread","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"initial-action","operation":"generateBrief","target":{"type":"artifact"},"instruction":"拟定候选简报"}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let work_uri = format!("{base}/work-tasks/work-{task_id}");
    let before = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    let db = rusqlite::Connection::open(&path).unwrap();
    // Inject faults after startup so this exercises transaction rollback, not
    // the separate startup schema-mismatch rejection boundary.
    let app = router(
        task_fixture::state(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    for trigger in [
        "CREATE TRIGGER reject_append BEFORE INSERT ON story_author_messages_v1 WHEN NEW.intent_revision>0 BEGIN SELECT RAISE(ABORT,'message failure'); END;",
        "CREATE TRIGGER reject_append BEFORE INSERT ON story_task_intents_v1 BEGIN SELECT RAISE(ABORT,'intent failure'); END;",
        "CREATE TRIGGER reject_append BEFORE UPDATE ON story_work_tasks_v1 BEGIN SELECT RAISE(ABORT,'late work failure'); END;",
    ] {
        db.execute_batch(trigger).unwrap();
        assert!(
            !call_app(
                app.clone(),
                "POST",
                &format!("{task_uri}/intents"),
                append(),
                AUTHOR
            )
            .await
            .0
            .is_success()
        );
        assert_eq!(
            call_app(app.clone(), "GET", &work_uri, Value::Null, AUTHOR).await,
            before
        );
        assert_eq!(
            call_app(
                app.clone(),
                "GET",
                &format!("{task_uri}/actions"),
                Value::Null,
                AUTHOR
            )
            .await
            .1["actions"],
            json!([action.clone()])
        );
        assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_task_intents_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?))).unwrap(),(1,0,1));
        db.execute_batch("DROP TRIGGER reject_append;").unwrap();
    }
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_uri}/intents"),
            append(),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{task_uri}/actions"),
            Value::Null,
            AUTHOR
        )
        .await
        .1["actions"][0]["status"],
        "superseded"
    );
}

#[tokio::test]
async fn damaged_append_cannot_be_read_delivered_or_used_to_propose_actions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("damaged.sqlite3");
    let (base, task) = setup(&path).await;
    let task_id = task["id"].as_str().unwrap();
    let uri = format!("{base}/tasks/{task_id}/intents");
    let (_, pending) = call(&path, "POST", &uri, append(), AUTHOR).await;
    let message = pending["authorMessageId"].as_str().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let delivery_uri = format!("{uri}/1/delivered");
    for sql in [
        "UPDATE story_author_messages_v1 SET author_text='伪造作者要求' WHERE intent_revision=1",
        "UPDATE story_author_messages_v1 SET thread_id='other-thread' WHERE intent_revision=1",
        "UPDATE story_task_intents_v1 SET author_message_id='intent-source-root' WHERE intent_revision=1",
    ] {
        db.execute_batch(sql).unwrap();
        assert!(
            !call(&path, "GET", &uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert!(
            !call(&path, "POST", &delivery_uri, json!({}), AUTHOR)
                .await
                .0
                .is_success()
        );
        db.execute("UPDATE story_author_messages_v1 SET author_text=?1,thread_id='intent-source-thread' WHERE intent_revision=1",[append()["authorText"].as_str().unwrap()]).unwrap();
        db.execute(
            "UPDATE story_task_intents_v1 SET author_message_id=?1 WHERE task_id=?2",
            rusqlite::params![message, task_id],
        )
        .unwrap();
    }
    assert_eq!(
        call(&path, "POST", &delivery_uri, json!({}), AUTHOR)
            .await
            .0,
        StatusCode::OK
    );
    db.execute_batch(
        "UPDATE story_author_messages_v1 SET author_text='伪造作者要求' WHERE intent_revision=1",
    )
    .unwrap();
    assert!(
        !call(
            &path,
            "POST",
            &format!("{base}/tasks/{task_id}/complete"),
            json!({"threadId":"intent-source-thread","summary":"不应完成"}),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    assert!(!call(&path,"POST",&format!("{base}/tasks/{task_id}/actions"),json!({"taskId":task_id,"threadId":"intent-source-thread","expectedRevision":0,"expectedIntentRevision":1,"idempotencyKey":"damaged-action","operation":"generateBrief","target":{"type":"artifact"},"instruction":"不能提交"}),CANDIDATE).await.0.is_success());
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_task_action_requests_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).unwrap(),(0,1));
}

#[tokio::test]
async fn paused_and_cancelled_work_allow_receipt_replay_but_not_new_append_or_delivery() {
    for command in ["pause", "cancel"] {
        for pending in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("controlled.sqlite3");
            let (base, task) = setup(&path).await;
            let task_id = task["id"].as_str().unwrap();
            let uri = format!("{base}/tasks/{task_id}/intents");
            let recorded = if pending {
                Some(call(&path, "POST", &uri, append(), AUTHOR).await.1)
            } else {
                None
            };
            let work_uri = format!("{base}/work-tasks/work-{task_id}");
            let (status, work) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
            assert_eq!(status, StatusCode::OK, "{work}");
            let (status, controlled) = call(
                &path,
                "POST",
                &format!("{work_uri}/control"),
                json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":work["threadId"],"command":command,"expectedControlRevision":work["controlRevision"]}),
                AUTHOR,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{controlled}");
            let replay = call(&path, "POST", &uri, append(), AUTHOR).await;
            if let Some(original) = &recorded {
                assert_eq!(replay, (StatusCode::OK, original.clone()));
                assert!(
                    !call(
                        &path,
                        "POST",
                        &uri,
                        json!({"expectedIntentRevision":1,"authorText":"停止后的新要求"}),
                        AUTHOR
                    )
                    .await
                    .0
                    .is_success()
                );
            } else {
                assert!(!replay.0.is_success());
            }
            assert_eq!(
                call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1,
                controlled["work"]
            );
            if let Some(recorded) = recorded {
                assert!(
                    !call(
                        &path,
                        "POST",
                        &format!("{uri}/1/delivered"),
                        json!({}),
                        AUTHOR
                    )
                    .await
                    .0
                    .is_success()
                );
                assert_eq!(
                    call(&path, "GET", &uri, Value::Null, AUTHOR).await,
                    (StatusCode::OK, json!({"intents":[recorded]}))
                );
            }
            let db = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).unwrap(),(if pending { 2 } else { 1 },1));
        }
    }
}

#[tokio::test]
async fn author_answers_are_readable_after_restart_without_claiming_core_delivery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("answer.sqlite3");
    let (base, task) = setup(&path).await;
    let id = task["id"].as_str().unwrap();
    let answers = format!("{base}/tasks/{id}/answers");
    let body =
        json!({"expectedIntentRevision":0,"authorText":"作者回答：开放结局，保留人物原先的选择。"});
    let (status, _) = call(&path, "POST", &answers, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, saved) = call(&path, "POST", &answers, body.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["status"], "recorded");
    assert!(saved["deliveredAt"].is_null());
    let (status, replay) = call(&path, "POST", &answers, body.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["authorMessageId"], saved["authorMessageId"]);
    let (_, history) = call(
        &path,
        "GET",
        &format!("{base}/tasks/{id}/intents"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(history["intents"].as_array().unwrap().len(), 1);
    let context_uri = format!(
        "{base}/context?taskId={id}&threadId={}&projectRevision={}&purpose={}&byteBudget=24000",
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        task["purpose"].as_str().unwrap()
    );
    let (status, context) = call(&path, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["authorIntentState"]["expectedIntentRevision"], 1);
    let source = context["authorRequirementsSource"]["sourceId"]
        .as_str()
        .unwrap();
    let (status, page) = call(
        &path,
        "GET",
        &format!(
            "{base}/tasks/{id}/task-source?threadId={}&expectedRevision={}&sourceId={source}",
            task["threadId"].as_str().unwrap(),
            task["inputRevision"]
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let payload: Value = serde_json::from_str(page["payloadPart"].as_str().unwrap()).unwrap();
    assert!(
        payload["authorText"]
            .as_str()
            .unwrap()
            .contains(body["authorText"].as_str().unwrap())
    );
    let (status, _) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{id}/cancel"),
        json!({}),
        AUTHOR,
    )
    .await;
    assert!(status.is_success());
    let delivered = format!("{base}/tasks/{id}/intents/1/delivered");
    let (status, receipt) = call(&path, "POST", &delivered, json!({}), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "delivered");
    assert_eq!(
        call(&path, "POST", &delivered, json!({}), AUTHOR).await.1["authorMessageId"],
        saved["authorMessageId"]
    );
    let (_, cancelled) = call(
        &path,
        "GET",
        &format!("{base}/tasks/{id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(cancelled["status"], "cancelled");
}
