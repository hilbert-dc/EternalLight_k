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

const AUTHOR: &str = "proposal-author-token-123456789012345";
const CANDIDATE: &str = "proposal-candidate-token-123456789012";
const CAPABILITY: &str = "proposal-capability-12345678901234567";

#[tokio::test]
async fn planning_pack_trims_whole_proposals_and_rehashes_accurate_coverage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("规划.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 2);
    let source: Value = serde_json::from_str(
        &repo
            .revision_content(&RevisionId::parse("revision-1").unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let quote = source["body"]
        .as_str()
        .unwrap()
        .chars()
        .take(/*n*/ 4)
        .collect::<String>();
    let candidates = (0..8).map(|index|json!({
        "id":format!("proposal-{index}"),"kind":"suspense","summary":"等待后续揭示".repeat(/*n*/ 30),"relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
        "setupEvidenceRefs":[{"objectId":"draft-chapter-1","revisionId":"revision-1","quote":quote,"start":0,"end":4}],
        "fulfillmentCriteria":"后文提供原文证据","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null
    })).collect::<Vec<_>>();
    let content = json!({"payload":{"chapterId":"chapter-1","sourceRevisionId":"revision-1","examinedRanges":[{"start":0,"end":4}],"candidates":candidates}}).to_string();
    let bound = repo
        .create_task(CreateTaskRequest {
            task_id: "extract-report".into(),
            project_id: project.clone(),
            thread_id: "extract-thread".into(),
            operation: "extractCommitments".into(),
            input_revision: 0,
            model: None,
            target_json:
                json!({"chapterId":"chapter-1","chapterNumber":1,"sourceRevisionId":"revision-1"})
                    .to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "b".repeat(/*n*/ 64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("commitment-extraction-chapter-1").unwrap(),
        object_kind: "commitmentExtraction".into(),
        revision_id: RevisionId::parse("report-1").unwrap(),
        commit_id: CommitId::parse("report-commit").unwrap(),
        idempotency_key: IdempotencyKey::parse("report-key").unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: 0,
        expected_snapshot_id: repo
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse("report-snapshot").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: None,
        task_output: Some(TaskOutputBinding {
            task_id: bound.task_id.clone(),
            thread_id: bound.thread_id.clone(),
            output_kind: "commitmentExtraction".into(),
            completeness: None,
        }),
        linked_impact_ids: vec![],
    })
    .unwrap();
    repo.complete_task(
        &project,
        &bound.task_id,
        &bound.thread_id,
        String::new(), None,
        /*now*/ 10,
    )
    .unwrap();
    drop(repo);
    let application = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status,task) = call(&application,Request::builder().method("POST").uri(format!("/api/projects/{project}/tasks"))
        .header("authorization",format!("Bearer {AUTHOR}")).header("content-type","application/json")
        .body(Body::from(json!({"operation":"generateOutline","inputRevision":0,"threadId":"planning-thread","target":{},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires,"authorIntent":"规划下一章"}).to_string())).unwrap()).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let uri = format!(
        "/api/projects/{project}/context?taskId={task_id}&threadId=planning-thread&projectRevision=0&purpose=planner&budget=24&byteBudget="
    );
    let (status, roomy) = call(&application, candidate_request(&format!("{uri}60000"))).await;
    assert_eq!(status, StatusCode::OK, "{roomy}");
    let entries = roomy["candidateCommitmentProposals"].as_array().unwrap();
    assert!(!entries.is_empty());
    assert_eq!(roomy["relevantCommitments"], json!([]));
    assert!(
        entries
            .iter()
            .all(|entry| entry["reportRevisionId"] == "report-1"
                && entry["sourceRevisionId"] == "revision-1"
                && entry["authority"] == "candidateProposal")
    );
    let budget = roomy.to_string().len() - 300;
    let (status, trimmed) = call(&application, candidate_request(&format!("{uri}{budget}"))).await;
    assert_eq!(status, StatusCode::OK, "{trimmed}");
    let selected = trimmed["candidateCommitmentProposals"].as_array().unwrap();
    assert!(selected.len() < entries.len());
    assert!(trimmed.to_string().len() <= budget);
    assert_eq!(
        trimmed["operationContext"]["commitmentProposalCoverage"]["includedEntries"],
        selected.len()
    );
    assert_eq!(
        trimmed["operationContext"]["commitmentProposalCoverage"]["complete"],
        false
    );
    assert!(
        trimmed["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("commitmentProposals"))
    );
    assert!(
        trimmed["coverage"]["truncationReasons"]
            .as_array()
            .unwrap()
            .contains(&json!("contextProposalBudget"))
    );
    let mut hashed = trimmed.clone();
    hashed.as_object_mut().unwrap().remove("id");
    hashed.as_object_mut().unwrap().remove("contentHash");
    assert_eq!(
        trimmed["contentHash"],
        format!("{:x}", Sha256::digest(hashed.to_string().as_bytes()))
    );
    assert_ne!(trimmed["contentHash"], roomy["contentHash"]);
    let (status,rolling) = call(&application,Request::builder().method("POST").uri(format!("/api/projects/{project}/tasks"))
        .header("authorization",format!("Bearer {AUTHOR}")).header("content-type","application/json")
        .body(Body::from(json!({"operation":"generatePlan","inputRevision":0,"threadId":"rolling-thread","target":{"planFromChapter":3},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires,"authorIntent":"在大量前文知识下规划第三章"}).to_string())).unwrap()).await;
    assert_eq!(status, StatusCode::CREATED, "{rolling}");
    let task_id = rolling["id"].as_str().unwrap();
    let (status,pack) = call(&application,candidate_request(&format!("/api/projects/{project}/context?taskId={task_id}&threadId=rolling-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"))).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(
        pack["candidateCommitmentProposals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        pack["candidateCommitmentProposals"][0]["reportRevisionId"],
        "report-1"
    );
    assert_eq!(
        pack["operationContext"]["commitmentProposalCoverage"]["includedEntries"],
        1
    );
    assert!(pack.to_string().len() <= 60_000);
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
