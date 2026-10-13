use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use novel_codex_story_core::{
    BranchId, KnowledgePerspective, KnowledgeStateQuery, ProjectId, RelationDirection,
    RelationGraphQuery, RelationGraphScope, SqliteStoryRepository, WriterKnowledgeScope,
};
use novel_codex_story_runtime::entity_audit_tick;
use novel_codex_story_runtime::fact_audit_tick;
use novel_codex_story_runtime::knowledge_audit_tick;
use novel_codex_story_runtime::relation_audit_tick;
use novel_codex_story_runtime::router;
use novel_codex_story_runtime::search_audit_tick;
use novel_codex_story_runtime::snapshot_knowledge_audit_tick;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

const AUTHOR_TOKEN: &str = "author-token-12345678901234567890123";
const CANDIDATE_TOKEN: &str = "candidate-token-12345678901234567890";

fn application(path: &std::path::Path) -> Router {
    router(task_fixture::state(
        path.to_owned(),
        AUTHOR_TOKEN.into(),
        CANDIDATE_TOKEN.into(),
    ))
}

async fn call(
    application: Router,
    method: &str,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    call_as(application, method, uri, payload, AUTHOR_TOKEN, None).await
}

async fn call_as(
    application: Router,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    token: &str,
    capability: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if let Some(capability) = capability {
        builder = builder.header("x-novel-task-capability", capability);
    }
    let body = if let Some(payload) = payload {
        builder = builder.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = application
        .author_fixture_request(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn read_frozen_context_source(
    path: &std::path::Path,
    project_id: &str,
    task: &Value,
    capability: &str,
    context: &Value,
    section: &str,
) -> Value {
    let reference = context.pointer(section).expect("context source reference");
    assert_eq!(reference["textDelivered"], false);
    let source = context["taskSources"]["entries"]
        .as_array().unwrap().iter()
        .find(|entry| entry["sectionPath"] == section).unwrap();
    assert_eq!(reference["sourceId"], source["sourceId"]);
    let mut cursor = None::<String>;
    let mut payload = String::new();
    loop {
        let suffix = cursor.as_ref().map_or(String::new(), |value| format!("&cursor={value}"));
        let (status, page) = call_as(
            application(path), "GET",
            &format!("/api/projects/{project_id}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{suffix}",
                task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap(),
                task["inputRevision"], source["sourceId"].as_str().unwrap()),
            None, CANDIDATE_TOKEN, Some(capability),
        ).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sectionPath"], section);
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert_eq!(page["byteRange"]["start"], payload.len());
        payload.push_str(page["payloadPart"].as_str().unwrap());
        assert_eq!(page["byteRange"]["end"], payload.len());
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(page["byteRange"]["total"], payload.len());
            break;
        }
    }
    assert_eq!(format!("{:x}", Sha256::digest(payload.as_bytes())), source["contentHash"]);
    serde_json::from_str(&payload).unwrap()
}

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settlement;

#[path = "runtime_http/review.rs"]
mod review;

#[path = "runtime_http/knowledge.rs"]
mod knowledge;

#[path = "runtime_http/task.rs"]
mod task;

#[path = "runtime_http/audit.rs"]
mod audit;

#[path = "runtime_http/search.rs"]
mod search;

#[path = "runtime_http/chapter.rs"]
mod chapter;

#[path = "runtime_http/modeling.rs"]
mod modeling;
