use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{SqliteStoryRepository, TaskUpdate};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "semantic-author-token-123456789012345";
const CANDIDATE: &str = "semantic-candidate-token-123456789012";
const CAPABILITY: &str = "semantic-capability-12345678901234567";

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
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"frameworkRejection":String::from_utf8_lossy(&bytes)}));
    (status, value)
}

async fn task(app: &Router, project: &str, operation: &str, target: Value) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status,value) = call(app,"POST",&format!("/api/projects/{project}/tasks"),json!({"operation":operation,
        "inputRevision":0,"threadId":"semantic-thread","target":target,"authorIntent":"根据含义寻找证据",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value["id"].as_str().unwrap().to_owned()
}

fn request() -> Value {
    json!({"threadId":"semantic-thread","expectedRevision":0,"plan":{"question":"哪章最相关？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"text","operation":{"type":"text","query":"港城","limit":3}}]}})
}

fn ranked(mut request: Value, report: &Value) -> Value {
    let rows = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|step| step["results"].as_array().unwrap())
        .collect::<Vec<_>>();
    request["plan"]["ranking"] = json!({"type":"modelSemantic","baseReportHash":report["contentHash"],
        "judgments":rows.iter().enumerate().map(|(index,row)|json!({"resultId":row["resultId"],"score":index*100,
            "rationale":"工程指定的相关性候选；不代表事实已确认"})).collect::<Vec<_>>()});
    request
}

fn encoded(plan: &Value) -> String {
    plan.to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect()
}

#[tokio::test]
async fn semantic_judgments_reorder_real_evidence_and_fuse_into_the_same_context_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("语义查询.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 3);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let id = task(
        &app,
        project.as_str(),
        "advisory",
        json!({"type":"project"}),
    )
    .await;
    let uri = format!("/api/projects/{project}/tasks/{id}/query");
    let body = request();
    let (status, baseline) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{baseline}");
    let rows = baseline["steps"][0]["results"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    let candidate = ranked(body, &baseline);
    let (status, report) = call(&app, "POST", &uri, candidate.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        report["steps"][0]["results"][0]["resultId"],
        rows[2]["resultId"]
    );
    assert_eq!(
        report["steps"][0]["results"][0]["literalRelevanceScore"],
        rows[2]["relevanceScore"]
    );
    assert_eq!(report["evidence"], baseline["evidence"]);
    assert_eq!(report["sourceWatermarks"], baseline["sourceWatermarks"]);
    assert_eq!(report["policy"], baseline["policy"]);
    assert_eq!(report["coverage"], baseline["coverage"]);
    assert_eq!(report["ranking"]["authority"], "modelCandidate");
    assert_eq!(report["ranking"]["qualityVerified"], false);
    assert_eq!(report["ranking"]["baseReportHash"], baseline["contentHash"]);
    assert!(report.to_string().len() <= 9000);
    let mut hash_input = report.clone();
    hash_input["contentHash"] = json!("");
    assert_eq!(
        report["contentHash"],
        format!("{:x}", Sha256::digest(hash_input.to_string().as_bytes()))
    );
    let context_uri = format!(
        "/api/projects/{project}/context?taskId={id}&threadId=semantic-thread&projectRevision=0&purpose=planner&byteBudget=60000&queryPlan={}",
        encoded(&candidate["plan"])
    );
    let (status, pack) = call(&app, "GET", &context_uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["queryReport"], report);
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
    assert!(pack.to_string().len() <= 60_000);
    assert_eq!(
        call(&app, "POST", &uri, candidate, CANDIDATE).await,
        (StatusCode::OK, report)
    );
}

#[tokio::test]
async fn stale_forged_or_incomplete_semantic_judgments_never_expand_the_query() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("语义围栏.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 3);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let id = task(
        &app,
        project.as_str(),
        "advisory",
        json!({"type":"project"}),
    )
    .await;
    let uri = format!("/api/projects/{project}/tasks/{id}/query");
    let body = request();
    let (_, baseline) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    let candidate = ranked(body.clone(), &baseline);
    let mut missing_mode = body;
    missing_mode["plan"]
        .as_object_mut()
        .unwrap()
        .remove("ranking");
    assert_eq!(
        call(&app, "POST", &uri, missing_mode, CANDIDATE).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    enum Failure {
        Missing,
        Duplicate,
        Unknown,
        Rationale,
        Score,
        Question,
        Hash,
    }
    for failure in [
        Failure::Missing,
        Failure::Duplicate,
        Failure::Unknown,
        Failure::Rationale,
        Failure::Score,
        Failure::Question,
        Failure::Hash,
    ] {
        let mut invalid = candidate.clone();
        match failure {
            Failure::Missing => {
                invalid["plan"]["ranking"]["judgments"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            Failure::Duplicate => {
                invalid["plan"]["ranking"]["judgments"][0]["resultId"] =
                    invalid["plan"]["ranking"]["judgments"][1]["resultId"].clone();
            }
            Failure::Unknown => {
                invalid["plan"]["ranking"]["judgments"][0]["resultId"] = json!("a".repeat(64))
            }
            Failure::Rationale => {
                invalid["plan"]["ranking"]["judgments"][0]["rationale"] = json!("界".repeat(86))
            }
            Failure::Score => invalid["plan"]["ranking"]["judgments"][0]["score"] = json!(1001),
            Failure::Question => invalid["plan"]["question"] = json!("替换原问题"),
            Failure::Hash => invalid["plan"]["ranking"]["baseReportHash"] = json!("b".repeat(64)),
        }
        assert_ne!(
            call(&app, "POST", &uri, invalid, CANDIDATE).await.0,
            StatusCode::OK
        );
    }
    let other = task(
        &app,
        project.as_str(),
        "advisory",
        json!({"type":"project"}),
    )
    .await;
    assert_ne!(
        call(
            &app,
            "POST",
            &format!("/api/projects/{project}/tasks/{other}/query"),
            candidate.clone(),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    let generation = repo.search_index_generation(&project).unwrap();
    repo.rebuild_search_index(&project, generation).unwrap();
    assert_ne!(
        call(&app, "POST", &uri, candidate.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::OK
    );
    repo.update_task(
        &project,
        &id,
        "semantic-thread",
        TaskUpdate {
            status: "cancelled".into(),
            error: None,
            result_summary: None,
            now: 10,
        },
    )
    .unwrap();
    assert_ne!(
        call(&app, "POST", &uri, candidate, CANDIDATE).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn writer_and_pov_cannot_rank_evidence_hidden_by_their_bound_scope() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("语义视角.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let root = task(
        &app,
        project.as_str(),
        "advisory",
        json!({"type":"project"}),
    )
    .await;
    let body = request();
    let (_, global) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/tasks/{root}/query"),
        body.clone(),
        CANDIDATE,
    )
    .await;
    for pov in [false, true] {
        let mut target = json!({"chapterId":"chapter-2","chapterNumber":2,"readingChapter":2,"sceneOrder":1,"storyTime":2});
        if pov {
            target["povCharacterId"] = json!("person-0");
        }
        let id = task(&app, project.as_str(), "generateChapter", target).await;
        let uri = format!("/api/projects/{project}/tasks/{id}/query");
        let (_, scoped) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
        assert!(
            scoped["steps"][0]["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["sourceRevisionId"] != "revision-3"
                    && row["sourceRevisionId"] != "revision-4")
        );
        let forged = ranked(body.clone(), &global);
        assert_ne!(
            call(&app, "POST", &uri, forged, CANDIDATE).await.0,
            StatusCode::OK
        );
        if pov {
            assert_eq!(scoped["steps"][0]["results"], json!([]));
            assert_eq!(scoped["coverage"]["complete"], false);
        } else {
            let (_, ranked_report) =
                call(&app, "POST", &uri, ranked(body.clone(), &scoped), CANDIDATE).await;
            assert_eq!(ranked_report["policy"], scoped["policy"]);
            assert_eq!(ranked_report["coverage"], scoped["coverage"]);
        }
    }
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

use novel_codex_story_core::{SearchRebuildRepository, TasksRepository};
