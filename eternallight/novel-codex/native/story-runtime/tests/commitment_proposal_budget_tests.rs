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
        "id":format!("proposal-{index}"),"kind":"suspense","summary":"等待后续揭示".repeat(/*n*/ 60),"relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
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
        String::new(),
        None,
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
    assert_eq!(
        roomy["candidateCommitmentProposals"]["textDelivered"],
        false
    );
    let proposals = read_proposals(&application, &project, &task, &roomy).await;
    let entries = proposals.as_array().unwrap();
    assert!(!entries.is_empty());
    assert!(entries.len() < candidates.len());
    assert_eq!(roomy["relevantCommitments"], json!([]));
    assert!(
        entries
            .iter()
            .all(|entry| entry["reportRevisionId"] == "report-1"
                && entry["sourceRevisionId"] == "revision-1"
                && entry["authority"] == "candidateProposal")
    );
    for entry in entries {
        let original = candidates
            .iter()
            .find(|candidate| candidate["id"] == entry["proposalId"])
            .unwrap();
        for field in [
            "kind",
            "summary",
            "fulfillmentCriteria",
            "setupEvidenceRefs",
        ] {
            assert_eq!(
                entry[field], original[field],
                "no partial candidate field: {field}"
            );
        }
        assert_eq!(entry["established"], false);
        assert_eq!(entry["chapterId"], "chapter-1");
        assert_eq!(entry["chapterNumber"], 1);
        assert_eq!(entry["confirmationState"], "unconfirmed");
        assert_eq!(entry["sourceCurrent"], true);
        assert_eq!(entry["literaryFulfillmentVerified"], false);
    }
    let proposal_coverage = &roomy["operationContext"]["commitmentProposalCoverage"];
    let omitted = candidates
        .iter()
        .filter(|candidate| {
            !entries
                .iter()
                .any(|entry| entry["proposalId"] == candidate["id"])
        })
        .collect::<Vec<_>>();
    assert!(!omitted.is_empty());
    assert_eq!(omitted.len(), candidates.len() - entries.len());
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry["proposalId"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        entries.len()
    );
    assert_eq!(proposal_coverage["includedEntries"], entries.len());
    assert_eq!(proposal_coverage["scannedCandidates"], candidates.len());
    assert_eq!(proposal_coverage["truncated"], true);
    assert_eq!(proposal_coverage["oversizedEntriesOmitted"], false);
    assert_eq!(proposal_coverage["byteLimit"], 8_000);
    assert!(proposals.to_string().len() <= 8_000);
    assert!(entries.iter().all(|entry| entry.to_string().len() <= 2_000));
    assert_eq!(proposal_coverage["complete"], false);
    assert!(
        roomy["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("commitmentProposals"))
    );
    assert!(
        roomy["coverage"]["truncationReasons"]
            .as_array()
            .unwrap()
            .contains(&json!("proposalSelectionBudget"))
    );
    assert_context_hash(&roomy);
    // Bound sources are frozen before transport fitting. Tight transport must
    // reject honestly; it cannot silently delete part of the registered source.
    let budget = roomy.to_string().len() - 300;
    assert!(budget >= 4_000);
    let (status, rejected) = call(&application, candidate_request(&format!("{uri}{budget}"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{rejected}");
    assert_eq!(rejected["error"]["code"], "context_budget_exceeded");
    assert!(rejected.get("contentHash").is_none());
    assert_eq!(
        read_proposals(&application, &project, &task, &roomy).await,
        proposals
    );
    let (status, refreshed) = call(&application, candidate_request(&format!("{uri}59999"))).await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    assert_context_hash(&refreshed);
    assert!(refreshed.to_string().len() <= 59_999);
    assert_eq!(refreshed["budgetsUsed"]["byteLimit"], 59_999);
    assert_eq!(
        refreshed["candidateCommitmentProposals"]["sourceId"],
        roomy["candidateCommitmentProposals"]["sourceId"]
    );
    assert_ne!(refreshed["contentHash"], roomy["contentHash"]);
    let (status,rolling) = call(&application,Request::builder().method("POST").uri(format!("/api/projects/{project}/tasks"))
        .header("authorization",format!("Bearer {AUTHOR}")).header("content-type","application/json")
        .body(Body::from(json!({"operation":"generatePlan","inputRevision":0,"threadId":"rolling-thread","target":{"planFromChapter":3},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires,"authorIntent":"在大量前文知识下规划第三章"}).to_string())).unwrap()).await;
    assert_eq!(status, StatusCode::CREATED, "{rolling}");
    let task_id = rolling["id"].as_str().unwrap();
    let (status,pack) = call(&application,candidate_request(&format!("/api/projects/{project}/context?taskId={task_id}&threadId=rolling-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"))).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["candidateCommitmentProposals"]["textDelivered"], false);
    let rolling_proposals = read_proposals(&application, &project, &rolling, &pack).await;
    assert_eq!(rolling_proposals, proposals);
    assert_eq!(
        pack["operationContext"]["commitmentProposalCoverage"]["includedEntries"],
        entries.len()
    );
    assert!(pack.to_string().len() <= 60_000);
    assert_context_hash(&pack);
    assert_ne!(pack["contentHash"], roomy["contentHash"]);
}

fn assert_context_hash(pack: &Value) {
    let mut hashed = pack.clone();
    hashed.as_object_mut().unwrap().remove("id");
    hashed.as_object_mut().unwrap().remove("contentHash");
    assert_eq!(
        pack["contentHash"],
        format!("{:x}", Sha256::digest(hashed.to_string().as_bytes()))
    );
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
}

async fn read_proposals(
    application: &Router,
    project: &ProjectId,
    task: &Value,
    pack: &Value,
) -> Value {
    let source = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["sectionPath"] == "/candidateCommitmentProposals")
        .unwrap();
    assert_eq!(
        source["sourceId"],
        pack["candidateCommitmentProposals"]["sourceId"]
    );
    let uri = format!(
        "/api/projects/{project}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let mut suffix = String::new();
    let mut payload = String::new();
    loop {
        let (status, page) = call(application, candidate_request(&format!("{uri}{suffix}"))).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.to_string().len() <= 6_500);
        assert_eq!(page["sourceId"], source["sourceId"]);
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert_eq!(page["byteRange"]["start"], payload.len());
        payload.push_str(page["payloadPart"].as_str().unwrap());
        assert_eq!(page["byteRange"]["end"], payload.len());
        if let Some(cursor) = page["nextCursor"].as_str() {
            suffix = format!("&cursor={}", cursor.replace(':', "%3A"));
        } else {
            assert_eq!(page["byteRange"]["total"], payload.len());
            assert_eq!(page["endOfInputFromCursor"], true);
            break;
        }
    }
    assert_eq!(source["payloadBytes"], payload.len());
    assert_eq!(
        source["contentHash"],
        format!("{:x}", Sha256::digest(payload.as_bytes()))
    );
    serde_json::from_str(&payload).unwrap()
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

use novel_codex_story_core::{ProjectRevisionRepository, TasksRepository};
