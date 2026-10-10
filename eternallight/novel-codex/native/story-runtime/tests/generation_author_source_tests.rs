use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

const AUTHOR: &str = "generation-source-author-token-1234567890";
const CANDIDATE: &str = "generation-source-candidate-token-1234567";
const CAPABILITY: &str = "generation-source-task-capability-1234567";

#[path = "generation_author_source/execution_scope_tests.rs"]
mod execution_scope;

#[path = "generation_author_source/delivery_tests.rs"]
mod delivery;

#[path = "generation_author_source/materialization_tests.rs"]
mod materialization;

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
    let (status, project) = call(
        path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"作者来源"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{project}");
    let base = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
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
    let request = json!({"expectedProjectRevision":workspace["revision"],"expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
        "authorIntent":"写港城故事，保留白塔设定","targetChapters":1,"maxTaskAttempts":7,"maxProviderRequests":14,"maxProviderTokens":200000,"deadlineAt":now+600});
    (base, request)
}

#[tokio::test]
async fn generation_author_origin_is_atomic_and_cannot_be_created_by_candidate_or_injected_into_start()
 {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.sqlite3");
    let (base, request) = setup(&path).await;
    let uri = format!("{base}/generation-runs");
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let mut injected = request.clone();
    injected["authorMessageId"] = json!("forged-author");
    assert!(
        !call(&path, "POST", &uri, injected, AUTHOR)
            .await
            .0
            .is_success()
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_generation_message BEFORE INSERT ON story_author_messages_v1 BEGIN SELECT RAISE(ABORT,'message failure'); END;").unwrap();
    assert!(
        !call(&path, "POST", &uri, request.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_generation_runs_v1),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap(),(0,0,0,0));
    db.execute_batch("DROP TRIGGER reject_generation_message;")
        .unwrap();
    let (status, run) = call(&path, "POST", &uri, request.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let origin: (String,String,String,Option<String>,Option<String>,String)=db.query_row("SELECT message_id,generation_run_id,work_task_id,task_id,thread_id,author_text FROM story_author_messages_v1",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).unwrap();
    let id = run["id"].as_str().unwrap();
    assert_eq!(
        origin,
        (
            run["authorMessageId"].as_str().unwrap().into(),
            id.into(),
            format!("work-{id}"),
            None,
            None,
            request["authorIntent"].as_str().unwrap().into()
        )
    );
    assert!(
        !call(&path, "POST", &uri, request, AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_generation_runs_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_tasks_v2)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap(),(1,1,1,0));
}

#[tokio::test]
async fn generation_tasks_rebuild_author_history_after_reopen_and_reject_source_corruption() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.sqlite3");
    let (base, request) = setup(&path).await;
    let (status, run) = call(
        &path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap();
    let uri = format!("{base}/generation-runs/{id}");
    let (status, updated) = call(
        &path,
        "POST",
        &format!("{uri}/future-intents"),
        json!({"expectedVersion":run["version"],"instruction":"林夕不能知道密函内容"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["authorMessageId"], run["authorMessageId"]);
    let source = &updated["intentRevisions"][1]["source"];
    assert_eq!(source["type"], "authorMessage");
    assert_ne!(source["authorMessageId"], run["authorMessageId"]);
    let db = rusqlite::Connection::open(&path).unwrap();
    let append: (String, String, String, i64, i64) = db.query_row(
        "SELECT author_text,generation_run_id,work_task_id,intent_revision,created_at FROM story_author_messages_v1 WHERE message_id=?1",
        [source["authorMessageId"].as_str().unwrap()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).unwrap();
    assert_eq!(
        append,
        (
            "林夕不能知道密函内容".into(),
            id.into(),
            format!("work-{id}"),
            1,
            source["createdAt"].as_i64().unwrap()
        )
    );
    let (status, next) = call(&path, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    let claim = json!({"operation":next["action"]["operation"],"inputRevision":0,"threadId":"generation-thread","target":next["action"]["target"],"authorIntent":"篡改为删除白塔并泄露秘密",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":run["deadlineAt"]});
    let (status, task) = call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    assert_eq!(
        task["inputManifest"]["authorIntent"],
        json!(
            novel_codex_story_core::generation_author_intent(
                &serde_json::from_value::<Vec<novel_codex_story_core::CreativeIntentRevision>>(
                    updated["intentRevisions"].clone()
                )
                .unwrap(),
                1
            )
            .unwrap()
        )
    );
    assert_eq!(
        task["inputManifest"]["generationAuthorIntentBinding"],
        json!({"runId":id,"authorMessageId":run["authorMessageId"],"intentGeneration":1,"chapterNumber":1})
    );
    let task_id = task["id"].as_str().unwrap();
    let budget_uri = format!("{base}/tasks/{task_id}/provider-budget");
    let grant = call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await;
    assert_eq!(grant.0, StatusCode::OK, "{}", grant.1);
    assert_eq!(grant.1["workTaskId"], format!("work-{id}"));
    let context_uri = format!(
        "{base}/context?taskId={task_id}&threadId=generation-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&path, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    db.execute("INSERT INTO story_work_tasks_v1(id,project_id,thread_id,goal,kind,status,input_revision) SELECT 'foreign-work',project_id,'','other author work','generation','running',input_revision FROM story_work_tasks_v1 WHERE id=?1",[format!("work-{id}")]).unwrap();
    for (column, value) in [
        ("author_text", json!("模型篡改")),
        ("created_at", json!(-1)),
        ("intent_revision", json!(2)),
        ("work_task_id", json!("foreign-work")),
    ] {
        // Damage an existing source without changing task/run JSON: all readers must fail closed.
        db.execute(
            &format!("UPDATE story_author_messages_v1 SET {column}=?1 WHERE message_id=?2"),
            rusqlite::params![
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
                source["authorMessageId"].as_str().unwrap()
            ],
        )
        .unwrap();
        assert!(
            !call(&path, "GET", &uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success(),
            "{column}"
        );
        assert!(
            !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success(),
            "{column}"
        );
        assert!(
            !call(&path, "GET", &context_uri, Value::Null, CANDIDATE)
                .await
                .0
                .is_success(),
            "{column}"
        );
        db.execute("UPDATE story_author_messages_v1 SET author_text=?1,created_at=?2,intent_revision=1,work_task_id=?3 WHERE message_id=?4",
            rusqlite::params![append.0,append.4,append.2,source["authorMessageId"].as_str().unwrap()]).unwrap();
        assert_eq!(
            call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await.0,
            StatusCode::OK
        );
    }
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse(base.trim_start_matches("/api/projects/")).unwrap();
    let before = repo.generation_run(&project, id).unwrap();
    let mut forged: Value = serde_json::from_str(&before.state_json).unwrap();
    forged["version"] = json!(before.version + 1);
    forged["intentRevisions"][1]["instruction"] = json!("重写已交付要求");
    assert!(
        repo.update_generation_run(&project, id, before.version, &forged.to_string())
            .is_err()
    );
    assert_eq!(repo.generation_run(&project, id).unwrap(), before);
    let db = rusqlite::Connection::open(&path).unwrap();
    for field in ["generationAuthorIntentBinding", "authorMessageId"] {
        let mut damaged = task["inputManifest"].clone();
        damaged.as_object_mut().unwrap().remove(field);
        db.execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
            rusqlite::params![damaged.to_string(), task_id],
        )
        .unwrap();
        assert!(
            !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success(),
            "missing {field}"
        );
    }
    let mut anonymous = task["inputManifest"].clone();
    anonymous
        .as_object_mut()
        .unwrap()
        .remove("generationAuthorIntentBinding");
    anonymous.as_object_mut().unwrap().remove("authorMessageId");
    let mut target = task["target"].clone();
    target.as_object_mut().unwrap().remove("runId");
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1,target_json=?2 WHERE task_id=?3",
        rusqlite::params![anonymous.to_string(), target.to_string(), task_id],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
    db.execute(
        "UPDATE story_tasks_v2 SET target_json=?1 WHERE task_id=?2",
        rusqlite::params![task["target"].to_string(), task_id],
    )
    .unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        rusqlite::params![task["inputManifest"].to_string(), task_id],
    )
    .unwrap();
    db.execute(
        "UPDATE story_author_messages_v1 SET author_text='模型伪造要求' WHERE generation_run_id=?1",
        [id],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &uri, Value::Null, AUTHOR)
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
        !call(&path, "GET", &context_uri, Value::Null, CANDIDATE)
            .await
            .0
            .is_success()
    );
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_task_provider_budgets_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap(),(1,1,1,2));
}

#[tokio::test]
async fn generation_append_is_author_owned_raw_and_atomic_with_run_work_and_budget() {
    for table in [
        "story_author_messages_v1",
        "story_generation_runs_v1",
        "story_work_tasks_v1",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("append.sqlite3");
        let (base, mut request) = setup(&path).await;
        request["authorIntent"] = json!("  港城故事🌙\n保留白塔  ");
        let (status, run) = call(
            &path,
            "POST",
            &format!("{base}/generation-runs"),
            request.clone(),
            AUTHOR,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        assert_eq!(
            run["intentRevisions"][0]["instruction"],
            request["authorIntent"]
        );
        let id = run["id"].as_str().unwrap();
        let uri = format!("{base}/generation-runs/{id}");
        let append_uri = format!("{uri}/future-intents");
        let append = json!({"expectedVersion":run["version"],"instruction":"  人物动机：不相信预言🌙\n结局保持开放  "});
        assert_eq!(
            call(&path, "POST", &append_uri, append.clone(), CANDIDATE)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let mut injected = append.clone();
        injected["authorMessageId"] = json!("forged-message");
        assert!(
            !call(&path, "POST", &append_uri, injected, AUTHOR)
                .await
                .0
                .is_success()
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        let before: (String, i64, String) = db.query_row("SELECT state_json,(SELECT version FROM story_work_tasks_v1 WHERE id=?2),(SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?2) FROM story_generation_runs_v1 WHERE run_id=?1",rusqlite::params![id,format!("work-{id}")],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        let operation = if table == "story_author_messages_v1" {
            "INSERT"
        } else {
            "UPDATE"
        };
        db.execute_batch(&format!("CREATE TRIGGER reject_append BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'append failure'); END;")).unwrap();
        assert!(
            !call(&path, "POST", &append_uri, append.clone(), AUTHOR)
                .await
                .0
                .is_success(),
            "{table}"
        );
        let after: (String, i64, String) = db.query_row("SELECT state_json,(SELECT version FROM story_work_tasks_v1 WHERE id=?2),(SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?2) FROM story_generation_runs_v1 WHERE run_id=?1",rusqlite::params![id,format!("work-{id}")],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(after, before, "{table}");
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
        db.execute_batch("DROP TRIGGER reject_append;").unwrap();
        let (status, updated) = call(&path, "POST", &append_uri, append.clone(), AUTHOR).await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(updated["providerBudget"], run["providerBudget"]);
        assert_eq!(
            updated["intentRevisions"][1]["instruction"],
            append["instruction"]
        );
        let message = &updated["intentRevisions"][1]["source"];
        let stored: (String,i64,i64)=db.query_row("SELECT author_text,intent_revision,created_at FROM story_author_messages_v1 WHERE message_id=?1",[message["authorMessageId"].as_str().unwrap()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(
            stored,
            (
                append["instruction"].as_str().unwrap().into(),
                1,
                message["createdAt"].as_i64().unwrap()
            )
        );
        assert_eq!(
            call(&path, "GET", &uri, Value::Null, AUTHOR).await,
            (StatusCode::OK, updated)
        );
        assert!(
            !call(&path, "POST", &append_uri, append, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
    }
}

#[tokio::test]
async fn future_history_must_fit_before_commit_even_when_current_chapter_still_fits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bounded-future.sqlite3");
    let (base, mut request) = setup(&path).await;
    request["authorIntent"] = json!("海".repeat(4_000));
    request["targetChapters"] = json!(3);
    request["maxTaskAttempts"] = json!(17);
    request["maxProviderRequests"] = json!(34);
    let (status, run) = call(
        &path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap();
    let project = ProjectId::parse(base.trim_start_matches("/api/projects/")).unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    for generation in 1..=4 {
        let before = repo.generation_run(&project, id).unwrap();
        let mut next: Value = serde_json::from_str(&before.state_json).unwrap();
        next["version"] = json!(before.version + 1);
        next["intentGeneration"] = json!(generation);
        next["intentRevisions"].as_array_mut().unwrap().push(json!({
            "id":format!("future-intent-{generation}"), "generation":generation,
            "instruction":"海".repeat(1_000), "effectiveFromChapter":2,
            "source":{"type":"authorMessage","authorMessageId":format!("future-message-{generation}"),"createdAt":1}
        }));
        let history: Vec<novel_codex_story_core::CreativeIntentRevision> =
            serde_json::from_value(next["intentRevisions"].clone()).unwrap();
        assert!(novel_codex_story_core::generation_author_intent(&history, 1).is_ok());
        let result = repo.update_generation_run(&project, id, before.version, &next.to_string());
        if generation == 4 {
            assert_eq!(
                result,
                Err(novel_codex_story_core::RepositoryError::TaskBindingInvalid)
            );
            assert_eq!(repo.generation_run(&project, id).unwrap(), before);
        } else {
            result.unwrap();
        }
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        4
    );
}

#[tokio::test]
async fn damaged_append_cannot_complete_a_real_saved_output_and_restoration_keeps_its_allowance() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("completion-source.sqlite3");
    let (base, request) = setup(&path).await;
    let (status, run) = call(
        &path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let uri = format!("{base}/generation-runs/{}", run["id"].as_str().unwrap());
    let (status, updated) = call(
        &path,
        "POST",
        &format!("{uri}/future-intents"),
        json!({"expectedVersion":run["version"],"instruction":"记录白塔，不泄露密函"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let action = prepared_action(
        &path,
        &base,
        &updated,
        "generateBrief",
        "completion-source-thread",
    )
    .await;
    let (status,task)=call(&path,"POST",&format!("{base}/tasks"),json!({"operation":"generateBrief","inputRevision":0,"threadId":"completion-source-thread","target":action["target"],"authorIntent":"untrusted wrapper",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":run["deadlineAt"]}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (status,output)=call(&path,"POST",&format!("{base}/artifacts/brief"),json!({"payload":{"premise":"抄写员为遗忘中的港城记录白塔。"},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"completion-source-brief",
        "taskId":task_id,"threadId":"completion-source-thread"}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{output}");
    let db = rusqlite::Connection::open(&path).unwrap();
    let message = updated["intentRevisions"][1]["source"]["authorMessageId"]
        .as_str()
        .unwrap();
    let before: (String,String,String)=db.query_row("SELECT ledger_json,(SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1),(SELECT status FROM story_tasks_v2 WHERE task_id=?2) FROM story_work_budgets_v1 WHERE work_task_id='work-'||?1",rusqlite::params![run["id"].as_str().unwrap(),task_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    db.execute(
        "UPDATE story_author_messages_v1 SET author_text='伪造追加' WHERE message_id=?1",
        [message],
    )
    .unwrap();
    let complete = json!({"threadId":"completion-source-thread","summary":"保存创作简报"});
    let (status, rejected) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{task_id}/complete"),
        complete.clone(),
        CANDIDATE,
    )
    .await;
    assert_eq!(
        (status, rejected["error"]["message"].clone()),
        (StatusCode::BAD_REQUEST, json!("TaskBindingInvalid"))
    );
    let after: (String,String,String)=db.query_row("SELECT ledger_json,(SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1),(SELECT status FROM story_tasks_v2 WHERE task_id=?2) FROM story_work_budgets_v1 WHERE work_task_id='work-'||?1",rusqlite::params![run["id"].as_str().unwrap(),task_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(after, before);
    db.execute(
        "UPDATE story_author_messages_v1 SET author_text=?1 WHERE message_id=?2",
        rusqlite::params![
            updated["intentRevisions"][1]["instruction"]
                .as_str()
                .unwrap(),
            message
        ],
    )
    .unwrap();
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{task_id}/complete"),
        complete,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        db.query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id='work-'||?1",
            [run["id"].as_str().unwrap()],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        before.0
    );
}

async fn prepared_action(
    path: &std::path::Path,
    base: &str,
    run: &Value,
    operation: &str,
    thread: &str,
) -> Value {
    let uri = format!("{base}/generation-runs/{}", run["id"].as_str().unwrap());
    let (code, next) = call(path, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{next}");
    let (_, workspace) = call(
        path,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (code,parent)=call(path,"POST",&format!("{base}/tasks"),json!({"operation":"advisory","inputRevision":workspace["revision"],"threadId":thread,"target":next["action"]["target"],"authorIntent":"wrapper","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{parent}");
    let id = parent["id"].as_str().unwrap();
    let revision = parent["inputRevision"].as_i64().unwrap();
    let (code, plan) = call(
        path,
        "GET",
        &format!("{base}/tasks/{id}/work-plan?threadId={thread}&expectedRevision={revision}"),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{plan}");
    if plan["version"] == 0 {
        let (code, saved) = call(path,"POST",&format!("{base}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":thread,"expectedRevision":revision,"expectedPlanVersion":0,"idempotencyKey":"source-execution-plan","plan":{"intentMode":"executeGoals","goals":[{"id":"source-creation","title":"按作者批准范围完成港城故事","acceptance":"保留白塔设定，完成本次全部授权工作","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"当前保存成果与作者要求是否一致","checkScope":"本次作者批准范围","approach":"按实际状态选下一步并独立核对正文","assessment":"","sourceIds":[],"sourceChecks":[],"evidence":[],"continuation":null}]}}),CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{saved}");
    }
    let (code, mut status) = call(
        path,
        "GET",
        &format!("{base}/tasks/{id}/work-decision?threadId={thread}&expectedRevision={revision}"),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{status}");
    let mut cursor = status["pagination"]["nextCursor"].clone();
    while let Some(next) = cursor.as_str() {
        let (code,page) = call(path,"GET",&format!("{base}/tasks/{id}/work-decision?threadId={thread}&expectedRevision={revision}&cursor={next}"),Value::Null,CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        for field in ["readyActions", "requirements"] {
            status[field]
                .as_array_mut()
                .unwrap()
                .extend(page[field].as_array().unwrap().iter().cloned());
        }
        cursor = page["pagination"]["nextCursor"].clone();
    }
    let key = status["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == operation)
        .unwrap_or_else(|| panic!("{operation}: {status}"))["actionKey"]
        .clone();
    let mut choice = json!({"taskId":id,"threadId":thread,"expectedRevision":revision,"expectedWorkVersion":status["version"],"expectedSourceDigest":status["sourceDigest"],"actionKey":key,"instruction":"按原作者许可执行这项工作","idempotencyKey":format!("choose-{operation}")});
    if matches!(operation, "writeNextChapter" | "generateChapter") {
        choice["chapterAcceptance"] = if status["chapterAcceptance"].is_object() {
            status["chapterAcceptance"].clone()
        } else {
            json!({"requirements":[{"id":"whole-chapter","text":"按作者要求保存完整章节，保留白塔设定"}],"length":{"requirement":"作者未限定本次正文字符数","minCharacters":null,"maxCharacters":null}})
        };
    }
    let (code, selected) = call(
        path,
        "POST",
        &format!("{base}/tasks/{id}/work-decision/actions"),
        choice,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{selected}");
    let (grant_status, grant) = call(
        path,
        "GET",
        &format!("{base}/tasks/{id}/provider-budget"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(grant_status, StatusCode::OK, "{grant}");
    let (code, completed) = call(
        path,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":thread,"summary":"已选下一项工作"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{completed}");
    let mut budget=eternallight_engine_core::PersistentBudget::new(&path.with_extension("budget.sqlite3"),id,json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000},"now":1000})).unwrap();
    budget.complete("completed").unwrap();
    let (code, charged) = call(
        path,
        "POST",
        &format!("{uri}/provider-usage"),
        json!({"taskId":id}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{charged}");
    let (code, advanced) = call(
        path,
        "POST",
        &format!("{uri}/advance-task"),
        json!({"expectedVersion":charged["version"],"taskId":id}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{advanced}");
    let (code, next) = call(path, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{next}");
    next["action"].clone()
}
