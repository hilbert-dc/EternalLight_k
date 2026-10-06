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

const AUTHOR: &str = "brief-author-token-1234567890123456";
const CANDIDATE: &str = "brief-candidate-token-1234567890123456";
const CAP: &str = "brief-capability-1234567890123456";

async fn call(app: &Router, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("x-novel-task-capability", CAP)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn brief_rejects_chapter_payload_without_outputs_and_keeps_exact_save_cas_and_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("brief.sqlite3");
    let project = ProjectId::parse("brief-contract").unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "潮汐信使".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
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
    let initial = repository
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap();
    drop(repository);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let route = "/api/projects/brief-contract/artifacts/brief";
    let base = json!({"taskId":"task","threadId":"root","expectedRevision":0,
        "expectedCandidateRevisionId":null,"idempotencyKey":"brief-save"});
    let invalid = [
        json!({}),
        json!({"premise":null}),
        json!({"premise":12}),
        json!({"premise":"简报","promise":12}),
        json!({"premise":"简报","stakes":null}),
        json!({"premise":"简报","endingDirection":[]}),
        json!({"chapterNumber":1,"kind":"draft","content":"章节正文"}),
        json!({"premise":"简报","title":"不属于简报"}),
    ];
    for payload in invalid {
        for token in [AUTHOR, CANDIDATE] {
            let mut body = base.clone();
            body["payload"] = payload.clone();
            let (status, error) = call(&app, route, token, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
            assert_eq!(error["error"]["code"], json!("invalid_brief_payload"));
        }
    }
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap(),
        initial
    );
    assert_eq!(
        repository
            .task(&project, "task")
            .unwrap()
            .unwrap()
            .output_refs_json,
        "[]"
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let counts: (i64,i64) = connection.query_row("SELECT (SELECT count(*) FROM story_revisions_v2), (SELECT count(*) FROM story_chapters_v2)", [], |row| Ok((row.get(/*idx*/ 0)?,row.get(/*idx*/ 1)?))).unwrap();
    assert_eq!(counts, (0, 0));
    drop(connection);
    drop(repository);

    let payload = json!({"premise":"潮汐信使","promise":"揭示预言代价","stakes":"","endingDirection":"保留记忆"});
    let mut body = base.clone();
    body["payload"] = payload.clone();
    let (status, saved) = call(&app, route, CANDIDATE, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["payload"], payload);
    let (status, replay) = call(&app, route, CANDIDATE, body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay, saved);
    let mut next = base.clone();
    next["idempotencyKey"] = json!("brief-next");
    next["payload"] = json!({"premise":"","promise":" \n\t"});
    let (status, _) = call(&app, route, CANDIDATE, next.clone()).await;
    assert!(!status.is_success());
    next["expectedCandidateRevisionId"] = json!("null");
    let (status, _) = call(&app, route, CANDIDATE, next.clone()).await;
    assert!(!status.is_success());
    next["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    let (status, revised) = call(&app, route, CANDIDATE, next).await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    assert_eq!(revised["payload"], json!({"premise":"","promise":" \n\t"}));
    let (status, completed) = call(
        &app,
        "/api/projects/brief-contract/tasks/task/complete",
        CANDIDATE,
        json!({"threadId":"root","summary":"简报已保存，未创建章节"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["status"], json!("completed"));
    let connection = rusqlite::Connection::open(&path).unwrap();
    let counts: (i64,i64) = connection.query_row("SELECT (SELECT count(*) FROM story_revisions_v2), (SELECT count(*) FROM story_chapters_v2)", [], |row| Ok((row.get(/*idx*/ 0)?,row.get(/*idx*/ 1)?))).unwrap();
    assert_eq!(counts, (2, 0));
}

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
