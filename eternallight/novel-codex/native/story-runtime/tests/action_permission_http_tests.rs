use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "permission-author-token-123456789012345";
const CANDIDATE: &str = "permission-candidate-token-123456789012";
const CAPABILITY: &str = "permission-task-secret-1234567890123456";

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let app = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json")
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    let response = app.author_fixture_request(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn author_reads_frozen_permission_without_granting_candidate_access_or_new_execution() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("permission.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"title":"许可依据"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status, source) = call(&path, "POST", &format!("{base}/tasks"), json!({
        "operation":"advisory","inputRevision":0,"threadId":"permission-thread",
        "target":{"type":"project"},"authorIntent":"为港城故事拟一份候选简报",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires
    }), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    let source_id = source["id"].as_str().unwrap();
    let actions = format!("{base}/tasks/{source_id}/actions");
    let (status, action) = call(&path, "POST", &actions, json!({
        "taskId":source_id,"threadId":"permission-thread","expectedRevision":0,"expectedIntentRevision":0,
        "idempotencyKey":"proposal","operation":"generateBrief","target":{},"instruction":"保存简报候选"
    }), CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let action_id = action["id"].as_str().unwrap();
    let uri = format!("{actions}/{action_id}/permission");
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, json!({"permission":null}))
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, approved) = call(
        &path,
        "POST",
        &format!("{actions}/{action_id}/approve"),
        json!({"expectedIntentRevision":0}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let (status, read) = call(&path, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(
        read["permission"]["evidence"],
        json!({
            "authorMessageId":source["inputManifest"]["authorMessageId"],
            "projectId":project,"sourceTaskId":source_id,"intentRevision":0,"inputRevision":0,
            "operation":"generateBrief","target":{"type":"artifact"},"instruction":"保存简报候选",
            "authorIntent":"为港城故事拟一份候选简报"
        })
    );
    assert_eq!(read["permission"]["kind"], "exactAction");
    assert_eq!(read["permission"]["actionStatus"], "queued");
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, read.clone())
    );
    let (_, other) = call(
        &path,
        "POST",
        "/api/projects",
        json!({"title":"另一作品"}),
        AUTHOR,
    )
    .await;
    let foreign = uri.replace(project, other["project"]["id"].as_str().unwrap());
    assert!(
        !call(&path, "GET", &foreign, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
    assert_eq!(
        call(&path, "GET", &uri, Value::Null, CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let budget_uri = format!("{base}/tasks/{source_id}/provider-budget");
    let original = "为港城故事拟一份候选简报";
    // Message provenance must remain authoritative independently of the
    // budget's initial-request marker, including corrupted budget metadata.
    let digest: String = db.query_row("SELECT initial_request_digest FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",[format!("work-{source_id}")],|row|row.get(0)).unwrap();
    db.execute("UPDATE story_work_budget_authorizations_v1 SET initial_request_digest=NULL WHERE work_task_id=?1",[format!("work-{source_id}")]).unwrap();
    for (sql, restore) in [
        (
            "UPDATE story_author_messages_v1 SET author_text=?1 WHERE task_id=?2",
            "UPDATE story_author_messages_v1 SET author_text=?1 WHERE task_id=?2",
        ),
        (
            "UPDATE story_tasks_v2 SET input_manifest_json=json_set(input_manifest_json,'$.authorIntent',?1) WHERE task_id=?2",
            "UPDATE story_tasks_v2 SET input_manifest_json=json_set(input_manifest_json,'$.authorIntent',?1) WHERE task_id=?2",
        ),
    ] {
        db.execute(sql, rusqlite::params!["不是作者的指令", source_id])
            .unwrap();
        assert!(
            !call(&path, "GET", &uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        assert!(
            !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
                .await
                .0
                .is_success()
        );
        db.execute(restore, rusqlite::params![original, source_id])
            .unwrap();
        assert_eq!(
            call(&path, "GET", &uri, Value::Null, AUTHOR).await,
            (StatusCode::OK, read.clone())
        );
        assert_eq!(
            call(&path, "GET", &budget_uri, Value::Null, AUTHOR).await.0,
            StatusCode::OK
        );
    }
    db.execute("UPDATE story_work_budget_authorizations_v1 SET initial_request_digest=?1 WHERE work_task_id=?2",rusqlite::params![digest,format!("work-{source_id}")]).unwrap();
    db.execute(
        "UPDATE story_task_action_requests_v1 SET instruction='扩大范围' WHERE action_id=?1",
        [action_id],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
    db.execute(
        "UPDATE story_task_action_requests_v1 SET instruction='保存简报候选' WHERE action_id=?1",
        [action_id],
    )
    .unwrap();
    db.execute(
        "DELETE FROM story_author_messages_v1 WHERE task_id=?1",
        [source_id],
    )
    .unwrap();
    assert!(
        !call(&path, "GET", &uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
    assert!(
        !call(&path, "GET", &budget_uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
