use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const AUTHOR: &str = "renew-author-token-12345678901234567890";
const CANDIDATE: &str = "renew-candidate-token-1234567890123456";

async fn call(path: &std::path::Path, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    )
    .oneshot(
        Request::builder()
            .method(if body.is_null() { "GET" } else { "POST" })
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
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
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn only_author_host_can_renew_the_exact_live_task_without_creating_an_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renew.sqlite3");
    let (_, project) = call(&path, AUTHOR, "/api/projects", json!({"title":"续期测试"})).await;
    let id = project["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{id}");
    let (_, workspace) = call(
        &path,
        AUTHOR,
        &format!("{base}/creative-workspace"),
        Value::Null,
    )
    .await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let hash = format!(
        "{:x}",
        Sha256::digest(b"renew-live-secret-12345678901234567890")
    );
    let expires = now + 120;
    let (status, task) = call(&path, AUTHOR, &format!("{base}/tasks"), json!({
        "threadId":"renew-thread","operation":"advisory","inputRevision":0,
        "model":"test-model","target":{"type":"project"},"authorIntent":"分析小说",
        "capabilityHash":hash,"capabilityExpiresAt":expires,
        "initialBudget":{"authorizationId":"renew-http-authority","authorMessageId":"renew-http-message",
            "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
            "maxRequests":null,"maxTokens":null,"deadlineAt":null}
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let history_endpoint = format!("{base}/threads/renew-thread/author-messages");
    let history_payload = json!({"turnIds":["renew-turn"]});
    assert_eq!(
        call(&path, CANDIDATE, &history_endpoint, history_payload.clone())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, _) = call(
        &path,
        AUTHOR,
        &format!("{base}/tasks/{task_id}/turn"),
        json!({"threadId":"renew-thread","turnId":"renew-turn"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        call(&path, AUTHOR, &history_endpoint, history_payload).await,
        (
            StatusCode::OK,
            json!({"threadId":"renew-thread","messages":[{"turnId":"renew-turn","text":"分析小说"}]})
        )
    );
    let endpoint = format!("{base}/tasks/{task_id}/capability/renew");
    let payload = json!({"threadId":"renew-thread","operation":"advisory","model":"test-model",
        "target":{"type":"project"},"capabilityHash":hash,"expectedExpiresAt":expires});
    assert_eq!(
        call(&path, CANDIDATE, &endpoint, payload.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (_, before) = call(
        &path,
        AUTHOR,
        &format!("{base}/tasks/{task_id}/provider-budget"),
        Value::Null,
    )
    .await;
    let (status, renewed) = call(&path, AUTHOR, &endpoint, payload.clone()).await;
    assert_eq!(status, StatusCode::OK, "{renewed}");
    assert_eq!(renewed["taskId"], task_id);
    assert!(renewed["expiresAt"].as_u64().unwrap() > expires);
    assert!(renewed["expiresAt"].as_u64().unwrap() <= now + 1802);
    assert_eq!(
        call(
            &path,
            AUTHOR,
            &format!("{base}/tasks/{task_id}/provider-budget"),
            Value::Null
        )
        .await
        .1,
        before
    );
    assert!(!call(&path, AUTHOR, &endpoint, payload).await.0.is_success());
    let (_, read) = call(
        &path,
        AUTHOR,
        &format!("{base}/tasks/{task_id}"),
        Value::Null,
    )
    .await;
    let mut expected = task;
    expected["turnId"] = json!("renew-turn");
    assert_eq!(read, expected);
}
