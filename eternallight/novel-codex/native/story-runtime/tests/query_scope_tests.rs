use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{
    BeginModelingBatch, ObjectId, RevisionId, SqliteStoryRepository, TaskUpdate,
};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "query-scope-author-token-123456789012345";
const CANDIDATE: &str = "query-scope-candidate-token-123456789012";
const CAPABILITY: &str = "query-scope-capability-12345678901234567";

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
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

async fn task(app: &Router, project: &str, operation: &str, target: Value) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status, value) = call(app, "POST", &format!("/api/projects/{project}/tasks"), json!({
        "operation":operation, "inputRevision":0, "threadId":"query-scope-thread", "target":target,
        "authorIntent":"依据正文继续创作", "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())), "capabilityExpiresAt":now+600,
    }), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value["id"].as_str().unwrap().to_owned()
}

fn plan(chapter: usize, perspective: &str) -> Value {
    json!({"question":"人物在港城的正文与状态", "byteBudget":9000, "ranking":{"type":"literal"}, "steps":[
        {"id":"text", "operation":{"type":"text", "query":fixture::marker(chapter), "limit":1}},
        {"id":"name", "operation":{"type":"resolveEntity", "name":"人物倀"}},
        {"id":"state", "operation":{"type":"state", "entityId":"person-0", "predicate":"locatedAt", "perspective":perspective}},
        {"id":"graph", "operation":{"type":"relations", "entityId":"person-0", "relationType":"locatedAt", "direction":"outgoing", "maxHops":1, "limit":1}}
    ]})
}

async fn query(app: &Router, project: &str, id: &str, plan: &Value) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/projects/{project}/tasks/{id}/query"),
        json!({"threadId":"query-scope-thread", "expectedRevision":0, "plan":plan}),
        CANDIDATE,
    )
    .await
}

async fn context(
    app: &Router,
    project: &str,
    id: &str,
    purpose: &str,
    byte_budget: usize,
    plan: &Value,
) -> (StatusCode, Value) {
    let encoded: String = plan
        .to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    call(app, "GET", &format!("/api/projects/{project}/context?taskId={id}&threadId=query-scope-thread&projectRevision=0&purpose={purpose}&byteBudget={byte_budget}&queryPlan={encoded}"), Value::Null, CANDIDATE).await
}

