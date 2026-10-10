use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{RuntimeState, router};
use serde_json::{Value, json};
use std::path::Path;
use tower::ServiceExt;

const AUTHOR: &str = "conversation-author-123456789012345678901";
const CANDIDATE: &str = "conversation-candidate-123456789012345678";
async fn call(
    path: &Path,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let app = router(RuntimeState::new(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let body = match body {
        Some(value) => {
            request = request.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"error":String::from_utf8_lossy(&bytes)})),
    )
}
async fn project(path: &Path) -> String {
    let (status, body) = call(
        path,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"独立会话"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["project"]["id"].as_str().unwrap().into()
}
fn create(thread: &str) -> Value {
    json!({"threadId":thread,"title":"新对话","forkedFromId":null,"lastTurnId":null})
}

#[tokio::test]
async fn registry_lists_empty_threads_updates_metadata_and_rejects_stale_versions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conversation.sqlite3");
    let project = project(&path).await;
    let base = format!("/api/projects/{project}/conversations");
    for thread in ["thread-a", "thread-b"] {
        let (status, record) = call(&path, "POST", &base, AUTHOR, Some(create(thread))).await;
        assert_eq!(status, StatusCode::OK, "{record}");
        assert_eq!(record["id"], thread);
        assert_eq!(record["projectId"], project);
        assert_eq!(record["version"], 0);
    }
    let (status, list) = call(&path, "GET", &base, AUTHOR, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["conversations"].as_array().unwrap().len(), 2);
    let uri = format!("{base}/thread-a");
    let (status, updated) = call(
        &path,
        "PATCH",
        &uri,
        AUTHOR,
        Some(json!({"expectedVersion":0,"title":"大纲讨论","isPinned":true,"isArchived":true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["version"], 1);
    let (status, _) = call(
        &path,
        "PATCH",
        &uri,
        AUTHOR,
        Some(json!({"expectedVersion":0,"isPinned":false})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, archived) = call(&path, "GET", &format!("{base}?archived=true"), AUTHOR, None).await;
    assert_eq!(archived["conversations"][0]["id"], "thread-a");
    let (_, duplicate) = call(&path, "POST", &base, AUTHOR, Some(create("thread-a"))).await;
    assert_eq!(duplicate, updated);
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("/api/projects/{project}/creative-workspace"),
        AUTHOR,
        None,
    )
    .await;
    assert_eq!(workspace["revision"], 0);
    let repository = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
    assert!(
        repository
            .tasks(&novel_codex_story_core::ProjectId::parse(&project).unwrap())
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn ownership_is_private_and_candidate_cannot_use_registry_routes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conversation.sqlite3");
    let a = project(&path).await;
    let b = project(&path).await;
    let a_base = format!("/api/projects/{a}/conversations");
    let b_base = format!("/api/projects/{b}/conversations");
    assert_eq!(
        call(&path, "POST", &a_base, AUTHOR, Some(create("thread-a")))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&path, "POST", &b_base, AUTHOR, Some(create("thread-a")))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&path, "GET", &format!("{b_base}/thread-a"), AUTHOR, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &path,
            "PATCH",
            &format!("{b_base}/thread-a"),
            AUTHOR,
            Some(json!({"expectedVersion":0,"isPinned":true}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    for (method, uri, body) in [
        ("GET", a_base.clone(), None),
        ("POST", a_base.clone(), Some(create("forged"))),
        (
            "PATCH",
            format!("{a_base}/thread-a"),
            Some(json!({"expectedVersion":0,"title":"forged"})),
        ),
    ] {
        assert_eq!(
            call(&path, method, &uri, CANDIDATE, body).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn registry_http_strictly_rejects_unknown_keys_invalid_ids_and_cross_project_branch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conversation.sqlite3");
    let a = project(&path).await;
    let b = project(&path).await;
    let base = format!("/api/projects/{a}/conversations");
    let mut unknown = create("thread-a");
    unknown["unexpected"] = json!(true);
    assert_eq!(
        call(&path, "POST", &base, AUTHOR, Some(unknown)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(&path, "POST", &base, AUTHOR, Some(create("bad\nthread")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&path, "POST", &base, AUTHOR, Some(create("thread-a")))
            .await
            .0,
        StatusCode::OK
    );
    let branch =
        json!({"threadId":"branch","title":"分支","forkedFromId":"thread-a","lastTurnId":"turn-a"});
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("/api/projects/{b}/conversations"),
            AUTHOR,
            Some(branch.clone())
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&path, "POST", &base, AUTHOR, Some(branch)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &path,
            "PATCH",
            &format!("{base}/thread-a"),
            AUTHOR,
            Some(json!({"expectedVersion":0,"projectId":b,"isPinned":true}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{base}?archived=unexpected"),
            AUTHOR,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{base}?unexpected=true"),
            AUTHOR,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
