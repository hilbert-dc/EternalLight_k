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

const AUTHOR: &str = "revision-author-token-1234567890123456";
const CANDIDATE: &str = "revision-candidate-token-123456789012";
const CAPABILITY: &str = "revision-capability-token-1234567890";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {actor}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn conversation_revision_roundtrip_keeps_author_approval_separate() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("承诺修订.sqlite3");
    let project = ProjectId::parse("revision-http").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "灯".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch.clone(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: branch.clone(),
        object_id: ObjectId::parse("draft-one").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source").unwrap(),
        commit_id: CommitId::parse("source-save").unwrap(),
        idempotency_key: IdempotencyKey::parse("source-save").unwrap(),
        request_digest: "source".into(),
        content_digest: "source".into(),
        content_json: json!({"chapterNumber":1,"body":"灯将熄灭。灯火归来。"}).to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("source-snapshot").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    drop(repo);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let resource = "/api/projects/revision-http/commitments/lamp";
    let mut payload = json!({"id":"lamp","kind":"suspense","summary":"灯能否归来","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
        "setupEvidenceRefs":[{"objectId":"draft-one","revisionId":"source","quote":"灯将熄灭","start":0,"end":4}],
        "fulfillmentCriteria":"正文明确灯火归来","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null});
    let (status,base)=call(&app,"POST",resource,AUTHOR,Some(json!({"payload":payload,"extractionOrigin":null,"expectedRevision":0,"baseSnapshotId":"source-snapshot","expectedCandidateRevisionId":null,"idempotencyKey":"manual"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{base}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let mut create = json!({"operation":"advisory","inputRevision":0,"threadId":"discussion","target":{"type":"project"},"authorIntent":"把灯火承诺改为已经兑现的候选",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, discussion) = call(
        &app,
        "POST",
        "/api/projects/revision-http/tasks",
        AUTHOR,
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{discussion}");
    let root = discussion["id"].as_str().unwrap();
    let input =
        format!("{resource}/task-input?taskId={root}&threadId=discussion&expectedRevision=0");
    let (status, read) = call(&app, "GET", &input, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(
        read,
        json!({"revisionId":base["revisionId"],"payload":payload})
    );
    let (status,action)=call(&app,"POST",&format!("/api/projects/revision-http/tasks/{root}/actions"),CANDIDATE,
        Some(json!({"taskId":root,"threadId":"discussion","expectedRevision":0,"expectedIntentRevision":0,"operation":"reviseCommitment","idempotencyKey":"adjust",
            "target":{"commitmentId":"lamp","revisionId":read["revisionId"]},"instruction":"按原文灯火归来标记为已兑现的候选"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/projects/revision-http/tasks/{root}/complete"),
            AUTHOR,
            Some(json!({"threadId":"discussion","summary":"已登记"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    settle_fixture::settle(&path, "revision-http", root);
    let (status, approved) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/revision-http/tasks/{root}/actions/{}/approve",
            action["id"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedIntentRevision":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    create["operation"] = json!("reviseCommitment");
    create["threadId"] = json!("revision");
    create["target"] = action["target"].clone();
    create["target"]["requestedByTaskId"] = json!(root);
    create["target"]["requestedActionId"] = action["id"].clone();
    let (status, task) = call(
        &app,
        "POST",
        "/api/projects/revision-http/tasks",
        AUTHOR,
        Some(create),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "/api/projects/revision-http/context?taskId={id}&threadId=revision&projectRevision=0&purpose=critic&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&app, "GET", &context_uri, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["queryScope"],
        json!({"scope":"creativeProject","operation":"reviseCommitment"})
    );
    assert_eq!(
        context["target"]["operation"]["commitmentId"],
        json!("lamp")
    );
    let query = format!("taskId={id}&threadId=revision&expectedRevision=0");
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/projects/revision-http/commitments/other/task-input?{query}"),
            CANDIDATE,
            /*body*/ None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, "GET", resource, CANDIDATE, /*body*/ None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    payload["status"] = json!("fulfilled");
    payload["fulfillmentEvidenceRefs"] = json!([{"objectId":"draft-one","revisionId":"source","quote":"灯火归来","start":5,"end":9}]);
    let submit = json!({"taskId":id,"threadId":"revision","expectedRevision":0,"payload":payload,"expectedCandidateRevisionId":base["revisionId"],"idempotencyKey":"generated"});
    let route = format!("{resource}/task-candidate");
    assert_eq!(
        call(&app, "POST", &route, AUTHOR, Some(submit.clone()))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let mut forged = submit.clone();
    forged["payload"]["fulfillmentEvidenceRefs"][0]["quote"] = json!("伪造证据");
    assert_eq!(
        call(&app, "POST", &route, CANDIDATE, Some(forged)).await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, saved) = call(&app, "POST", &route, CANDIDATE, Some(submit.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(
        call(&app, "POST", &route, CANDIDATE, Some(submit)).await.1,
        saved
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{resource}/task-input?{query}"),
            CANDIDATE,
            /*body*/ None
        )
        .await
        .1,
        read
    );
    let (_, current) = call(&app, "GET", resource, AUTHOR, /*body*/ None).await;
    assert_eq!(current["confirmationState"], json!("candidate"));
    assert_eq!(current["revisionId"], saved["revisionId"]);
    let (status, completed) = call(
        &app,
        "POST",
        &format!("/api/projects/revision-http/tasks/{id}/complete"),
        CANDIDATE,
        Some(json!({"threadId":"revision","expectedRevision":0,"summary":"候选已保存"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["deliveryStatus"], json!("ready_for_review"));
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;

use novel_codex_story_core::{ProjectRevisionRepository};
