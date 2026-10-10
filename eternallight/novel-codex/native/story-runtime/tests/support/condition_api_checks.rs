use std::path::Path;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use novel_codex_story_core::ConditionEvaluationTarget;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{Role, call};

pub(super) async fn verify(path: &Path, base: &str, target: &ConditionEvaluationTarget) {
    let collection = format!("{base}/condition-evaluations");
    let payload = serde_json::to_value(target).unwrap();
    let (status, _) = call(path, "POST", &collection, payload.clone(), Role::Candidate).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let response = router(RuntimeState::new(
        path.to_owned(),
        super::super::AUTHOR.into(),
        super::super::CANDIDATE.into(),
    ))
    .oneshot(
        Request::builder()
            .method("POST")
            .uri(&collection)
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    // Client-supplied bindings and states are never accepted as authority.
    let mut forged = payload.clone();
    forged["bindingDigest"] = json!("client-proof");
    let response = router(RuntimeState::new(
        path.to_owned(),
        super::super::AUTHOR.into(),
        super::super::CANDIDATE.into(),
    ))
    .oneshot(
        Request::builder()
            .method("POST")
            .uri(&collection)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", super::super::AUTHOR))
            .body(Body::from(forged.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let (status, job) = call(path, "POST", &collection, payload.clone(), Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{job}");
    let uri = format!("{collection}/{}", job["id"].as_str().unwrap());
    let control = format!("{uri}/control");
    let (status, same) = call(path, "POST", &collection, payload, Role::Author).await;
    assert_eq!((status, same), (StatusCode::CREATED, job.clone()));
    for (method, url, body) in [
        ("GET", uri.as_str(), Value::Null),
        (
            "POST",
            control.as_str(),
            json!({"command":"cancel","expectedVersion":job["version"]}),
        ),
    ] {
        let (status, _) = call(path, method, url, body, Role::Candidate).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (_, foreign) = call(
        path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"隔离作品"}),
        Role::Author,
    )
    .await;
    let foreign_uri = format!(
        "/api/projects/{}/condition-evaluations/{}",
        foreign["project"]["id"].as_str().unwrap(),
        job["id"].as_str().unwrap()
    );
    let (status, _) = call(path, "GET", &foreign_uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        path,
        "POST",
        &format!("{foreign_uri}/control"),
        json!({"command":"cancel","expectedVersion":job["version"]}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, paused) = call(
        path,
        "POST",
        &control,
        json!({"command":"pause","expectedVersion":job["version"]}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    assert_eq!(paused["status"], "paused");
    assert!(!novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let (status, _) = call(
        path,
        "POST",
        &control,
        json!({"command":"resume","expectedVersion":job["version"]}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, read) = call(path, "GET", &uri, Value::Null, Role::Author).await;
    assert_eq!(read, paused);
    let (status, resumed) = call(
        path,
        "POST",
        &control,
        json!({"command":"resume","expectedVersion":paused["version"]}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert!(novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let (status, running) = call(path, "GET", &uri, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{running}");
    assert_eq!(
        (
            &running["status"],
            &running["scannedSources"],
            &running["result"]
        ),
        (&json!("running"), &json!(1), &Value::Null)
    );
    let (status, cancelled) = call(
        path,
        "POST",
        &control,
        json!({"command":"cancel","expectedVersion":running["version"]}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");
    assert!(!novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let (_, read) = call(path, "GET", &uri, Value::Null, Role::Author).await;
    assert_eq!(read, cancelled);
}
