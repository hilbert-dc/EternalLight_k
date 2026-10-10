use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "writer-query-author-token-123456789012345";
const CANDIDATE: &str = "writer-query-candidate-token-123456789012";
const CAPABILITY: &str = "writer-query-capability-12345678901234567";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    payload: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

enum Perspective {
    Narrator,
    Character,
    Unspecified,
}

async fn writer(app: &Router, project: &str, perspective: Perspective) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut target = json!({"chapterId":"chapter-3", "chapterNumber":3, "readingChapter":3,
        "sceneOrder":1, "scope":{"type":"throughScene","endSceneOrder":1},"readScope":{"type":"throughScene","endSceneOrder":1}, "storyTime":1});
    if matches!(perspective, Perspective::Character) {
        target["povCharacterId"] = json!("person-0");
    } else if matches!(perspective, Perspective::Unspecified) {
        target["povCharacterId"] = Value::Null;
        target["storyTime"] = Value::Null;
    }
    let (status, value) = call(app, "POST", &format!("/api/projects/{project}/tasks"), json!({
        "operation":"generateChapter", "inputRevision":0, "threadId":"writer-query-thread", "target":target,
        "authorIntent":"续写白塔", "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())), "capabilityExpiresAt":now+600,
    }), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn writer_query_and_context_accept_explicitly_unspecified_plan_position() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("writer-unspecified.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let task = writer(&app, project.as_str(), Perspective::Unspecified).await;
    let uri = format!("/api/projects/{project}/tasks/{task}/query");
    let (status, report) = call(
        &app,
        "POST",
        &uri,
        request(vec![
            json!({"type":"text", "query":fixture::marker(3), "limit":1}),
        ]),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["policy"]["povCharacterId"], Value::Null);
    assert_eq!(report["policy"]["storyTime"], Value::Null);
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        "revision-3"
    );
    assert!(!report.to_string().contains("revision-4"));
    let context_uri = format!(
        "/api/projects/{project}/context?taskId={task}&threadId=writer-query-thread&projectRevision=0&purpose=writer&readingChapter=3&sceneOrder=1&budget=24&byteBudget=60000"
    );
    let (status, pack) = call(&app, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["taskId"], task);
}

fn request(operations: Vec<Value>) -> Value {
    json!({"threadId":"writer-query-thread", "expectedRevision":0, "plan":{"question":"白塔前文与当前场景",
        "byteBudget":9000, "ranking":{"type":"literal"}, "steps":operations.into_iter().enumerate().map(|(index, operation)|
            json!({"id":format!("step-{index}"), "operation":operation})).collect::<Vec<_>>()}})
}

