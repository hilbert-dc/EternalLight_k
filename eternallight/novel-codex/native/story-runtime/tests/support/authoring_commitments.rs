use axum::http::StatusCode;
use novel_codex_story_core::{ProjectId, RevisionId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::call;

pub fn plan_refs(path: &std::path::Path, project: &str, previous: i64) -> Value {
    let repository = SqliteStoryRepository::open(path).unwrap();
    let project = ProjectId::parse(project).unwrap();
    let chapter = repository
        .chapter_bindings(&project)
        .unwrap()
        .into_iter()
        .find(|binding| binding.chapter_plan_id == format!("plan-{previous}"))
        .unwrap()
        .chapter_id;
    let report = repository
        .commitment_extraction(&project, &chapter, None)
        .unwrap()
        .unwrap();
    assert_eq!(report["sourceCurrent"], true);
    json!([{"type":"proposal","reportRevisionId":report["revisionId"],
        "proposalId":report["payload"]["candidates"][0]["id"],"sourceRevisionId":report["payload"]["sourceRevisionId"]}])
}

pub enum Recovery {
    Normal,
    ResumeAfterOutput,
}
pub struct Step<'a> {
    pub path: &'a std::path::Path,
    pub project_id: &'a str,
    pub run_uri: &'a str,
    pub task_id: &'a str,
    pub thread_id: &'a str,
    pub target: &'a Value,
    pub revision: i64,
    pub version: i64,
    pub recovery: Recovery,
}

pub async fn submit(step: Step<'_>) -> bool {
    let chapter = step.target["chapterId"].as_str().unwrap();
    let source = step.target["sourceRevisionId"].as_str().unwrap();
    let repository = SqliteStoryRepository::open(step.path).unwrap();
    let content: Value = serde_json::from_str(
        &repository
            .revision_content(&RevisionId::parse(source).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let body = content["body"].as_str().unwrap();
    let length = body.chars().count();
    assert!(length > 2);
    let project = ProjectId::parse(step.project_id).unwrap();
    let previous = repository
        .commitment_extraction(&project, chapter, None)
        .unwrap()
        .map(|report| report["revisionId"].clone());
    drop(repository);
    let uri = format!(
        "/api/projects/{}/chapters/{chapter}/commitment-extractions",
        step.project_id
    );
    let mut request = json!({"taskId":step.task_id,"threadId":step.thread_id,
        "expectedRevision":step.revision,"expectedCandidateRevisionId":previous,"idempotencyKey":format!("report-{}",step.task_id),
        "payload":{"chapterId":chapter,"sourceRevisionId":source,"examinedRanges":[],"candidates":[]}});
    let (status, rejected) = call(
        step.path,
        "POST",
        &uri,
        Some(request.clone()),
        /*candidate*/ true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    request["payload"]["examinedRanges"] = json!([{"start":0,"end":length-1}]);
    let (status, rejected) = call(
        step.path,
        "POST",
        &uri,
        Some(request.clone()),
        /*candidate*/ true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let completion = format!(
        "/api/projects/{}/tasks/{}/complete",
        step.project_id, step.task_id
    );
    let (status, rejected) = call(
        step.path,
        "POST",
        &completion,
        Some(json!({"threadId":step.thread_id,"summary":"nothing saved"})),
        /*candidate*/ true,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    request["payload"]["examinedRanges"] = json!([{"start":0,"end":length}]);
    request["payload"]["candidates"] = json!([{
        "id":"lamp","kind":"foreshadowing","summary":"后续解释灯光线索","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
        "setupEvidenceRefs":[{"objectId":format!("draft-{chapter}"),"revisionId":source,"quote":body.chars().take(2).collect::<String>(),"start":0,"end":2}],
        "fulfillmentCriteria":"后续解释线索","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null
    }]);
    let (status, saved) = call(
        step.path,
        "POST",
        &uri,
        Some(request),
        /*candidate*/ true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let (status, report) = call(
        step.path, "GET", &uri, /*payload*/ None, /*candidate*/ false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["sourceCurrent"], true);
    assert_eq!(report["authority"], "candidateProposalBatch");
    assert_eq!(report["established"], json!([]));
    assert_eq!(report["coverage"]["semanticCompleteness"], "unknown");
    if matches!(step.recovery, Recovery::Normal) {
        return false;
    }
    let (status, completed) = call(
        step.path,
        "POST",
        &completion,
        Some(json!({"threadId":step.thread_id,"summary":"report saved"})),
        /*candidate*/ true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let (status, paused) = call(
        step.path,
        "POST",
        &format!("{}/control", step.run_uri),
        Some(json!({"expectedVersion":step.version,"command":"pause"})),
        /*candidate*/ false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    let charged = super::charge_budget(step.path, step.run_uri, step.task_id).await;
    // Every call constructs a fresh Router/repository; no in-memory completion is reused.
    let (status, resumed) = call(
        step.path,
        "POST",
        &format!("{}/control", step.run_uri),
        Some(json!({"expectedVersion":charged["version"],"command":"resume"})),
        /*candidate*/ false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert_eq!(resumed["stage"], "assess");
    assert_eq!(resumed["usedTaskAttempts"], paused["usedTaskAttempts"]);
    assert_eq!(resumed["lastTaskId"], step.task_id);
    assert!(resumed["activeTaskId"].is_null());
    true
}

use novel_codex_story_core::{ChaptersRepository, CommitmentExtractionRepository, ProjectRevisionRepository};
