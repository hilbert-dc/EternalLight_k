use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{router, RuntimeState};
use serde_json::{json, Value};
use tower::ServiceExt;
const AUTHOR: &str = "context-author-123456789012345678901";
const CANDIDATE: &str = "context-candidate-123456789012345678";
async fn call(
    app: &Router,
    method: &str,
    url: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(url)
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
async fn canonical_facts_require_authority_and_explicit_adoption_preserves_the_prior_revision() {
    let dir = tempfile::tempdir().unwrap();
    let app = router(RuntimeState::new(
        dir.path().join("media.sqlite"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).unwrap());
    let (status, p) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"原创动画","logline":"","medium":"animation","idempotencyKey":"create"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{p}");
    let pid = p["project"]["id"].as_str().unwrap();
    let eid = p["activeEditionId"].as_str().unwrap();
    let canon_url = format!("/api/projects/{pid}/canon");
    let workspace_url = format!("/api/projects/{pid}/editions/{eid}/workspace");
    let (_, empty) = call(&app, "GET", &canon_url, AUTHOR, Value::Null).await;
    let request = json!({"expectedRevisionId":null,"idempotencyKey":"canon-save","document":{"title":"共同设定","entries":[{"id":"light-rule","category":"rule","statement":"灯不会自行熄灭。","sources":[]}]}});
    assert_eq!(
        call(&app, "POST", &canon_url, CANDIDATE, request.clone())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "GET", &canon_url, CANDIDATE, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, canon) = call(&app, "POST", &canon_url, AUTHOR, request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{canon}");
    assert_eq!(
        call(&app, "POST", &canon_url, AUTHOR, request).await.1["revisionId"],
        canon["revisionId"]
    );
    let (_, w) = call(&app, "GET", &workspace_url, AUTHOR, Value::Null).await;
    assert_eq!(w["edition"]["canonSnapshotId"], empty["snapshotId"]);
    let(status,saved)=call(&app,"POST",&workspace_url,AUTHOR,json!({"expectedRevisionId":null,"idempotencyKey":"document-save","document":w["document"],"readSet":[],"sources":[]})).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let(status,adopted)=call(&app,"POST",&format!("/api/projects/{pid}/editions/{eid}/patch"),AUTHOR,json!({"expectedRevisionId":saved["revisionId"],"idempotencyKey":"adopt","operations":[{"kind":"adoptCanonSnapshot","canonSnapshotId":canon["snapshotId"]}],"readSet":[],"sources":[]})).await;
    assert_eq!(status, StatusCode::OK, "{adopted}");
    assert_eq!(adopted["edition"]["canonSnapshotId"], canon["snapshotId"]);
    assert_ne!(adopted["revisionId"], saved["revisionId"]);
    assert_eq!(adopted["documentEncoding"], saved["documentEncoding"]);
    let (status, old) = call(
        &app,
        "GET",
        &format!(
            "{workspace_url}?revisionId={}",
            saved["revisionId"].as_str().unwrap()
        ),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{old}");
    assert_eq!(old["edition"]["canonSnapshotId"], empty["snapshotId"]);
    let (status, report) = call(
        &app,
        "GET",
        &format!("/api/projects/{pid}/editions/{eid}/source-impacts"),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["canon"]["changed"], false);
    let (status, error) = call(
        &app,
        "POST",
        &canon_url,
        AUTHOR,
        json!({"expectedRevisionId":null,"idempotencyKey":"stale","document":canon["document"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
}
