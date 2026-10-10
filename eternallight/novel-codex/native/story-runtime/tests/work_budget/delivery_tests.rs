use super::{AUTHOR, call, finish, initial};
use axum::http::StatusCode;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn work_get_list_and_attempt_routes_share_the_same_committed_delivery_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("work-delivery.sqlite3");
    let (root, request) = initial::request(&path).await;
    let (status, task) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let work_id = format!("work-{task_id}");
    let project = root.trim_start_matches("/api/projects/");
    finish(&path, project, task_id);
    let uri = format!("{root}/work-tasks/{work_id}");
    let (status, pending) = call(&path, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK);
    let mut report = json!({"evidence":[{"stepId":format!("step-{task_id}"),"attemptId":task_id,
        "kind":"completionReport","referenceId":task_id,"state":"recorded"}],
        "pendingActionIds":[],"unsettledAttemptIds":[task_id],"deliveryReview":null,
        "chapterAcceptanceReports":[],"issues":{"entries":[],"openCount":0,"staleCount":0,
        "disputedCount":0,"totalCount":0,"hasMore":false}});
    assert_eq!(
        (
            pending["status"].clone(),
            pending["deliveryStatus"].clone(),
            pending["deliveryReport"].clone()
        ),
        (json!("completed"), json!("pending"), report.clone())
    );
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.charge_work_provider_usage(
        &ProjectId::parse(project).unwrap(),
        &work_id,
        task_id,
        &json!({
            "runId":task_id,"budgetId":format!("budget-{task_id}"),"status":"completed",
            "limits":{"maxRequests":3,"maxTokens":700,"maxDurationMs":1000},
            "usage":{"requests":1,"tokens":25,"unknownTokens":0,"overrun":false}
        }),
    )
    .unwrap();
    let (status, settled) = call(&path, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK);
    report["unsettledAttemptIds"] = json!([]);
    assert_eq!(
        (
            settled["status"].clone(),
            settled["deliveryStatus"].clone(),
            settled["deliveryReport"].clone()
        ),
        (json!("completed"), json!("discussion_ended"), report)
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{root}/tasks/{task_id}/work-task"),
            Value::Null,
            AUTHOR
        )
        .await,
        (StatusCode::OK, settled.clone())
    );
    let (status, page) = call(
        &path,
        "GET",
        &format!("{root}/work-tasks"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["tasks"], json!([settled]));
    assert_eq!(
        repo.tasks(&ProjectId::parse(project).unwrap())
            .unwrap()
            .len(),
        1
    );
}
