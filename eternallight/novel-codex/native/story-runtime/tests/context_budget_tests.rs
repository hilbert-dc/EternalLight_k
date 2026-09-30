use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

const AUTHOR: &str = "budget-author-token-123456789012345678";
const CANDIDATE: &str = "budget-candidate-token-123456789012345";
const CAPABILITY: &str = "budget-capability-12345678901234567890";

#[tokio::test]
async fn context_budget_preserves_target_and_reports_scoped_optional_truncation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("上下文预算.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repository, 4);
    let sources = repository
        .context_sources(&project, ContextSourceScope::ThroughChapter(3))
        .unwrap();
    assert_eq!(sources.total_sources, 3);
    assert!(
        sources
            .sources
            .iter()
            .all(|source| source["status"] == "ready"
                && source.get("entities").is_none()
                && source.get("assertions").is_none())
    );
    add_commitments(&mut repository, &project);
    drop(repository);
    let application = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(
        &application,
        Request::builder()
            .method("POST")
            .uri(format!("/api/projects/{project}/tasks"))
            .header("authorization", format!("Bearer {AUTHOR}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"operation":"generateChapter","inputRevision":0,"threadId":"budget-thread",
            "target":{"chapterId":"chapter-3","chapterNumber":3,"readingChapter":3,"sceneOrder":2},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":expires,"authorIntent":"只续写第三章"})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let uri = format!(
        "/api/projects/{project}/context?taskId={task_id}&threadId=budget-thread&projectRevision=0&purpose=writer&readingChapter=3&sceneOrder=2&budget=24&byteBudget=20000"
    );
    let (status, roomy) = call(
        &application,
        candidate_request(&uri.replace("20000", "60000")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{roomy}");
    assert!(!roomy["relevantCommitments"].as_array().unwrap().is_empty());
    assert!(
        roomy["relevantCommitments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| {
                entry["authority"] == "candidateNarrativeInterpretation"
                    && entry["literaryFulfillmentVerified"] == false
                    && entry.get("authorMetadata").is_none()
            })
    );
    let (status, pack) = call(&application, candidate_request(&uri)).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    let bytes = pack.to_string().len();
    assert!(bytes <= 20_000);
    assert_eq!(
        pack["creativeCapabilityBinding"],
        task["inputManifest"]["creativeCapabilityBinding"]
    );
    assert_eq!(pack["budgetsUsed"]["byteSize"], bytes);
    assert!(
        pack["relevantCommitments"].as_array().unwrap().len()
            <= roomy["relevantCommitments"].as_array().unwrap().len()
    );
    assert_eq!(
        pack["operationContext"]["commitmentCoverage"]["includedEntries"],
        pack["relevantCommitments"].as_array().unwrap().len()
    );
    assert_eq!(
        pack["operationContext"]["commitmentCoverage"]["truncated"],
        true
    );
    assert!(
        pack["coverage"]["truncationReasons"]
            .as_array()
            .unwrap()
            .contains(&json!("contextCommitmentBudget"))
    );
    assert_eq!(pack["coverage"]["complete"], false);
    assert!(
        pack["coverage"]["truncationReasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason
                .as_str()
                .is_some_and(|reason| reason.starts_with("context"))),
        "{}",
        pack["coverage"]
    );
    assert_eq!(
        pack["coverage"]["scannedScope"]["modelingSourcesInScope"],
        3
    );
    assert_eq!(
        pack["coverage"]["scannedScope"]["draftsIncluded"],
        pack["relevantText"].as_array().unwrap().len()
    );
    assert!(
        pack["relevantText"]
            .as_array()
            .unwrap()
            .iter()
            .any(|draft| draft["sourceRevisionId"] == "revision-3")
    );
    assert!(!pack.to_string().contains("revision-4"));
    assert!(!pack.to_string().contains(&fixture::marker(4)));
    let projection = &pack["operationContext"]["workingKnowledge"];
    assert_eq!(projection["metadataTruncated"], true);
    assert_eq!(
        projection["propertyRulesPolicy"],
        "withheldForLimitedPerspective"
    );
    let entries = projection["entries"].as_array().unwrap();
    assert_eq!(projection["coverage"]["candidateEntries"], entries.len());
    let mut hashed = pack.clone();
    hashed.as_object_mut().unwrap().remove("id");
    hashed.as_object_mut().unwrap().remove("contentHash");
    assert_eq!(
        pack["contentHash"],
        format!("{:x}", Sha256::digest(hashed.to_string().as_bytes()))
    );
    let (status, too_small) = call(
        &application,
        candidate_request(&uri.replace("20000", "8000")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{too_small}");
    assert_eq!(too_small["error"]["code"], "context_budget_exceeded");
    let (status, _) = call(
        &application,
        candidate_request(&uri.replace("readingChapter=3", "readingChapter=4")),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

fn add_commitments(repository: &mut SqliteStoryRepository, project: &ProjectId) {
    let source: Value = serde_json::from_str(
        &repository
            .revision_content(&RevisionId::parse("revision-1").unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let quote = source["body"]
        .as_str()
        .unwrap()
        .chars()
        .take(/*n*/ 200)
        .collect::<String>();
    for index in 0..10 {
        let id = format!("budget-{index}");
        let content = json!({"payload":{
            "id":id,"kind":"goal","summary":"继续推进承诺".repeat(/*n*/ 20),
            "relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
            "setupEvidenceRefs":[{"objectId":"draft-chapter-1","revisionId":"revision-1","quote":quote,"start":0,"end":200}],
            "fulfillmentCriteria":"正文呈现具体的后果".repeat(/*n*/ 30),"targetWindow":{"kind":"open"},
            "status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null
        }}).to_string();
        repository
            .save_candidate(SaveCandidateRequest {
                project_id: project.clone(),
                branch_id: BranchId::parse("main").unwrap(),
                object_id: ObjectId::parse(format!("commitment-{id}")).unwrap(),
                object_kind: "commitment".into(),
                revision_id: RevisionId::parse(format!("promise-{id}")).unwrap(),
                commit_id: CommitId::parse(format!("promise-commit-{id}")).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("promise-save-{id}")).unwrap(),
                request_digest: content.clone(),
                content_digest: content.clone(),
                content_json: content,
                expected_project_revision: 0,
                expected_snapshot_id: repository
                    .branch_head(project, &BranchId::parse("main").unwrap())
                    .unwrap()
                    .unwrap()
                    .snapshot_id,
                next_snapshot_id: SnapshotId::parse(format!("promise-snapshot-{id}")).unwrap(),
                expected_candidate_revision_id: None,
                chapter_projection: None,
                task_output: None,
                linked_impact_ids: vec![],
            })
            .unwrap();
    }
}

fn candidate_request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {CANDIDATE}"))
        .header("x-novel-task-capability", CAPABILITY)
        .body(Body::empty())
        .unwrap()
}

async fn call(application: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = application
        .clone()
        .author_fixture_request(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
