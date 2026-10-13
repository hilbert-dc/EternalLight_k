use super::{AUTHOR, CANDIDATE, append, call, setup};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn reply(revision: i64) -> Value {
    json!({"threadId":"intent-source-thread","turnId":"actual-turn","intentRevision":revision,
        "itemId":"final-item","text":"## 结论\n\n保留结局，调整人物动机🌙","truncated":false})
}

#[tokio::test]
async fn host_reply_is_durable_exact_replay_and_not_a_model_completion_claim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reply.sqlite3");
    let (base, task) = setup(&path).await;
    let task_path = format!("{base}/tasks/{}", task["id"].as_str().unwrap());
    let uri = format!("{task_path}/final-reply");
    assert_eq!(
        call(&path, "POST", &uri, reply(0), CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        !call(&path, "POST", &uri, reply(0), AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_path}/turn"),
            json!({"threadId":"intent-source-thread","turnId":"actual-turn"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_path}/complete"),
            json!({"threadId":"intent-source-thread","summary":"模型声称完成"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, Value::Null)
    );
    let (status, receipt) = call(&path, "POST", &uri, reply(0), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["binding"], reply(0));
    assert_eq!(receipt["state"], "recorded");
    assert_eq!(
        call(&path, "POST", &uri, reply(0), AUTHOR).await,
        (StatusCode::OK, receipt.clone())
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, receipt.clone())
    );
    for patch in [
        json!({"text":"替换结论"}),
        json!({"turnId":"foreign"}),
        json!({"threadId":"foreign"}),
        json!({"intentRevision":1}),
    ] {
        let mut changed = reply(0);
        changed
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            !call(&path, "POST", &uri, changed, AUTHOR)
                .await
                .0
                .is_success()
        );
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE story_task_final_replies_v1 SET receipt_json=json_set(receipt_json,'$.binding.text','伪造文本')",[]).unwrap();
    assert!(
        !call(&path, "GET", &uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
}

#[tokio::test]
async fn reply_requires_delivered_latest_intent_and_closes_the_steering_race() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("steer.sqlite3");
    let (base, task) = setup(&path).await;
    let task_path = format!("{base}/tasks/{}", task["id"].as_str().unwrap());
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_path}/turn"),
            json!({"threadId":"intent-source-thread","turnId":"actual-turn"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_path}/intents"),
            append(),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    for revision in [0, 1] {
        assert!(
            !call(
                &path,
                "POST",
                &format!("{task_path}/final-reply"),
                reply(revision),
                AUTHOR
            )
            .await
            .0
            .is_success()
        );
    }
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_path}/intents/1/delivered"),
            json!({}),
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
            &format!("{task_path}/final-reply"),
            reply(0),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, receipt) = call(
        &path,
        "POST",
        &format!("{task_path}/final-reply"),
        reply(1),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let late = json!({"expectedIntentRevision":1,"authorText":"晚到的追加"});
    assert!(
        !call(&path, "POST", &format!("{task_path}/intents"), late, AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{task_path}/final-reply"),
            Value::Null,
            AUTHOR
        )
        .await,
        (StatusCode::OK, receipt)
    );
}

#[tokio::test]
async fn absent_truncated_and_cancelled_attempts_are_not_full_reply_evidence() {
    for kind in ["absent", "truncated", "cancelled"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("limits.sqlite3");
        let (base, task) = setup(&path).await;
        let task_path = format!("{base}/tasks/{}", task["id"].as_str().unwrap());
        assert_eq!(
            call(
                &path,
                "POST",
                &format!("{task_path}/turn"),
                json!({"threadId":"intent-source-thread","turnId":"actual-turn"}),
                AUTHOR
            )
            .await
            .0,
            StatusCode::OK
        );
        let uri = format!("{task_path}/final-reply");
        let mut too_large = reply(0);
        too_large["text"] = json!("🌙".repeat(16_385));
        assert!(
            !call(&path, "POST", &uri, too_large, AUTHOR)
                .await
                .0
                .is_success()
        );
        let mut binding = reply(0);
        if kind == "cancelled" {
            assert_eq!(
                call(
                    &path,
                    "POST",
                    &format!("{task_path}/cancel"),
                    json!({}),
                    AUTHOR
                )
                .await
                .0,
                StatusCode::OK
            );
            assert!(
                !call(&path, "POST", &uri, binding, AUTHOR)
                    .await
                    .0
                    .is_success()
            );
            assert_eq!(
                call(&path, "GET", &uri, Value::Null, AUTHOR).await,
                (StatusCode::OK, Value::Null)
            );
            continue;
        }
        if kind == "absent" {
            binding["itemId"] = Value::Null;
            binding["text"] = json!("");
        } else {
            binding["text"] = json!("🌙".repeat(16_384));
            binding["truncated"] = json!(true);
        }
        let (status, receipt) = call(&path, "POST", &uri, binding.clone(), AUTHOR).await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert_eq!(
            (receipt["state"].clone(), receipt["binding"].clone()),
            (json!(kind), binding)
        );
        assert_eq!(
            call(&path, "GET", &uri, Value::Null, AUTHOR).await,
            (StatusCode::OK, receipt)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn competing_receipts_commit_once_and_survive_later_domain_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("competing.sqlite3");
    let (base, task) = setup(&path).await;
    let task_path = format!("{base}/tasks/{}", task["id"].as_str().unwrap());
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{task_path}/turn"),
            json!({"threadId":"intent-source-thread","turnId":"actual-turn"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let uri = format!("{task_path}/final-reply");
    let mut other = reply(0);
    other["text"] = json!("另一个回复，不能覆盖已提交内容");
    let (first, second) = tokio::join!(
        call(&path, "POST", &uri, reply(0), AUTHOR),
        call(&path, "POST", &uri, other, AUTHOR),
    );
    assert_ne!(first.0.is_success(), second.0.is_success());
    let receipt = if first.0.is_success() {
        first.1
    } else {
        second.1
    };
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, receipt.clone())
    );
    assert_eq!(
        call(
            &path,
            "PATCH",
            &task_path,
            json!({"threadId":"intent-source-thread","status":"failed","error":"领域提交失败"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, receipt.clone())
    );
    assert_eq!(
        call(&path, "POST", &uri, receipt["binding"].clone(), AUTHOR).await,
        (StatusCode::OK, receipt)
    );
    assert_eq!(
        call(&path, "GET", &task_path, Value::Null, AUTHOR).await.1["status"],
        "failed"
    );
    let count: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM story_task_final_replies_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
