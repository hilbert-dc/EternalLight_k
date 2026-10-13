//! Real HTTP serialization checks complement the generated TypeScript shape gate.
use axum::{body::Body, http::{Request, StatusCode}};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{RuntimeState, router, contracts::RuntimeErrorEnvelope};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn request(app: axum::Router, method: &str, uri: &str, body: Value, authenticated: bool) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if authenticated { request = request.header("authorization", "Bearer author-contract-token-123456789012345"); }
    let response = app.oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn project_response_and_error_envelopes_survive_the_http_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let state = RuntimeState::new(directory.path().join("wire.sqlite3"), "author-contract-token-123456789012345".into(), "candidate-contract-token-123456789012".into()).unwrap();
    let app = router(state);
    let (status, error) = request(app.clone(), "GET", "/api/projects", Value::Null, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let decoded: RuntimeErrorEnvelope = serde_json::from_value(error.clone()).unwrap();
    assert!(!decoded.error.code.is_empty());
    assert!(decoded.error.recovery.is_none());
    assert_eq!(serde_json::to_value(decoded).unwrap(), error);

    let (status, created) = request(app.clone(), "POST", "/api/projects", json!({"title":"契约边界", "logline":"", "medium":"novel", "idempotencyKey":"wire-create"}), true).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (status, listed) = request(app, "GET", "/api/projects", Value::Null, true).await;
    assert_eq!(status, StatusCode::OK);
    let project = &listed["projects"][0];
    assert_eq!(project["id"], created["project"]["id"]);
    for field in ["id", "title", "logline"] { assert!(project[field].is_string(), "{field}"); }
    for field in ["created_at", "updated_at"] { assert!(project[field].is_i64(), "{field}"); }
    let mut keys = project.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ["created_at", "id", "logline", "title", "updated_at"]);
}

#[test]
fn dynamic_error_details_preserve_null_arrays_and_structured_recovery() {
    let value = json!({"error":{"code":"conflict","message":"changed","details":{"expected":2,"actual":3,"paths":["/title"],"known":false,"unknown":null},"recovery":{"kind":"readCurrent","allowed":true}}});
    let decoded: RuntimeErrorEnvelope = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn custom_budget_ledger_reader_preserves_shape_and_rejects_unknown_fields() {
    use novel_codex_story_core::ProviderBudget;
    let value = json!({"maxRequests":null,"maxTokens":null,"usedRequests":0,"usedTokens":0,"unknownTokens":0,"overrun":false,"charges":[]});
    let budget: ProviderBudget = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(budget).unwrap(), value);
    let mut unknown = value.clone();
    unknown["unrecognized"] = json!(true);
    assert!(serde_json::from_value::<ProviderBudget>(unknown).is_err());
    let mut missing = value;
    missing.as_object_mut().unwrap().remove("maxTokens");
    assert!(serde_json::from_value::<ProviderBudget>(missing).is_err());
}

#[tokio::test]
async fn knowledge_route_preserves_optional_entity_and_predicate_selectors() {
    let directory = tempfile::tempdir().unwrap();
    let state = RuntimeState::new(directory.path().join("selectors.sqlite3"), "author-contract-token-123456789012345".into(), "candidate-contract-token-123456789012".into()).unwrap();
    let app = router(state);
    let (status, created) = request(app.clone(), "POST", "/api/projects", json!({"title":"筛选合同", "logline":"", "medium":"novel", "idempotencyKey":"selectors-create"}), true).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let (status, workspace) = request(app.clone(), "GET", &format!("/api/projects/{project}/creative-workspace"), Value::Null, true).await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    let snapshot = workspace["headSnapshotId"].as_str().unwrap();
    let (status, historical) = request(app.clone(), "GET", &format!("/api/projects/{project}/knowledge/state?perspective=world&snapshotId={snapshot}"), Value::Null, true).await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["snapshotId"], snapshot);
    for (query, selectors) in [
        ("perspective=world", json!({"entityId":null,"predicate":null})),
        ("perspective=world&entityId=letter&predicate=action", json!({"entityId":"letter","predicate":"action"})),
    ] {
        let (status, body) = request(app.clone(), "GET", &format!("/api/projects/{project}/knowledge/state?{query}"), Value::Null, true).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["selectors"], selectors);
    }
}
