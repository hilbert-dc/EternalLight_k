use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[path = "support/author_task_fixture.rs"]
mod fixture;
#[path = "support/settle_task_fixture.rs"]
mod usage;
use fixture::AuthorTaskFixture;
const AUTHOR: &str = "generation-handoff-author-1234567890";
const CANDIDATE: &str = "generation-handoff-candidate-1234567";
const CAP: &str = "generation-handoff-capability-12345";
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("x-novel-task-capability", CAP)
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
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn setup(path: &std::path::Path) -> (Router, String, String, Value) {
    let app = router(fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        json!({"title":"范围提案验收"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap().to_owned();
    let base = format!("/api/projects/{project}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,task)=call(&app,"POST",&format!("{base}/tasks"),json!({"operation":"advisory","threadId":"original-thread",
        "inputRevision":0,"target":{"type":"project"},"authorIntent":"写两章中文悬疑小说，每章八百字。",
        "capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    (app, base, project, task)
}
async fn propose(app: &Router, base: &str, task: &Value) -> Value {
    let id = task["id"].as_str().unwrap();
    let path = format!("{base}/tasks/{id}/generation-handoff");
    let body = json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,
        "targetChapters":2,"instruction":"从失踪的钟表匠开始，连续写两章候选。","idempotencyKey":"two-chapters"});
    let (status, value) = call(app, "POST", &path, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(
        call(app, "POST", &path, body, CANDIDATE).await,
        (StatusCode::OK, value.clone())
    );
    value["proposal"].clone()
}
fn start_request(proposal: &Value) -> Value {
    json!({"expectedProjectRevision":proposal["expectedRevision"],"expectedBaseSnapshotId":proposal["baseSnapshotId"],
        "expectedSourceDigest":proposal["sourceDigest"],"authorIntent":proposal["authorIntent"],"targetChapters":proposal["targetChapters"],
        "generationProposalId":proposal["proposalId"],"revisionPredecessor":null,"maxTaskAttempts":null,
        "maxProviderRequests":null,"maxProviderTokens":null,"deadlineAt":null})
}

#[tokio::test]
async fn proposal_is_not_permission_and_confirm_consumes_exact_source_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("handoff.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let proposal = propose(&app, &base, &task).await;
    let id = task["id"].as_str().unwrap();
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(workspace["chapters"], json!([]));
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, completed) = call(
        &app,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":"original-thread","summary":"等待作者确认两章范围"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    // Completion alone cannot bypass an unsettled provider receipt.
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    usage::settle(&path, &project, id);
    let mut wrong = start_request(&proposal);
    wrong["targetChapters"] = json!(3);
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            wrong,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, run) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs"),
        start_request(&proposal),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(
        json!({"origin":run["originTaskId"],"thread":run["rootThreadId"],"proposal":run["generationProposalId"]}),
        json!({"origin":id,"thread":"original-thread","proposal":proposal["proposalId"]})
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{base}/tasks/{id}/generation-handoff"),
            Value::Null,
            AUTHOR
        )
        .await,
        (StatusCode::OK, json!({"proposal":null}))
    );
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
}

#[tokio::test]
async fn changed_story_requires_a_new_proposal_even_if_author_supplies_fresh_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stale.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let proposal = propose(&app, &base, &task).await;
    let id = task["id"].as_str().unwrap();
    let (status, value) = call(
        &app,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":"original-thread","summary":"确认范围"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    usage::settle(&path, &project, id);
    let (status, value) = call(
        &app,
        "POST",
        &format!("{base}/artifacts/brief"),
        json!({"expectedRevision":0,"expectedCandidateRevisionId":null,
        "idempotencyKey":"new-direction","payload":{"premise":"作者修改为海上冒险"}}),
        AUTHOR,
    )
    .await;
    assert!(status.is_success(), "{value}");
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let mut request = start_request(&proposal);
    request["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    request["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
    request["expectedProjectRevision"] = workspace["revision"].clone();
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            request,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, runs) = call(
        &app,
        "GET",
        &format!("{base}/generation-runs"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!((status, runs), (StatusCode::OK, json!({"runs":[]})));
}

#[tokio::test]
async fn rejected_proposal_can_be_corrected_without_an_unknown_write_fence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proposal-recovery.sqlite3");
    let (app, base, _, task) = setup(&path).await;
    let id = task["id"].as_str().unwrap();
    for (call_id, intent, expected_status) in [
        ("invalid-intent", 1, StatusCode::BAD_REQUEST),
        ("corrected-intent", 0, StatusCode::OK),
    ] {
        let body = json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":intent,
            "targetChapters":2,"instruction":"按作者要求写两章候选","idempotencyKey":"scope-proposal"});
        let response = app
            .clone()
            .author_fixture_request(
                Request::builder()
                    .method("POST")
                    .uri(format!("{base}/tasks/{id}/generation-handoff"))
                    .header("authorization", format!("Bearer {CANDIDATE}"))
                    .header("content-type", "application/json")
                    .header("x-novel-task-capability", CAP)
                    .header("x-novel-tool-name", "request_generation_handoff")
                    .header("x-novel-root-thread", "original-thread")
                    .header("x-novel-caller-thread", "original-thread")
                    .header("x-novel-turn", "proposal-turn")
                    .header("x-novel-tool-call", call_id)
                    .header(
                        "x-novel-request-hash",
                        format!("{:x}", Sha256::digest(body.to_string())),
                    )
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected_status);
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    let rows=db.prepare("SELECT call_id,state,receipt_ref IS NOT NULL FROM story_tool_operations_v1 ORDER BY call_id").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,bool>(2)?))).unwrap()
        .collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(
        rows,
        vec![
            ("corrected-intent".into(), "committed".into(), true),
            ("invalid-intent".into(), "rejected".into(), false)
        ]
    );
}


#[tokio::test]
async fn hundred_chapter_scope_is_author_bound_and_not_a_three_chapter_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long-scope.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let id = task["id"].as_str().unwrap();
    let uri = format!("{base}/tasks/{id}/generation-handoff");
    let mut body = json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,
        "targetChapters":101,"instruction":"规划长期创作范围，等待作者批准。","idempotencyKey":"long-scope"});
    assert!(!call(&app,"POST",&uri,body.clone(),CANDIDATE).await.0.is_success());
    body["targetChapters"] = json!(100);
    let (status, result) = call(&app,"POST",&uri,body,CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let proposal = &result["proposal"];
    assert!(!call(&app,"POST",&format!("{base}/generation-runs"),start_request(proposal),CANDIDATE).await.0.is_success());
    let (status, reply) = call(&app,"POST",&format!("{base}/tasks/{id}/complete"),json!({"threadId":"original-thread","summary":"仅提议范围，尚未开始写作"}),CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    usage::settle(&path, &project, id);
    let mut changed = start_request(proposal);
    changed["targetChapters"] = json!(99);
    assert!(!call(&app,"POST",&format!("{base}/generation-runs"),changed,AUTHOR).await.0.is_success());
    let (status, run) = call(&app,"POST",&format!("{base}/generation-runs"),start_request(proposal),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["targetChapters"], 100);
    assert_eq!(run["completedChapters"], 0);
    let (_, workspace) = call(&app,"GET",&format!("{base}/creative-workspace"),Value::Null,AUTHOR).await;
    assert_eq!(workspace["chapters"], json!([]), "approving scope must not create placeholder chapters");
}
