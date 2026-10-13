use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::SqliteStoryRepository;
use novel_codex_story_runtime::{RuntimeState, router};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "benchmark-author-token-123456789012345";
const CANDIDATE: &str = "benchmark-candidate-token-123456789012";
const CAPABILITY: &str = "benchmark-context-capability-12345678";
const CHAPTERS: usize = 512;
const SAMPLES: usize = 30;

#[tokio::main]
async fn main() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("上下文基准.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let start = Instant::now();
    let (project, _) = fixture::build(&mut repository, CHAPTERS);
    let build_ms = start.elapsed().as_secs_f64() * 1000.0;
    drop(repository);
    let application = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let mut reports = serde_json::Map::new();
    for (name, pov) in [("writer", None), ("writerPov", Some("person-0"))] {
        let thread = format!("benchmark-{name}");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let task = call(&application, Request::builder().method("POST")
            .uri(format!("/api/projects/{project}/tasks"))
            .header("authorization", format!("Bearer {AUTHOR}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({"operation":"generateChapter","inputRevision":0,"threadId":thread,
                "target":{"chapterId":format!("chapter-{CHAPTERS}"),"chapterNumber":CHAPTERS,
                    "readingChapter":CHAPTERS,"sceneOrder":2,"storyTime":CHAPTERS,"povCharacterId":pov},
                "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt":now + 3600,"authorIntent":"继续目标章节"}).to_string())).unwrap(), StatusCode::CREATED).await;
        let task_id = task["id"].as_str().unwrap();
        let mut uri = format!(
            "/api/projects/{project}/context?taskId={task_id}&threadId={thread}&projectRevision=0&purpose=writer&readingChapter={CHAPTERS}&sceneOrder=2&storyTime={CHAPTERS}&budget=24&byteBudget=60000"
        );
        if let Some(pov) = pov {
            uri.push_str(&format!("&povCharacterId={pov}"));
        }
        let mut samples = Vec::new();
        let mut first_ms = 0.0;
        let mut pack_bytes = 0;
        let mut baseline = None;
        for sample in 0..=SAMPLES {
            let start = Instant::now();
            let pack = call(
                &application,
                Request::builder()
                    .uri(&uri)
                    .header("authorization", format!("Bearer {CANDIDATE}"))
                    .header("x-novel-task-capability", CAPABILITY)
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::OK,
            )
            .await;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if sample == 0 {
                first_ms = elapsed;
            } else {
                samples.push(elapsed);
            }
            pack_bytes = pack.to_string().len();
            assert!(pack_bytes <= 60_000);
            assert_eq!(pack["budgetsUsed"]["byteSize"], pack_bytes);
            assert_eq!(pack["taskId"], task_id);
            assert_eq!(pack["coverage"]["complete"], false);
            assert_eq!(
                pack["coverage"]["scannedScope"]["modelingSourcesInScope"],
                CHAPTERS
            );
            assert!(
                pack["relevantText"].as_array().unwrap().iter().any(
                    |draft| draft["sourceRevisionId"] == format!("revision-{CHAPTERS}")
                        && draft["content"]["body"]
                            .as_str()
                            .unwrap()
                            .contains(&fixture::marker(CHAPTERS))
                )
            );
            assert!(
                pack["operationContext"]["modeling"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|source| source.get("assertions").is_none()
                        && source.get("entities").is_none())
            );
            if let Some(hash) = baseline.as_ref() {
                assert_eq!(&pack["contentHash"], hash);
            } else {
                baseline = Some(pack["contentHash"].clone());
            }
        }
        samples.sort_by(f64::total_cmp);
        reports.insert(name.into(), json!({"firstQueryMs":first_ms,"warmP50Ms":samples[SAMPLES / 2],
            "warmP95Ms":samples[(SAMPLES * 95).div_ceil(100) - 1],"warmMaxMs":samples[SAMPLES - 1],"packBytes":pack_bytes}));
    }
    println!(
        "{}",
        json!({"datasetVersion":"long-work-literal-v1","chapters":CHAPTERS,
        "buildMs":build_ms,"profile":"release","platform":std::env::consts::OS,
        "architecture":std::env::consts::ARCH,"samplesPerQuery":SAMPLES,"queries":reports,
        "limitations":["in-process public HTTP router; no TCP or model latency", "bounded knowledge coverage, not semantic completeness", "no Windows or literary gate"]})
    );
}

async fn call(application: &Router, request: Request<Body>, expected: StatusCode) -> Value {
    let response = application.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, expected, "{value}");
    value
}
