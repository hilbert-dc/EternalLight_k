use super::*;

#[tokio::test]
async fn draft_checkpoint_creates_chapter_and_is_visible_in_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Draft"})),
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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Working source"})),
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
