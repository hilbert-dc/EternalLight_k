use super::*;

#[tokio::test]
async fn work_task_api_pages_all_active_tasks_and_rejects_invalid_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Many tasks"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = created["project"]["id"].as_str().unwrap();
    let create_uri = format!("/api/projects/{project_id}/tasks");
    for index in 0..31 {
        let (status, _) = call(
            application(&path),
            "POST",
            &create_uri,
            Some(json!({
                "operation": "advisory",
                "inputRevision": 0,
                "threadId": format!("thread-{index}"),
                "target": {"type":"project"},
                "capabilityHash": "a".repeat(64),
                "capabilityExpiresAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 600,
                "authorIntent": "Discuss this story"
            })),
        ).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let list_uri = format!("/api/projects/{project_id}/work-tasks");
    let (status, first) = call(application(&path), "GET", &list_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["activeCount"], 31);
    assert_eq!(first["tasks"].as_array().unwrap().len(), 30);
    let cursor = first["nextCursor"].as_str().unwrap();
    let (status, second) = call(
        application(&path),
        "GET",
        &format!("{list_uri}?cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(second["nextCursor"], Value::Null);
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{list_uri}?cursor=invalid"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn candidate_writes_are_task_bound_and_capability_is_revoked_on_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Capability"})),
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
    let (status, second_checkpoint) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "memory, continued"},
            "expectedRevision": 0,
            "expectedCandidateRevisionId": artifact["revisionId"],
            "idempotencyKey": "candidate-brief-2",
            "taskId": task_id,
            "threadId": "thread-capability"
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second_checkpoint}");
    let task_context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=thread-capability&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, _) = call_as(
        application(&path),
        "GET",
        &task_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

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
    assert_eq!(completed["outputRefs"].as_array().unwrap().len(), 2);

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

    let next_capability = "next-task-secret-123456789012345678901";
    let next_hash = format!("{:x}", Sha256::digest(next_capability.as_bytes()));
    let (status, next_task) = call(
        application(&path),
        "POST",
        &task_uri,
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "thread-capability-next", "target": {},
            "capabilityHash": next_hash, "capabilityExpiresAt": capability_expires_at,
            "authorIntent": "Revise the existing brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{next_task}");
    assert_eq!(
        next_task["inputManifest"]["expectedCandidateRevisionId"],
        second_checkpoint["revisionId"]
    );
    assert_eq!(
        next_task["inputManifest"]["outputObjectId"],
        "artifact-brief"
    );
    let (status, author_change) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "author changed it"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": second_checkpoint["revisionId"],
            "idempotencyKey": "author-concurrent-brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{author_change}");
    let stale_context_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=thread-capability-next&projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
        next_task["id"].as_str().unwrap()
    );
    let (status, stale_context) = call_as(
        application(&path),
        "GET",
        &stale_context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(next_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_context}");
    let (status, stale_task_write) = call_as(
        application(&path),
        "POST",
        &artifact_uri,
        Some(json!({
            "payload": {"premise": "unfrozen task write"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": author_change["revisionId"],
            "idempotencyKey": "stale-task-brief",
            "taskId": next_task["id"], "threadId": "thread-capability-next"
        })),
        CANDIDATE_TOKEN,
        Some(next_capability),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_task_write}");
}
