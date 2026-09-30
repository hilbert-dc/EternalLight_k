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
