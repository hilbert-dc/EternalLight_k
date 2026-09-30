use super::{AUTHOR, CANDIDATE, CAPABILITY, call, setup};
use axum::http::StatusCode;
use eternallight_engine_core::PersistentBudget;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

async fn claimed(path: &Path) -> (String, Value, Value) {
    let (base, request) = setup(path).await;
    let (status, run) = call(
        path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let (status, next) = call(
        path,
        "POST",
        &format!(
            "{base}/generation-runs/{}/next",
            run["id"].as_str().unwrap()
        ),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    let (status, task) = call(path,"POST",&format!("{base}/tasks"),json!({"operation":"generateBrief","inputRevision":0,"threadId":"generation-thread","target":next["action"]["target"],"authorIntent":"客户端不能替换授权","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":run["deadlineAt"]}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    (base, run, task)
}

#[tokio::test]
async fn generation_scope_confirmation_commits_with_author_run_and_single_account() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scope-confirm.sqlite3");
    let (base, request) = setup(&path).await;
    let uri = format!("{base}/generation-runs");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_scope BEFORE INSERT ON story_execution_scopes_v1 BEGIN SELECT RAISE(ABORT,'scope failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_generation_runs_v1),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_execution_scopes_v1)",[],|row|Ok((row.get::<_, i64>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?,row.get::<_, i64>(3)?,row.get::<_, i64>(4)?))).unwrap(),(0,0,0,0,0));
    db.execute_batch("DROP TRIGGER fail_scope;").unwrap();
    let (status, run) = call(&path, "POST", &uri, request.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let (kind, source, owner, message, payload): (String,Option<String>,String,String,String) = db.query_row("SELECT kind,source_task_id,work_task_id,author_message_id,scope_json FROM story_execution_scopes_v1 WHERE generation_run_id=?1",[run["id"].as_str().unwrap()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(
        (kind, source, owner, message),
        (
            "generation".into(),
            None,
            format!("work-{}", run["id"].as_str().unwrap()),
            run["authorMessageId"].as_str().unwrap().into()
        )
    );
    let scope: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        json!({"bounds":scope["bounds"],"requests":scope["maxRequests"],"tokens":scope["maxTokens"],"snapshot":scope["initialSnapshotId"],"source":scope["initialSourceDigest"]}),
        json!({"bounds":{"startChapterNumber":1,"targetChapters":1,"maxTaskAttempts":7,"deadlineAt":request["deadlineAt"]},"requests":14,"tokens":200000,"snapshot":request["expectedBaseSnapshotId"],"source":request["expectedSourceDigest"]})
    );
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_task_provider_budgets_v1),(SELECT COUNT(*) FROM story_tasks_v2)",[],|row|Ok((row.get::<_, i64>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?))).unwrap(),(1,0,0));
}

#[tokio::test]
async fn mutable_run_bounds_or_missing_scope_cannot_authorize_context_budget_or_candidate_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scope-fence.sqlite3");
    let (base, run, task) = claimed(&path).await;
    let id = run["id"].as_str().unwrap();
    let task_id = task["id"].as_str().unwrap();
    let run_uri = format!("{base}/generation-runs/{id}");
    let context_uri = format!(
        "{base}/context?taskId={task_id}&threadId=generation-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let budget_uri = format!("{base}/tasks/{task_id}/provider-budget");
    assert_eq!(
        call(&path, "GET", &context_uri, Value::Null, CANDIDATE)
            .await
            .0,
        StatusCode::OK
    );
    let project = ProjectId::parse(base.trim_start_matches("/api/projects/")).unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let before = repository.generation_run(&project, id).unwrap();
    let original: Value = serde_json::from_str(&before.state_json).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let save = json!({"taskId":task_id,"threadId":"generation-thread","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"scope-brief","payload":{"premise":"港城证词"}});
    for (field, value) in [
        ("maxTaskAttempts", json!(32)),
        ("deadlineAt", json!(run["deadlineAt"].as_i64().unwrap() + 1)),
        ("targetChapters", json!(2)),
        ("startChapterNumber", json!(2)),
    ] {
        let mut next = original.clone();
        next[field] = value.clone();
        next["version"] = json!(before.version + 1);
        assert!(
            repository
                .update_generation_run(&project, id, before.version, &next.to_string())
                .is_err(),
            "{field}"
        );
        assert_eq!(repository.generation_run(&project, id).unwrap(), before);
        let mut damaged: Value = serde_json::from_str(&stored).unwrap();
        damaged[field] = value;
        db.execute(
            "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
            rusqlite::params![damaged.to_string(), id],
        )
        .unwrap();
        for uri in [&run_uri, &budget_uri] {
            assert!(
                !call(&path, "GET", uri, Value::Null, AUTHOR)
                    .await
                    .0
                    .is_success(),
                "{field}"
            );
        }
        assert!(
            !call(&path, "GET", &context_uri, Value::Null, CANDIDATE)
                .await
                .0
                .is_success(),
            "{field}"
        );
        assert!(
            !call(
                &path,
                "POST",
                &format!("{base}/artifacts/brief"),
                save.clone(),
                CANDIDATE
            )
            .await
            .0
            .is_success(),
            "{field}"
        );
        db.execute(
            "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
            rusqlite::params![stored, id],
        )
        .unwrap();
    }
    let (permit,payload,digest,message,confirmed): (String,String,String,String,i64) = db.query_row("SELECT permit_id,scope_json,preview_digest,author_message_id,confirmed_at FROM story_execution_scopes_v1 WHERE generation_run_id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    db.execute(
        "DELETE FROM story_execution_scopes_v1 WHERE permit_id=?1",
        [&permit],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &context_uri, Value::Null, CANDIDATE)
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
    assert!(
        !call(
            &path,
            "POST",
            &format!("{base}/artifacts/brief"),
            save.clone(),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_outputs_v2 WHERE task_id=?1",
            [task_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute("INSERT INTO story_execution_scopes_v1(permit_id,work_task_id,kind,author_message_id,generation_run_id,scope_json,preview_digest,confirmed_at,status) VALUES(?1,?2,'generation',?3,?4,?5,?6,?7,'active')",rusqlite::params![permit,format!("work-{id}"),message,id,payload,digest,confirmed]).unwrap();
    assert_eq!(
        call(&path, "GET", &context_uri, Value::Null, CANDIDATE)
            .await
            .0,
        StatusCode::OK
    );
    let (status, grant) = call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    let mut engine = PersistentBudget::new(&path.with_extension("budget.sqlite3"),task_id,json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000},"now":1000})).unwrap();
    engine
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":100},"now":1001}),
        )
        .unwrap();
    let (status, output) = call(
        &path,
        "POST",
        &format!("{base}/artifacts/brief"),
        save,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{output}");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/tasks/{task_id}/complete"),
            json!({"threadId":"generation-thread","summary":"候选已保存"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    engine.complete("completed").unwrap();
    let (status, charged) = call(
        &path,
        "POST",
        &format!("{run_uri}/provider-usage"),
        json!({"taskId":task_id}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    assert_eq!(
        json!({"requests":charged["providerBudget"]["usedRequests"],"unknown":charged["providerBudget"]["unknownTokens"],"deadline":charged["deadlineAt"]}),
        json!({"requests":1,"unknown":100,"deadline":run["deadlineAt"]})
    );
    let (status, advanced) = call(
        &path,
        "POST",
        &format!("{run_uri}/advance-task"),
        json!({"taskId":task_id,"expectedVersion":charged["version"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{advanced}");
    assert_eq!(advanced["stage"], "outline");
    assert_eq!(
        db.query_row(
            "SELECT scope_json FROM story_execution_scopes_v1 WHERE permit_id=?1",
            [permit],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        payload
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_work_budgets_v1", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        1
    );
}
