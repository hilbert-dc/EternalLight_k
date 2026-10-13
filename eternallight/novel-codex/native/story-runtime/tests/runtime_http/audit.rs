use super::*;

#[tokio::test]
async fn author_search_audit_runs_in_worker_and_can_pause_or_resume() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/search-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(search_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = search_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert!(done.audit.complete());
    let (status, fetched) =
        call(application(&path), "GET", &format!("{base}/{job_id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["status"], "completed");
    assert_eq!(fetched["verifiedConsistent"], true);
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["id"], job_id);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_entity_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("entity-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Entity audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/entity-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(entity_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = entity_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_relation_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("relation-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Relation audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/relation-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    assert_eq!(started["verifiedConsistent"], Value::Null);
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(relation_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = relation_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_knowledge_audit_checks_both_indexes_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("knowledge-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Knowledge audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/knowledge-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    assert_eq!(started["verifiedConsistent"], Value::Null);
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(knowledge_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = knowledge_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert_eq!(done.verified_consistent, Some(true));
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    assert_eq!(latest["job"]["audit"]["assertions"]["publishedBatches"], 0);
    assert_eq!(latest["job"]["audit"]["epistemic"]["publishedBatches"], 0);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_fact_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fact-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Fact audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/fact-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(fact_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = fact_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap().unwrap();
    assert_eq!(
        (done.status.as_str(), done.verified_consistent),
        ("completed", Some(true))
    );
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["audit"]["facts"], 0);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn author_historical_knowledge_audit_runs_in_worker_and_is_hidden_from_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot-knowledge-audit.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Historical knowledge audit"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/snapshot-knowledge-audits");
    let (status, started) = call(application(&path), "POST", &base, None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(started["status"], "queued");
    assert_eq!(started["verifiedConsistent"], Value::Null);
    let job_id = started["id"].as_str().unwrap();
    let control = format!("{base}/{job_id}/control");
    let (status, paused) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"pause","expectedVersion":started["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(snapshot_knowledge_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap(), None);
    let (status, resumed) = call(
        application(&path),
        "POST",
        &control,
        Some(json!({"command":"resume","expectedVersion":paused["version"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["status"], "queued");
    let done = snapshot_knowledge_audit_tick(&novel_codex_story_core::StoryDatabase::open(&path, 2).unwrap()).unwrap().unwrap();
    assert_eq!(
        (done.status.as_str(), done.verified_consistent),
        ("completed", Some(true))
    );
    let (status, latest) = call(application(&path), "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["job"]["audit"]["members"], 0);
    assert_eq!(latest["job"]["verifiedConsistent"], true);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{base}/{job_id}"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