#[tokio::test]
async fn writer_combines_all_channels_and_context_without_future_or_scope_override() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("writer-query.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let task = writer(&app, project.as_str(), Perspective::Narrator).await;
    let uri = format!("/api/projects/{project}/tasks/{task}/query");
    let body = request(vec![
        json!({"type":"text", "query":fixture::marker(3), "limit":1}),
        json!({"type":"resolveEntity", "name":"人物倀"}),
        json!({"type":"state", "entityId":"person-0", "perspective":"world", "predicate":"locatedAt"}),
        json!({"type":"relations", "entityId":"person-0", "direction":"outgoing", "relationType":"locatedAt", "maxHops":1, "limit":1}),
    ]);
    let (status, report) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["policy"]["structuredThroughChapter"], 2);
    assert_eq!(report["policy"]["endSceneOrder"], 1);
    assert_eq!(report["policy"]["identityPolicy"], "rawSourceClaims");
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        "revision-3"
    );
    for index in 1..=3 {
        assert!(
            !report["steps"][index]["results"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{report}"
        );
        for row in report["steps"][index]["results"].as_array().unwrap() {
            assert!([json!("revision-1"), json!("revision-2")].contains(&row["sourceRevisionId"]));
            assert_eq!(row["confirmationState"], "candidate");
        }
    }
    assert!(!report.to_string().contains("revision-4"));
    assert!(report.to_string().len() <= 9_000);
    let encoded: String = body["plan"]
        .to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    let context_uri = format!(
        "/api/projects/{project}/context?taskId={task}&threadId=writer-query-thread&projectRevision=0&purpose=writer&readingChapter=3&sceneOrder=1&storyTime=1&byteBudget=60000&queryPlan={encoded}"
    );
    let (status, pack) = call(&app, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["queryReport"], report);
    assert_eq!(
        pack["queryReport"]["sourceWatermarks"]["digest"],
        pack["sourceWatermarks"]["digest"]
    );
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
    let future = request(vec![
        json!({"type":"text", "query":fixture::marker(4), "limit":1}),
    ]);
    let (status, empty) = call(&app, "POST", &uri, future, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["steps"][0]["results"], json!([]));
    let target = repo
        .revision_content(&novel_codex_story_core::RevisionId::parse("revision-3").unwrap())
        .unwrap()
        .unwrap();
    let target: Value = serde_json::from_str(&target).unwrap();
    let hidden = target["sceneGroups"][1]["body"]
        .as_str()
        .unwrap()
        .chars()
        .take(8)
        .collect::<String>();
    let (status, empty) = call(
        &app,
        "POST",
        &uri,
        request(vec![json!({"type":"text", "query":hidden, "limit":1})]),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["steps"][0]["results"], json!([]));
    for operation in [
        json!({"type":"state", "entityId":"person-0", "perspective":"author"}),
        json!({"type":"state", "entityId":"person-0", "perspective":"reader", "readingChapter":4}),
        json!({"type":"state", "entityId":"person-0", "perspective":"world", "worldTime":4}),
        json!({"type":"relations", "entityId":"person-0", "direction":"outgoing", "maxHops":1, "limit":1, "worldTime":4}),
    ] {
        assert_eq!(
            call(&app, "POST", &uri, request(vec![operation]), CANDIDATE)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn pov_plan_inherits_owner_and_time_and_withholds_unverified_prose_and_names() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pov-query.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let task = writer(&app, project.as_str(), Perspective::Character).await;
    let uri = format!("/api/projects/{project}/tasks/{task}/query");
    let body = request(vec![
        json!({"type":"text", "query":fixture::marker(1), "limit":1}),
        json!({"type":"resolveEntity", "name":"人物倀"}),
        json!({"type":"state", "entityId":"location-1-0", "perspective":"character"}),
        json!({"type":"relations", "entityId":"person-0", "direction":"outgoing", "relationType":"locatedAt", "maxHops":1, "limit":1}),
    ]);
    let (status, report) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["steps"][0]["results"], json!([]));
    assert_eq!(report["steps"][0]["coverage"]["povTextUnverified"], true);
    assert_eq!(report["steps"][1]["results"], json!([]));
    assert_eq!(report["steps"][1]["coverage"]["complete"], false);
    assert_eq!(
        report["steps"][2]["results"][0]["assertion"]["epistemicOwnerId"],
        "person-0"
    );
    assert_eq!(
        report["steps"][2]["results"][0]["sourceRevisionId"],
        "revision-1"
    );
    assert_eq!(
        report["steps"][3]["results"][0]["sourceRevisionId"],
        "revision-1"
    );
    for operation in [
        json!({"type":"state", "entityId":"person-0", "perspective":"world"}),
        json!({"type":"state", "entityId":"location-1-0", "perspective":"character", "ownerId":"person-1"}),
        json!({"type":"state", "entityId":"location-1-0", "perspective":"character", "epistemicTime":4}),
    ] {
        assert_eq!(
            call(&app, "POST", &uri, request(vec![operation]), CANDIDATE)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    let future = request(vec![
        json!({"type":"state", "entityId":"location-4-0", "perspective":"character"}),
    ]);
    let (_, empty) = call(&app, "POST", &uri, future, CANDIDATE).await;
    assert_eq!(empty["steps"][0]["results"], json!([]));
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
