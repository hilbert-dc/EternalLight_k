use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "recovery-author-token-1234567890";
const CANDIDATE: &str = "recovery-candidate-token-1234567890";
const CAP: &str = "recovery-task-capability-1234567890";

async fn request(
    app: &Router,
    path: &str,
    actor: &str,
    body: Value,
    metadata: Option<(&str, &str, &str)>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {actor}"))
        .header("content-type", "application/json")
        .header("x-novel-task-capability", CAP);
    if let Some((tool, call, caller)) = metadata {
        for (name, value) in [
            ("x-novel-tool-name", tool),
            ("x-novel-tool-call", call),
            ("x-novel-root-thread", "root"),
            ("x-novel-caller-thread", caller),
            ("x-novel-turn", "turn"),
        ] {
            builder = builder.header(name, value);
        }
        builder = builder.header(
            "x-novel-request-hash",
            format!(
                "{:x}",
                Sha256::digest(json!({"tool":tool,"arguments":body}).to_string().as_bytes())
            ),
        );
    }
    let response = app
        .clone()
        .author_fixture_request(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn candidate_write_ledger_is_author_only_and_keeps_exact_proofs_after_router_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let project = ProjectId::parse("recovery-http").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "恢复".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "task".into(),
        project_id: project.clone(),
        thread_id: "root".into(),
        operation: "generateBrief".into(),
        input_revision: 0,
        model: None,
        target_json: "{}".into(),
        input_manifest_json: "{}".into(),
        capability_hash: format!("{:x}", Sha256::digest(CAP.as_bytes())),
        capability_expires_at: 9999999999,
    })
    .unwrap();
    repo.bind_task_turn(&project, "task", "root", "turn")
        .unwrap();
    drop(repo);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let route = "/api/projects/recovery-http/artifacts/brief";
    let body = json!({"taskId":"task","threadId":"root","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"save-key","payload":{"premise":"正文不是恢复元数据"}});
    let (status, _) = request(
        &app,
        route,
        CANDIDATE,
        body.clone(),
        Some(("save_brief", "child-call", "child")),
    )
    .await;
    assert!(!status.is_success());
    let (status, saved) = request(
        &app,
        route,
        CANDIDATE,
        body.clone(),
        Some(("save_brief", "save-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    drop(app); // Drop response observer and reopen Runtime over the persisted database.
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let recovery = "/api/projects/recovery-http/tasks/recovery-snapshot";
    let query = json!({"taskIds":["task"],"limit":1});
    let (status, _) = request(&app, recovery, CANDIDATE, query.clone(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, proof) = request(&app, recovery, AUTHOR, query.clone(), None).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operations"][0]["tool_call_id"], "save-call");
    assert_eq!(
        proof["operations"][0]["receipt_ref"],
        format!(
            "runtime:candidate:{}",
            saved["revisionId"].as_str().unwrap()
        )
    );
    assert!(!proof.to_string().contains("正文不是恢复元数据"));
    let (status, replayed) = request(
        &app,
        route,
        CANDIDATE,
        body.clone(),
        Some(("save_brief", "replay-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{replayed}");
    assert_eq!(replayed["revisionId"], saved["revisionId"]);
    let (status, page) = request(&app, recovery, AUTHOR, query.clone(), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["complete"], false);
    let (status, next) = request(
        &app,
        recovery,
        AUTHOR,
        json!({"taskIds":["task"],"fence":page["fence"],"cursor":1,"limit":1}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        next["operations"][0]["receipt_ref"],
        proof["operations"][0]["receipt_ref"]
    );
    let mut different = body.clone();
    different["payload"] = json!({"premise":"different"});
    let (status, _) = request(
        &app,
        route,
        CANDIDATE,
        different,
        Some(("save_brief", "save-call", "root")),
    )
    .await;
    assert!(!status.is_success());
    let mut failed = body;
    failed["idempotencyKey"] = json!("bad-cas");
    let (status, _) = request(
        &app,
        route,
        CANDIDATE,
        failed,
        Some(("save_brief", "failed-call", "root")),
    )
    .await;
    assert!(!status.is_success());
    let (status, latest) = request(&app, recovery, AUTHOR, json!({"taskIds":["task"]}), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["operations"][2]["state"], "accepted");
    assert_eq!(latest["operations"][2]["receipt_ref"], Value::Null);
    assert_eq!(latest["operations"].as_array().unwrap().len(), 3);
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

#[tokio::test]
async fn rejected_candidate_proposal_is_known_zero_but_storage_failure_remains_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proposal-recovery.sqlite3");
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = request(
        &app,
        "/api/projects",
        AUTHOR,
        json!({"title":"提议拒绝恢复"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status,task) = request(&app,&format!("{base}/tasks"),AUTHOR,json!({
        "operation":"advisory","inputRevision":0,"threadId":"root","target":{"type":"project"},
        "authorIntent":"为小说拟一份创作简报","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),
        "capabilityExpiresAt":expires}),None).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    SqliteStoryRepository::open(&path)
        .unwrap()
        .bind_task_turn(&ProjectId::parse(project).unwrap(), task_id, "root", "turn")
        .unwrap();
    let route = format!("{base}/tasks/{task_id}/actions");
    let mut body = json!({"taskId":task_id,"threadId":"root","expectedRevision":0,"expectedIntentRevision":99,
        "idempotencyKey":"stale-proposal","operation":"generateBrief","target":{},"instruction":"只调整简报"});
    let (status, rejected) = request(
        &app,
        &route,
        CANDIDATE,
        body.clone(),
        Some(("request_candidate_task", "rejected-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("authorIntentState.expectedIntentRevision"),
        "{rejected}"
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT state FROM story_tool_operations_v1 WHERE call_id='rejected-call'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "rejected"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_action_requests_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_task_outputs_v2", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // Correct a definite rejection before any unknown operation exists.
    body["expectedIntentRevision"] = json!(0);
    body["idempotencyKey"] = json!("corrected-proposal");
    let (status, saved) = request(
        &app,
        &route,
        CANDIDATE,
        body.clone(),
        Some(("request_candidate_task", "corrected-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    // A later proposal suffers a storage failure. Do not retry it or submit a
    // replacement after this point: only read its receipt and verify the fence.
    db.execute_batch("CREATE TRIGGER fail_action BEFORE INSERT ON story_task_action_requests_v1 BEGIN SELECT RAISE(ABORT,'simulated storage fault'); END;").unwrap();
    body["idempotencyKey"] = json!("unknown-proposal");
    let (status, failed) = request(
        &app,
        &route,
        CANDIDATE,
        body,
        Some(("request_candidate_task", "unknown-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{failed}");
    assert_eq!(
        db.query_row(
            "SELECT state FROM story_tool_operations_v1 WHERE call_id='unknown-call'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "accepted"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_action_requests_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    db.execute_batch("DROP TRIGGER fail_action;").unwrap();
    let (status, snapshot) = request(
        &app,
        &format!("{base}/tasks/recovery-snapshot"),
        AUTHOR,
        json!({"taskIds":[task_id]}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    let operations = snapshot["operations"].as_array().unwrap();
    let by_call = |call: &str| {
        operations
            .iter()
            .find(|o| o["tool_call_id"] == call)
            .unwrap()
    };
    assert_eq!(by_call("rejected-call")["state"], "rejected");
    assert_eq!(by_call("rejected-call")["receipt_ref"], Value::Null);
    assert_eq!(by_call("unknown-call")["state"], "accepted");
    assert_eq!(by_call("unknown-call")["receipt_ref"], Value::Null);
    assert_eq!(by_call("corrected-call")["state"], "committed");
    assert_eq!(
        by_call("corrected-call")["receipt_ref"],
        format!("runtime:action:{}", saved["id"].as_str().unwrap())
    );
    // End the model turn and settle its zero-request provider receipt, so the
    // remaining blocker is the unknown tool write rather than an active worker.
    let (status, completed) = request(
        &app,
        &format!("{base}/tasks/{task_id}/complete"),
        CANDIDATE,
        json!({"threadId":"root","summary":"提议保存结果尚未确认，停止执行并等待核对"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    settle_fixture::settle(&path, project, task_id);
    let repo = SqliteStoryRepository::open(&path).unwrap();
    let project_id = ProjectId::parse(project).unwrap();
    let work = repo.work_task_for_attempt(&project_id, task_id).unwrap();
    let watermarks = repo.project_source_watermarks(&project_id).unwrap();
    let (status, decision) = request(&app,&format!("{base}/work-tasks/{}/decision",work.id),AUTHOR,
        json!({"expectedWorkVersion":work.version,"expectedRevision":0,"expectedSourceDigest":watermarks["digest"]}),None).await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    assert_eq!(decision["stopReason"], "pendingRecovery");
    assert_eq!(decision["step"], Value::Null);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_work_steps_v1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_action_requests_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;
