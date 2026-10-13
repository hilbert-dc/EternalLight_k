use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_runtime::{RuntimeState, router};
use serde_json::Value;
use tower::ServiceExt;

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "state-author-token-123456789012345678";
const CANDIDATE: &str = "state-candidate-token-123456789012345";

#[tokio::test]
async fn state_selector_http_is_author_only_and_respects_time_and_frozen_scope() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state-selector.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 2);
    drop(repository);
    let application = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    for (scope, expected_count) in [
        (
            "perspective=world&entityId=person-0&predicate=locatedAt&worldTime=1".to_owned(),
            1,
        ),
        (
            format!(
                "perspective=author&entityId=location-2-0&predicate=knows&snapshotId={snapshot}"
            ),
            1,
        ),
        (
            "perspective=reader&entityId=location-2-0&predicate=knows&readingChapter=1".to_owned(),
            0,
        ),
    ] {
        let uri = format!("/api/projects/{project}/knowledge/state?{scope}");
        let response = application
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&uri)
                    .header("authorization", format!("Bearer {AUTHOR}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let state: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status, StatusCode::OK, "{state}");
        assert_eq!(
            state["entries"].as_array().unwrap().len(),
            expected_count,
            "{state}"
        );
        assert_eq!(state["coverage"]["matchingSourcesCount"], expected_count);
        assert_eq!(state["coverage"]["semanticCompleteness"], "unknown");
        let denied = application
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&uri)
                    .header("authorization", format!("Bearer {CANDIDATE}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    }
    let invalid = application
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/projects/{project}/knowledge/state?perspective=author&entityId="
                ))
                .header("authorization", format!("Bearer {AUTHOR}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}
