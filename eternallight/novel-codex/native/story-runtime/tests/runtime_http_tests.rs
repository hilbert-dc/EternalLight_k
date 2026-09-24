use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use novel_codex_story_runtime::RuntimeState;
use novel_codex_story_runtime::router;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use tower::ServiceExt;

const AUTHOR_TOKEN: &str = "author-token-12345678901234567890123";
const CANDIDATE_TOKEN: &str = "candidate-token-12345678901234567890";

fn application(path: &std::path::Path) -> Router {
    router(RuntimeState::new(
        path.to_owned(),
        AUTHOR_TOKEN.into(),
        CANDIDATE_TOKEN.into(),
    ))
}

async fn call(
    application: Router,
    method: &str,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    call_as(application, method, uri, payload, AUTHOR_TOKEN, None).await
}

async fn call_as(
    application: Router,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    token: &str,
    capability: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if let Some(capability) = capability {
        builder = builder.header("x-novel-task-capability", capability);
    }
    let body = if let Some(payload) = payload {
        builder = builder.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = application
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn candidate_writes_are_task_bound_and_capability_is_revoked_on_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Capability"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let capability = "task-secret-123456789012345678901234567890";
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    let capability_expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let task_uri = format!("/api/projects/{project_id}/tasks");
    let (status, task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "generateBrief",
            "inputRevision": 0,
            "threadId": "thread-capability",
            "target": {},
            "capabilityHash": capability_hash,
            "capabilityExpiresAt": capability_expires_at,
            "authorIntent": "Create a brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let candidate_write = json!({
        "payload": {"premise": "memory"},
        "expectedRevision": 0,
        "expectedCandidateRevisionId": null,
        "idempotencyKey": "candidate-brief-1",
        "taskId": task_id,
        "threadId": "thread-capability"
    });
    let (status, artifact) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_write.clone()),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(artifact["revisionId"].is_string());

    let completion_uri = format!("/api/projects/{project_id}/tasks/{task_id}/complete");
    let (status, completed) = call_as(
        application(&path),
        "POST",
        &completion_uri,
        Some(json!({"threadId": "thread-capability", "summary": "done"})),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["outputRefs"].as_array().unwrap().len(), 1);

    let (status, error) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_write),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error["error"]["code"], "task_capability_invalid");
}

#[tokio::test]
async fn artifact_review_accept_undo_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Round trip", "logline": "Atomic story"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = created["project"]["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let candidate_request = json!({
        "payload": {"premise": "memory", "stakes": "identity"},
        "expectedRevision": 0,
        "expectedCandidateRevisionId": null,
        "idempotencyKey": "save-brief-1",
        "linkedImpactIds": []
    });
    let (status, candidate) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, replay) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_request),
    )
    .await;
    assert_eq!(candidate, replay);

    let revision_id = candidate["revisionId"].as_str().unwrap();
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "brief",
            "revisionId": revision_id,
            "proposals": [],
            "selections": {},
            "idempotencyKey": "review-brief-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review["id"].as_str().unwrap();
    let accept_uri = format!("/api/projects/{project_id}/reviews/{review_id}/accept");
    let (status, accepted) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 0,
            "idempotencyKey": "accept-brief-1",
            "selections": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["resultRevision"], 1);

    let commit_id = accepted["commitId"].as_str().unwrap();
    let undo_uri = format!("/api/projects/{project_id}/commits/{commit_id}/undo");
    let (status, undone) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedRevision": 1,
            "idempotencyKey": "undo-brief-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone["resultRevision"], 2);

    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (status, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(workspace["revision"], 2);
    assert!(workspace["artifacts"]["brief"]["accepted"].is_null());
    assert_eq!(
        workspace["artifacts"]["brief"]["candidate"]["id"],
        revision_id
    );
}

#[tokio::test]
async fn confirmed_fact_is_visible_in_context_and_undo_removes_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Facts"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let (_, candidate) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "The lamp remembers names."},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "fact-candidate-1"
        })),
    )
    .await;
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "brief",
            "revisionId": candidate["revisionId"],
            "proposals": [{
                "subjectId": "white-tower",
                "predicate": "lampDurationSeconds",
                "value": 47,
                "evidence": "The harbor ledger"
            }],
            "selections": {},
            "idempotencyKey": "fact-review-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review["id"].as_str().unwrap();
    let fact_id = review["factProposals"][0]["id"].as_str().unwrap();
    let accept_uri = format!("/api/projects/{project_id}/reviews/{review_id}/accept");
    let (status, accepted) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 0,
            "idempotencyKey": "fact-accept-1",
            "selections": {"factDecisions": {fact_id: "confirmed"}}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let context_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=1&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context) = call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["facts"][0]["id"], fact_id);
    assert_eq!(context["facts"][0]["reviewDecision"], "confirmed");
    assert_eq!(context["facts"][0]["projectRevision"], 1);

    let commit_id = accepted["commitId"].as_str().unwrap();
    let undo_uri = format!("/api/projects/{project_id}/commits/{commit_id}/undo");
    let (status, _) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedRevision": 1,
            "idempotencyKey": "fact-undo-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let context_uri = format!(
        "/api/projects/{project_id}/context?projectRevision=2&purpose=planner&budget=24&byteBudget=60000"
    );
    let (_, context) = call(application(&path), "GET", &context_uri, None).await;
    assert_eq!(context["facts"], json!([]));
}

