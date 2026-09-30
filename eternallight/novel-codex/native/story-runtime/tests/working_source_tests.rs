use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "working-source-author-token-1234567890";
const CANDIDATE: &str = "working-source-candidate-token-123456";
const CAPABILITY: &str = "working-source-capability-123456789";

#[tokio::test]
async fn selected_historical_draft_is_frozen_read_and_advanced_by_writer_checkpoints() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("working-source.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"港城"})),
        false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let drafts = format!("{root}/drafts/checkpoints");
    let (status, original) = call(
        &path,
        "POST",
        &drafts,
        Some(json!({
            "payload":{"chapterNumber":1,"title":"白塔","body":"林夕藏起密函。"},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"original"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{original}");
    let chapter = original["chapterId"].as_str().unwrap();
    let (status, newer) = call(&path,"POST",&drafts,Some(json!({
        "payload":{"chapterId":chapter,"chapterNumber":1,"title":"白塔","body":"林夕烧掉密函。"},
        "expectedRevision":0,"expectedCandidateRevisionId":original["revisionId"],"idempotencyKey":"newer"
    })),false).await;
    assert_eq!(status, StatusCode::CREATED, "{newer}");
    let baseline_uri = format!("{root}/working-baselines/draft");
    let (status, selected) = call(
        &path,
        "PUT",
        &baseline_uri,
        Some(json!({
            "objectId":chapter,"revisionId":original["revisionId"],"expectedSelectionRevision":0
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    assert_eq!(selected["selectionRevision"], 1);
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path,"POST",&format!("{root}/tasks"),Some(json!({
        "operation":"generateChapter","inputRevision":0,"threadId":"selected-writer",
        "target":{"type":"chapter","chapterId":chapter,"chapterNumber":1,"readingChapter":1,"sceneOrder":1},
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"authorIntent":"从藏信稿继续"
    })),false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "{root}/context?taskId={task_id}&threadId=selected-writer&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["relevantText"][0]["sourceRevisionId"],
        original["revisionId"]
    );
    assert_eq!(
        context["relevantText"][0]["content"]["body"],
        "林夕藏起密函。"
    );
    assert_eq!(context["relevantText"][0]["authority"], "working");
    assert_eq!(context["relevantText"][0]["workingSelectionRevision"], 1);
    assert_eq!(
        context["operationContext"]["modeling"][0]["sourceRevisionId"],
        original["revisionId"]
    );
    assert_eq!(
        context["operationContext"]["modeling"][0]["status"],
        "model_pending"
    );
    assert_eq!(
        context["operationContext"]["workingKnowledge"]["coverage"]["sourceModelingIncomplete"],
        true
    );
    let query_uri = format!("{root}/tasks/{task_id}/query");
    let (status, report) = call(
        &path,
        "POST",
        &query_uri,
        Some(json!({
            "threadId":"selected-writer","expectedRevision":0,
            "plan":{
                "question":"林夕对密函做了什么？","byteBudget":9000,
                "ranking":{"type":"literal"},
                "steps":[
                    {"id":"passage","operation":{"type":"passages","cursor":null,"maxChars":160,"limit":2}},
                    {"id":"text","operation":{"type":"text","query":"藏起密函","limit":2}}
                ]
            }
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        original["revisionId"]
    );
    assert_eq!(
        report["steps"][1]["results"][0]["sourceRevisionId"],
        original["revisionId"]
    );
    let (status, unmodeled) = call(
        &path,
        "POST",
        &query_uri,
        Some(json!({
            "threadId":"selected-writer","expectedRevision":0,
            "plan":{
                "question":"林夕当前知道什么？","byteBudget":9000,
                "ranking":{"type":"literal"},
                "steps":[{"id":"state","operation":{"type":"state","entityId":"linxi","perspective":"world"}}]
            }
        })),
        true,
    )
    .await;
    assert!(status.is_client_error(), "{unmodeled}");
    let before_digest = context["sourceWatermarks"]["digest"].clone();
    let save = |body: &str, previous: Value, key: &str| {
        json!({
            "payload":{"chapterId":chapter,"chapterNumber":1,"title":"白塔","body":body},
            "expectedRevision":0,"expectedCandidateRevisionId":previous,"idempotencyKey":key,
            "taskId":task_id,"threadId":"selected-writer"
        })
    };
    let (status, first) = call(
        &path,
        "POST",
        &drafts,
        Some(save(
            "林夕把密函藏进灯座。",
            newer["revisionId"].clone(),
            "writer-first",
        )),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let database = rusqlite::Connection::open(&path).unwrap();
    let first_base: String = database.query_row(
        "SELECT working_base_revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND ordinal=1",
        [task_id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(first_base, original["revisionId"].as_str().unwrap());
    let refs: String = database
        .query_row(
            "SELECT output_refs_json FROM story_tasks_v2 WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&refs).unwrap()[0]["workingBaseRevisionId"],
        original["revisionId"]
    );
    let (status, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    assert_eq!(
        workspace["chapters"][0]["workingSelection"]["revisionId"],
        first["revisionId"]
    );
    assert_eq!(
        workspace["chapters"][0]["workingSelection"]["selectionRevision"],
        2
    );
    let (status, refreshed) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    assert_eq!(
        refreshed["relevantText"][0]["sourceRevisionId"],
        first["revisionId"]
    );
    assert_ne!(refreshed["sourceWatermarks"]["digest"], before_digest);
    let (status, second) = call(
        &path,
        "POST",
        &drafts,
        Some(save(
            "林夕记下灯座的刻痕。",
            first["revisionId"].clone(),
            "writer-second",
        )),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    let second_base: String = database.query_row(
        "SELECT working_base_revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND ordinal=2",
        [task_id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(second_base, first["revisionId"].as_str().unwrap());
    let (status, switch) = call(
        &path,
        "PUT",
        &baseline_uri,
        Some(json!({
            "objectId":chapter,"revisionId":newer["revisionId"],"expectedSelectionRevision":3
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{switch}");
    let (status, stale) = call(&path, "GET", &context_uri, None, true).await;
    assert!(status.is_client_error(), "{stale}");
    let (status, late) = call(
        &path,
        "POST",
        &drafts,
        Some(save(
            "不应写入的迟到正文。",
            second["revisionId"].clone(),
            "writer-late",
        )),
        true,
    )
    .await;
    assert!(status.is_client_error(), "{late}");
    let (_, after) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(
        after["chapters"][0]["candidate"]["id"],
        second["revisionId"]
    );
    assert_eq!(
        after["chapters"][0]["workingSelection"]["revisionId"],
        newer["revisionId"]
    );
    assert_eq!(
        after["chapters"][0]["workingSelection"]["selectionRevision"],
        4
    );
}

#[tokio::test]
async fn planner_uses_selected_artifact_and_a_changed_selection_invalidates_its_task() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("working-artifact.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"港城"})),
        false,
    )
    .await;
    let root = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let brief_uri = format!("{root}/artifacts/brief");
    let (status, first) = call(
        &path,
        "POST",
        &brief_uri,
        Some(json!({
            "payload":{"premise":"港城遗忘海名"},"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":"first-brief"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, latest) = call(
        &path,
        "POST",
        &brief_uri,
        Some(json!({
            "payload":{"premise":"港城记住海名"},"expectedRevision":0,
            "expectedCandidateRevisionId":first["revisionId"],"idempotencyKey":"latest-brief"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{latest}");
    let baseline_uri = format!("{root}/working-baselines/brief");
    let (status, _) = call(
        &path,
        "PUT",
        &baseline_uri,
        Some(json!({
            "revisionId":first["revisionId"],"expectedSelectionRevision":0
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path,"POST",&format!("{root}/tasks"),Some(json!({
        "operation":"advisory","inputRevision":0,"threadId":"selected-planner","target":{"type":"project"},
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"authorIntent":"从选定简报分析故事"
    })),false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let uri = format!(
        "{root}/context?taskId={}&threadId=selected-planner&projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()
    );
    let (status, context) = call(&path, "GET", &uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["narrativeGoals"]["brief"]["premise"],
        "港城遗忘海名"
    );
    assert_eq!(
        context["narrativeGoals"]["brief"]["sourceRevisionId"],
        first["revisionId"]
    );
    assert_eq!(
        context["narrativeGoals"]["brief"]["workingSelectionRevision"],
        1
    );
    let (status, _) = call(
        &path,
        "DELETE",
        &baseline_uri,
        Some(json!({
            "expectedSelectionRevision":1
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, stale) = call(&path, "GET", &uri, None, true).await;
    assert!(status.is_client_error(), "{stale}");
    let (status, author_context) = call(
        &path,
        "GET",
        &format!("{root}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{author_context}");
    assert_eq!(
        author_context["narrativeGoals"]["brief"]["premise"],
        "港城记住海名"
    );
    assert_eq!(
        author_context["narrativeGoals"]["brief"]["sourceRevisionId"],
        latest["revisionId"]
    );
}

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    candidate: bool,
) -> (StatusCode, Value) {
    let app: Router = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let mut request = Request::builder().method(method).uri(uri).header(
        "authorization",
        format!("Bearer {}", if candidate { CANDIDATE } else { AUTHOR }),
    );
    if candidate {
        request = request.header("x-novel-task-capability", CAPABILITY);
    }
    let body = if let Some(payload) = payload {
        request = request.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .author_fixture_request(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("non-JSON response: {}", String::from_utf8_lossy(&bytes)));
    (status, value)
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
