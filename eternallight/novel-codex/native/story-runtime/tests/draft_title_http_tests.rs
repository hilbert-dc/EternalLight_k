use std::path::Path;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

const AUTHOR: &str = "title-author-123456789012345678901234";
const CANDIDATE: &str = "title-candidate-12345678901234567890";

// Reconstruct the router on each request: titles must survive persisted revision reads.
async fn call(path: &Path, method: &str, uri: &str, payload: Option<Value>) -> (StatusCode, Value) {
    let application = router(RuntimeState::new(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ).unwrap());
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {AUTHOR}"));
    let body = if let Some(payload) = payload {
        request = request.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = application
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn formal_candidate_and_working_titles_follow_their_own_persisted_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("titles.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "标题归属"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let checkpoints = format!("{base}/drafts/checkpoints");
    let (status, formal) = call(&path, "POST", &checkpoints, Some(json!({
        "payload": {"chapterId": "chapter-title", "chapterNumber": 1, "title": "正式雾港 A", "body": "正式正文。",
            "sceneGroups": [{"id": "scene-a", "sceneId": "stable-a", "body": "正式正文。", "locked": false}]},
        "expectedRevision": 0, "expectedCandidateRevisionId": null, "idempotencyKey": "title-formal"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{formal}");
    let chapter_id = formal["chapterId"].as_str().unwrap();
    let workspace_uri = format!("{base}/creative-workspace");
    let (status, before_review) = call(&path, "GET", &workspace_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{before_review}");
    assert_eq!(
        before_review["chapters"][0]["candidate"]["title"],
        "正式雾港 A"
    );
    assert_eq!(before_review["chapters"][0]["accepted"], Value::Null);
    assert_eq!(before_review["reviews"], json!([]));

    // Explicit whole-chapter adoption sets up the formal version. Acceptance semantics are unchanged.
    let (status, review) = call(&path, "POST", &format!("{base}/reviews"), Some(json!({
        "artifactKind": "draft", "revisionId": formal["revisionId"], "proposals": [], "selections": {},
        "idempotencyKey": "title-review"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let review_id = review["id"].as_str().unwrap();
    let (status, receipt) = call(&path, "POST", &format!("{base}/reviews/{review_id}/accept"), Some(json!({
        "expectedRevision": 0, "idempotencyKey": "title-adopt", "selections": {"sceneGroupIds": ["scene-a"]}
    }))).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let (status, adopted) = call(&path, "GET", &workspace_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{adopted}");
    let accepted = adopted["chapters"][0]["accepted"].clone();
    assert_eq!(accepted["title"], "正式雾港 A");

    let (status, candidate) = call(&path, "POST", &checkpoints, Some(json!({
        "payload": {"chapterId": chapter_id, "chapterNumber": 1, "title": "工作雾港 B", "body": "候选正文。",
            "sceneGroups": [{"id": "scene-b", "sceneId": "stable-a", "body": "候选正文。", "locked": false}]},
        "expectedRevision": receipt["resultRevision"], "expectedCandidateRevisionId": formal["revisionId"],
        "idempotencyKey": "title-candidate"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{candidate}");
    let baseline_uri = format!("{base}/working-baselines/draft");
    for (selection_revision, revision, title) in [
        (0, &formal["revisionId"], "正式雾港 A"),
        (1, &candidate["revisionId"], "工作雾港 B"),
    ] {
        let (status, baseline) = call(&path, "PUT", &baseline_uri, Some(json!({
            "objectId": chapter_id, "revisionId": revision, "expectedSelectionRevision": selection_revision
        }))).await;
        assert_eq!(status, StatusCode::OK, "{baseline}");
        let (status, workspace) = call(&path, "GET", &workspace_uri, None).await;
        assert_eq!(status, StatusCode::OK, "{workspace}");
        let chapter = &workspace["chapters"][0];
        assert_eq!(chapter["accepted"], accepted);
        assert_eq!(
            json!({
                "candidate": {"id": chapter["candidate"]["id"], "title": chapter["candidate"]["title"]},
                "working": {"id": chapter["working"]["id"], "title": chapter["working"]["title"]}
            }),
            json!({
                "candidate": {"id": candidate["revisionId"], "title": "工作雾港 B"},
                "working": {"id": revision, "title": title}
            })
        );
    }
}

#[tokio::test]
async fn draft_projection_rejects_invalid_current_revision_title_instead_of_inventing_one() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corrupt-title.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "标题完整性"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let (status, draft) = call(&path, "POST", &format!("{base}/drafts/checkpoints"), Some(json!({
        "payload": {"chapterNumber": 1, "title": "真实标题", "body": "正文。"},
        "expectedRevision": 0, "expectedCandidateRevisionId": null, "idempotencyKey": "title-corruption"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let connection = rusqlite::Connection::open(&path).unwrap();
    let revision = draft["revisionId"].as_str().unwrap();
    let content: String = connection
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
            [revision],
            |row| row.get(0),
        )
        .unwrap();
    let mut envelope: Value = serde_json::from_str(&content).unwrap();
    for invalid_title in [Value::Null, json!(42)] {
        envelope["title"] = invalid_title;
        connection
            .execute(
                "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id=?2",
                rusqlite::params![envelope.to_string(), revision],
            )
            .unwrap();
        let (status, response) =
            call(&path, "GET", &format!("{base}/creative-workspace"), None).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{response}");
    }
    connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id=?2",
            rusqlite::params![content, revision],
        )
        .unwrap();
    let (status, restored) = call(&path, "GET", &format!("{base}/creative-workspace"), None).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["chapters"][0]["candidate"]["title"], "真实标题");
}

#[tokio::test]
async fn empty_boundary_scene_groups_preserve_identity_and_reject_mismatched_prose() {
    for bodies in [["", "后续正文。"], ["原有正文。", ""], ["", ""]] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty-scenes.sqlite3");
        let (code, created) = call(&path, "POST", "/api/projects", Some(json!({
            "logline":"", "medium":"novel", "title":"空场景续写",
            "idempotencyKey":uuid::Uuid::new_v4().to_string()}))).await;
        assert_eq!(code, StatusCode::CREATED, "{created}");
        let base = format!("/api/projects/{}", created["project"]["id"].as_str().unwrap());
        let groups = json!([
            {"id":"first", "sceneId":null, "body":bodies[0], "locked":true},
            {"id":"second", "sceneId":null, "body":bodies[1], "locked":false}]);
        let body = bodies.join("\n\n---\n\n");
        let mut request = json!({"expectedRevision":0,"expectedCandidateRevisionId":null,
            "idempotencyKey":"empty-scenes", "payload":{"chapterId":"chapter-empty",
                "chapterNumber":1,"title":"空场景", "body":body,"sceneGroups":groups}});
        let mut mismatch = request.clone();
        mismatch["payload"]["body"] = json!("未经场景组记录的正文");
        let (code, _) = call(&path,"POST", &format!("{base}/drafts/checkpoints"),Some(mismatch)).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        let (code, saved) = call(&path,"POST", &format!("{base}/drafts/checkpoints"),Some(request.take())).await;
        assert_eq!(code, StatusCode::CREATED,"{saved}");
        let (_, workspace) = call(&path,"GET", &format!("{base}/creative-workspace"),None).await;
        assert_eq!(workspace["chapters"][0]["candidate"]["sceneGroups"],groups);
        assert_eq!(workspace["chapters"][0]["candidate"]["body"],body);
    }
}
