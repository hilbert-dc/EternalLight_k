//! Author approval for task fixtures in tests of other Runtime features.
//! Authorization tests deliberately use the raw router, not this fixture transport.
//! Derived claims and explicit budgets are never repaired here.

use std::convert::Infallible;
use std::path::PathBuf;

use axum::{Router, body::Body, http::Request, response::Response};
use http_body_util::BodyExt;
use novel_codex_story_runtime::RuntimeState;
use serde_json::{Value, json};
use tower::ServiceExt;

pub fn state(path: PathBuf, author: String, candidate: String) -> RuntimeState {
    let engine_path = path.with_extension("engine.sqlite3");
    RuntimeState::new(path, author, candidate).with_engine_budget_database(engine_path)
}

/// Builds an explicit finite root authorization from the current Author workspace.
/// The supplied request retains its revision, target and all validation errors.
pub trait AuthorTaskFixture {
    fn author_fixture_request(
        self,
        request: Request<Body>,
    ) -> impl std::future::Future<Output = Result<Response, Infallible>> + Send;
}

impl AuthorTaskFixture for Router {
    async fn author_fixture_request(self, request: Request<Body>) -> Result<Response, Infallible> {
        let segments: Vec<&str> = request.uri().path().split('/').collect();
        if request.method() != "POST"
            || segments.len() != 5
            || segments[1..3] != ["api", "projects"]
            || segments[4] != "tasks"
        {
            return self.oneshot(request).await;
        }
        let workspace_uri = format!("/api/projects/{}/creative-workspace", segments[3]);
        let (parts, body) = request.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let mut payload: Value = serde_json::from_slice(&bytes).unwrap();
        if payload.get("initialBudget").is_none()
            && ![
                "runId",
                "resumeStepId",
                "requestedActionId",
                "requestedByTaskId",
                "sourceTaskId",
                "reviewRunId",
            ]
            .iter()
            .any(|field| payload["target"].get(field).is_some())
        {
            let mut workspace = Request::builder().uri(workspace_uri);
            if let Some(token) = parts.headers.get("authorization") {
                workspace = workspace.header("authorization", token);
            }
            let response = self
                .clone()
                .oneshot(workspace.body(Body::empty()).unwrap())
                .await?;
            // Candidate/anonymous requests stay unauthorized. Never supply an Author token.
            if response.status().is_success() {
                let workspace: Value = serde_json::from_slice(
                    &response.into_body().collect().await.unwrap().to_bytes(),
                )
                .unwrap();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                payload["initialBudget"] = json!({
                    "authorizationId":uuid::Uuid::new_v4().to_string(),
                    "authorMessageId":uuid::Uuid::new_v4().to_string(),
                    "expectedBaseSnapshotId":workspace["headSnapshotId"],
                    "expectedSourceDigest":workspace["sourceDigest"],
                    "maxRequests":16,"maxTokens":200_000,"deadlineAt":now+900,
                });
            }
        }
        self.oneshot(Request::from_parts(parts, Body::from(payload.to_string())))
            .await
    }
}
