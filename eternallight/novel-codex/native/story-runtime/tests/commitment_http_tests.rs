use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

const AUTHOR: &str = "commitment-author-token-123456789012345";
const CANDIDATE: &str = "commitment-candidate-token-12345678901";

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    token: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(payload.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap();
    let response = router(RuntimeState::new(
        path.to_path_buf(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).unwrap())
    .oneshot(request)
    .await
    .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn author_commitment_round_trip_is_versioned_evidenced_and_candidate_isolated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commitments.sqlite3");
    let project = ProjectId::parse("commitment-http").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "港城".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-first").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source-first").unwrap(),
        commit_id: CommitId::parse("save-source").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-source").unwrap(),
        request_digest: "source".into(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("source-snapshot").unwrap(),
        expected_candidate_revision_id: None,
        content_digest: "source".into(),
        content_json: json!({"chapterNumber":1,"body":"白塔的灯将再亮。"}).to_string(),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    drop(repo);
    let uri = "/api/projects/commitment-http/commitments/lamp";
    let input = json!({"expectedRevision":0,"baseSnapshotId":"source-snapshot","expectedCandidateRevisionId":null,"idempotencyKey":"lamp-open",
        "payload":{"id":"lamp","kind":"foreshadowing","summary":"白塔再亮","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
            "setupEvidenceRefs":[{"objectId":"draft-first","revisionId":"source-first","quote":"白塔的灯","start":0,"end":4}],
            "fulfillmentCriteria":"再次亮灯的正文证据","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null}});
    for (method, target, payload) in [
        ("GET", uri, None),
        ("POST", uri, Some(input.clone())),
        (
            "POST",
            "/api/projects/commitment-http/commitments/lamp/reviews",
            Some(json!({"revisionId":"fake","idempotencyKey":"fake"})),
        ),
    ] {
        assert_eq!(
            call(&path, method, target, CANDIDATE, payload).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (status, saved) = call(&path, "POST", uri, AUTHOR, Some(input.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(
        call(&path, "POST", uri, AUTHOR, Some(input.clone())).await,
        (status, saved.clone())
    );
    let (status, read) = call(&path, "GET", uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["payload"], input["payload"]);
    assert_eq!(read["sourceCurrent"], true);
    let (status, review) = call(
        &path,
        "POST",
        &format!("{uri}/reviews"),
        AUTHOR,
        Some(json!({"revisionId":saved["revisionId"],"idempotencyKey":"review-lamp"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(
        &path,
        "POST",
        &format!(
            "/api/projects/commitment-http/reviews/{}/accept",
            review["id"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":0,"idempotencyKey":"accept-lamp","selections":{}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (_, confirmed) = call(&path, "GET", uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(confirmed["confirmationState"], "confirmed");
    let historical_uri = format!(
        "{uri}?snapshotId={}",
        confirmed["snapshotId"].as_str().unwrap()
    );
    let (_, historical_before_undo) =
        call(&path, "GET", &historical_uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(historical_before_undo["authorControls"], Value::Null);
    let mut confirmed_story = confirmed.clone();
    confirmed_story["authorControls"] = Value::Null;
    assert_eq!(historical_before_undo, confirmed_story);
    let (_, undo) = call(
        &path,
        "POST",
        &format!(
            "/api/projects/commitment-http/commits/{}/undo",
            accepted["commitId"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":1,"idempotencyKey":"undo-lamp"})),
    )
    .await;
    assert_eq!(undo["resultRevision"], 2);
    let (_, current) = call(&path, "GET", uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(current["confirmationState"], "candidate");
    let (_, historical) = call(
        &path,
        "GET",
        &format!(
            "{uri}?snapshotId={}",
            confirmed["snapshotId"].as_str().unwrap()
        ),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(historical, historical_before_undo);
    let mut invalid = input;
    invalid["baseSnapshotId"] = current["snapshotId"].clone();
    invalid["expectedRevision"] = json!(2);
    invalid["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    invalid["idempotencyKey"] = json!("invalid-fulfillment");
    invalid["payload"]["status"] = json!("fulfilled");
    let (status, error) = call(&path, "POST", uri, AUTHOR, Some(invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(
        call(&path, "GET", uri, AUTHOR, /*payload*/ None).await.1,
        current
    );
    let mut reopened = SqliteStoryRepository::open(&path).unwrap();
    let catalog_uri = "/api/projects/commitment-http/commitments?limit=1";
    let (catalog_status, catalog) = call(&path, "GET", catalog_uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(catalog_status, StatusCode::OK);
    assert_eq!(
        catalog,
        json!({"snapshotId":current["snapshotId"],"nextCursor":null,"data":[{"id":"lamp","summary":current["payload"]["summary"],"kind":current["payload"]["kind"],"status":"open","revisionId":current["revisionId"],"confirmationState":"candidate","sourceCurrent":true}]})
    );
    assert_eq!(
        call(&path, "GET", catalog_uri, CANDIDATE, /*payload*/ None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &path,
            "GET",
            "/api/projects/commitment-http/commitments?limit=21",
            AUTHOR,
            /*payload*/ None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        reopened
            .narrative_commitment(&project, "lamp", /*snapshot*/ None)
            .unwrap(),
        current
    );
    assert_eq!(reopened.project_revision(&project).unwrap(), 2);
    reopened
        .create_project(
            ProjectRecord {
                project_id: ProjectId::parse("other").unwrap(),
                title: "他作".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("other-snapshot").unwrap(),
        )
        .unwrap();
    drop(reopened);
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{uri}?snapshotId=other-snapshot"),
            AUTHOR,
            /*payload*/ None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{uri}?snapshotId=unknown-snapshot"),
            AUTHOR,
            /*payload*/ None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

use novel_codex_story_core::{NarrativeCommitmentsRepository, ProjectRevisionRepository};