#[tokio::test]
async fn confirmed_fact_creates_and_resolves_planning_impact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Impacts"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let outline_uri = format!("/api/projects/{project_id}/artifacts/outline");
    let (_, outline) = call(
        application(&path),
        "POST",
        &outline_uri,
        Some(json!({
            "payload": {"nodes": [{
                "id": "volume-1", "kind": "volume", "position": 1,
                "title": "The Tower", "purpose": "Reveal the ledger", "summary": "",
                "dependencyKeys": ["lampDurationSeconds"], "certainty": "settled",
                "protection": "preserve-intent"
            }]},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "impact-outline-1"
        })),
    )
    .await;
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (_, outline_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": outline["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "impact-outline-review-1"
        })),
    )
    .await;
    let outline_accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        outline_review["id"].as_str().unwrap()
    );
    let (_, _) = call(
        application(&path),
        "POST",
        &outline_accept_uri,
        Some(json!({"expectedRevision": 0, "idempotencyKey": "impact-outline-accept-1", "selections": {}})),
    )
    .await;
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (_, draft) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "Opening", "body": "Forty-seven seconds."},
            "expectedRevision": 1, "expectedCandidateRevisionId": null,
            "idempotencyKey": "impact-draft-1"
        })),
    )
    .await;
    let (_, draft_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "draft", "revisionId": draft["revisionId"],
            "proposals": [{"predicate": "lampDurationSeconds", "value": 47, "evidence": "Chapter 1"}],
            "selections": {}, "idempotencyKey": "impact-draft-review-1"
        })),
    )
    .await;
    let fact_id = draft_review["factProposals"][0]["id"].as_str().unwrap();
    let accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        draft_review["id"].as_str().unwrap()
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 1, "idempotencyKey": "impact-draft-accept-1",
            "selections": {"factDecisions": {fact_id: "confirmed"}}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let impacts_uri = format!("/api/projects/{project_id}/impacts?limit=25");
    let (status, impacts) = call(application(&path), "GET", &impacts_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(impacts["counts"]["open"], 1);
    assert_eq!(impacts["data"][0]["targetNodeId"], "volume-1");
    assert_eq!(impacts["data"][0]["severity"], "locked-conflict");
    let original_impact_id = impacts["data"][0]["id"].as_str().unwrap();

    let (_, revised_outline) = call(
        application(&path),
        "POST",
        &outline_uri,
        Some(json!({
            "payload": {"nodes": [{
                "id": "volume-1", "kind": "volume", "position": 1,
                "title": "The Tower", "purpose": "Reveal the ledger", "summary": "The reveal moves later.",
                "dependencyKeys": ["lampDurationSeconds"], "certainty": "settled",
                "protection": "free"
            }]},
            "expectedRevision": 2,
            "expectedCandidateRevisionId": outline["revisionId"],
            "idempotencyKey": "impact-outline-2"
        })),
    )
    .await;
    let (_, revised_review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": revised_outline["revisionId"],
            "proposals": [], "selections": {}, "idempotencyKey": "impact-outline-review-2"
        })),
    )
    .await;
    let revised_accept_uri = format!(
        "/api/projects/{project_id}/reviews/{}/accept",
        revised_review["id"].as_str().unwrap()
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &revised_accept_uri,
        Some(json!({"expectedRevision": 2, "idempotencyKey": "impact-outline-accept-2", "selections": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, impacts) = call(application(&path), "GET", &impacts_uri, None).await;
    assert_eq!(impacts["counts"]["open"], 1);
    assert_eq!(
        impacts["data"][0]["targetRevisionId"],
        revised_outline["revisionId"]
    );
    assert_eq!(impacts["data"][0]["severity"], "affected");
    assert_ne!(impacts["data"][0]["id"], original_impact_id);

    let impact_id = impacts["data"][0]["id"].as_str().unwrap();
    let resolve_uri = format!("/api/projects/{project_id}/impacts/{impact_id}/resolve");
    let (status, resolved) = call(
        application(&path),
        "POST",
        &resolve_uri,
        Some(json!({
            "expectedRevision": 3, "resolution": "kept", "note": "Keep the intended reveal",
            "idempotencyKey": "impact-resolve-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resolved["resultRevision"], 4);
    let (_, impacts) = call(application(&path), "GET", &impacts_uri, None).await;
    assert_eq!(impacts["counts"]["open"], 0);
    assert_eq!(impacts["data"][0]["state"], "resolved-kept");
}

#[tokio::test]
async fn planning_review_seat_uses_frozen_context_and_persists_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Review"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let outline_uri = format!("/api/projects/{project_id}/artifacts/outline");
    let (_, outline) = call(
        application(&path),
        "POST",
        &outline_uri,
        Some(json!({
            "payload": {"nodes": [{
                "id": "book-1", "kind": "book", "position": 1,
                "title": "The Last Lamp", "purpose": "Test memory", "summary": "A city forgets.",
                "dependencyKeys": [], "certainty": "tentative", "protection": "free"
            }]},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "planning-review-outline-1"
        })),
    )
    .await;
    let start_uri = format!("/api/projects/{project_id}/planning-reviews");
    let (status, run) = call(
        application(&path),
        "POST",
        &start_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": outline["revisionId"],
            "targetNodeId": null, "authorIntent": "Check causality",
            "roles": ["structure", "character"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let run_id = run["id"].as_str().unwrap();
    let capability = "review-secret-123456789012345678901234567890";
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let seat_uri =
        format!("/api/projects/{project_id}/planning-reviews/{run_id}/seats/structure/tasks");
    let (status, task) = call(
        application(&path),
        "POST",
        &seat_uri,
        Some(json!({
            "threadId": "thread-structure", "model": "test",
            "capabilityHash": capability_hash, "capabilityExpiresAt": expires_at
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let task_id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=thread-structure&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context) = call_as(
        application(&path),
        "GET",
        &context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["planningReview"]["role"], "structure");
    assert_eq!(
        context["planningReview"]["checks"],
        json!(["structure:book-1"])
    );
    let report_uri = format!("/api/projects/{project_id}/planning-reviews/{run_id}/reports");
    let (status, report) = call_as(
        application(&path),
        "POST",
        &report_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-structure", "expectedRevision": 0,
            "report": {
                "summary": "Causality is visible.", "strengths": [], "findings": [],
                "coverage": [{"checkId": "structure:book-1", "status": "checked", "reason": "Reviewed"}]
            }
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["status"], "complete");
    let complete_uri = format!("/api/projects/{project_id}/tasks/{task_id}/complete");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &complete_uri,
        Some(json!({"threadId": "thread-structure", "summary": "done"})),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let finalize_uri = format!("/api/projects/{project_id}/planning-reviews/{run_id}/finalize");
    let (status, finalized) =
        call(application(&path), "POST", &finalize_uri, Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finalized["status"], "partial");
    assert_eq!(finalized["reports"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn draft_checkpoint_creates_chapter_and_is_visible_in_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Draft"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, draft) = call(
        application(&path),
        "POST",
        &uri,
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "Opening", "body": "The lamp went dark."},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": null,
            "idempotencyKey": "draft-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (_, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(
        workspace["chapters"][0]["candidate"]["id"],
        draft["revisionId"]
    );
    assert_eq!(
        workspace["chapters"][0]["candidate"]["body"],
        "The lamp went dark."
    );
}

#[tokio::test]
async fn standalone_chapter_and_working_source_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"title": "Working source"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let materialize_uri = format!("/api/projects/{project_id}/chapters/materialize-standalone");
    let (status, materialized) = call(
        application(&path),
        "POST",
        &materialize_uri,
        Some(json!({"title": "Opening", "expectedRevision": 0, "idempotencyKey": "standalone-1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{materialized}");
    let chapter_id = materialized["chapterId"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, draft) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterId": chapter_id, "chapterNumber": 1, "title": "Opening", "body": "First version."},
            "expectedRevision": 1, "expectedCandidateRevisionId": null, "idempotencyKey": "working-draft-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let baseline_uri = format!("/api/projects/{project_id}/working-baselines/draft");
    let (status, baseline) = call(
        application(&path),
        "PUT",
        &baseline_uri,
        Some(json!({"objectId": chapter_id, "revisionId": draft["revisionId"], "expectedSelectionRevision": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(baseline["selectionRevision"], 1);
    let continuation_uri = format!("/api/projects/{project_id}/continuations/draft");
    let (status, continuation) = call(
        application(&path),
        "PUT",
        &continuation_uri,
        Some(json!({"targetId": chapter_id, "workingRevisionId": draft["revisionId"], "authorIntent": "Continue with the storm."})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(continuation["status"], "ready");
    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (_, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(
        workspace["chapters"][0]["working"]["body"],
        "First version."
    );
    assert_eq!(
        workspace["continuationIntents"][0]["authorIntent"],
        "Continue with the storm."
    );
}