#[tokio::test]
async fn brief_outline_and_project_plan_compose_all_channels_without_author_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project-queries.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repo, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    for operation in ["generateBrief", "generateOutline", "generatePlan"] {
        let id = task(
            &app,
            project.as_str(),
            operation,
            json!({"type":"artifact"}),
        )
        .await;
        let (status, report) = query(&app, project.as_str(), &id, &plan(4, "author")).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        assert_eq!(
            report["policy"],
            json!({"scope":"creativeProject", "operation":operation})
        );
        assert_eq!(
            report["steps"][0]["results"][0]["sourceRevisionId"],
            "revision-4"
        );
        assert!(
            report["steps"]
                .as_array()
                .unwrap()
                .iter()
                .all(|step| !step["results"].as_array().unwrap().is_empty())
        );
        let (status, pack) = context(
            &app,
            project.as_str(),
            &id,
            "planner",
            /*byte_budget*/ 60000,
            &plan(4, "author"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{pack}");
        assert_eq!(pack["queryReport"], report);
        assert_eq!(pack["queryScope"], report["policy"]);
        assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
        assert_eq!(
            pack["sourceWatermarks"]["digest"],
            report["sourceWatermarks"]["digest"]
        );
        assert_eq!(
            repo.branch_head(
                &project,
                &novel_codex_story_core::BranchId::parse("main").unwrap()
            )
            .unwrap()
            .unwrap()
            .snapshot_id,
            snapshot
        );
        assert_eq!(
            repo.task(&project, &id).unwrap().unwrap().output_refs_json,
            "[]"
        );
    }
}

#[tokio::test]
async fn rolling_plan_and_plain_search_share_prior_chapter_scope_including_empty_start() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rolling-queries.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    for from in [1, 3] {
        let id = task(
            &app,
            project.as_str(),
            "generatePlan",
            json!({"type":"artifact", "planFromChapter":from}),
        )
        .await;
        let (status, report) = query(&app, project.as_str(), &id, &plan(1, "world")).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        assert_eq!(report["policy"]["structuredThroughChapter"], from - 1);
        assert_eq!(
            report["policy"]["textScope"],
            json!({"type":"throughChapter", "chapterNumber":from - 1})
        );
        assert!(
            !report.to_string().contains("revision-3")
                && !report.to_string().contains("revision-4")
        );
        if from == 1 {
            assert!(
                report["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|step| step["results"] == json!([]))
            );
        }
        let (status, pack) = context(
            &app,
            project.as_str(),
            &id,
            "planner",
            /*byte_budget*/ 60000,
            &plan(1, "world"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{pack}");
        assert_eq!(pack["queryReport"], report);
        assert_eq!(pack["queryScope"], report["policy"]);
        assert_eq!(
            pack["sourceWatermarks"]["digest"],
            report["sourceWatermarks"]["digest"]
        );
        assert!(
            pack["relevantText"]
                .as_array()
                .unwrap()
                .iter()
                .all(|draft| draft["content"]["chapterNumber"].as_i64().unwrap() < from)
        );
        assert!(
            pack["operationContext"]["workingKnowledge"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["sourceChapter"].as_i64().unwrap() < from)
        );
        let search_uri = format!(
            "/api/projects/{project}/tasks/{id}/search?threadId=query-scope-thread&expectedRevision=0&q={}&kind=draft",
            fixture::marker(4)
        );
        let (status, search) = call(&app, "GET", &search_uri, Value::Null, CANDIDATE).await;
        assert_eq!(status, StatusCode::OK, "{search}");
        assert_eq!(search["results"], json!([]));
        let (status, forbidden) = query(&app, project.as_str(), &id, &plan(1, "author")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{forbidden}");
        repo.update_task(
            &project,
            &id,
            "query-scope-thread",
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 10,
            },
        )
        .unwrap();
        assert_ne!(
            query(&app, project.as_str(), &id, &plan(1, "world"))
                .await
                .0,
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn modeling_query_and_context_bind_exact_source_and_hide_later_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("modeling-queries.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 4);
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-chapter-3").unwrap(),
            source_revision_id: &RevisionId::parse("revision-3").unwrap(),
            expected_revision: 0,
            extractor_version: "query-scope-v1",
            now: 4,
        })
        .unwrap();
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let id = task(
        &app,
        project.as_str(),
        "modelKnowledge",
        json!({"batchId":batch.id}),
    )
    .await;
    let (status, report) = query(&app, project.as_str(), &id, &plan(3, "world")).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        report["policy"]["textScope"],
        json!({"type":"chapter", "chapterNumber":3})
    );
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        "revision-3"
    );
    assert!(!report.to_string().contains("revision-4"));
    let (status, pack) = context(
        &app,
        project.as_str(),
        &id,
        "critic",
        /*byte_budget*/ 60000,
        &plan(3, "world"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{pack}");
    assert_eq!(pack["error"]["code"], "context_model_item_budget_exceeded");
    // Read the full source with a smaller query alongside it, within the same frozen task.
    let mut bounded_plan = plan(3, "world");
    bounded_plan["byteBudget"] = json!(4000);
    bounded_plan["steps"]
        .as_array_mut()
        .unwrap()
        .truncate(/*len*/ 1);
    let (status, bounded_report) = query(&app, project.as_str(), &id, &bounded_plan).await;
    assert_eq!(status, StatusCode::OK, "{bounded_report}");
    let (status, pack) = context(
        &app,
        project.as_str(),
        &id,
        "critic",
        /*byte_budget*/ 60000,
        &bounded_plan,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["queryReport"], bounded_report);
    assert_eq!(
        pack["operationContext"]["modelingBatch"]["sourceRevisionId"],
        "revision-3"
    );
    assert!(
        !pack["operationContext"]["entityCatalog"]
            .to_string()
            .contains("revision-4")
    );
    assert!(pack.to_string().len() <= 60_000);
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
    assert_eq!(pack["relevantText"][0]["sourceRevisionId"], "revision-3");
    assert!(
        !pack["operationContext"]["modeling"]
            .to_string()
            .contains("revision-4")
    );
    let (status, too_small) = context(
        &app,
        project.as_str(),
        &id,
        "critic",
        /*byte_budget*/ 16000,
        &plan(3, "world"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{too_small}");
    for chapter in [1, 4] {
        let (status, empty) = query(&app, project.as_str(), &id, &plan(chapter, "world")).await;
        assert_eq!(status, StatusCode::OK, "{empty}");
        assert_eq!(empty["steps"][0]["results"], json!([]));
    }
    let (status, bad) = call(&app, "POST", &format!("/api/projects/{project}/tasks"), json!({"operation":"unknown", "inputRevision":0,
        "threadId":"query-scope-thread", "target":{"type":"project"}, "authorIntent":"读取正文",
        "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())), "capabilityExpiresAt":i64::MAX}), AUTHOR).await;
    assert_ne!(status, StatusCode::CREATED, "{bad}");
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
