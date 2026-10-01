use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_authoring::{AuthoringService, ReplanRun, ReplanStage};
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "replan-author-token-1234567890123456";
const CANDIDATE: &str = "replan-candidate-token-1234567890123";
const CAPABILITY: &str = "replan-task-secret-1234567890123456";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut body = body;
    if method == "POST"
        && uri.ends_with("/generation-runs")
        && let Some(payload) = body.as_mut()
        && payload.get("expectedBaseSnapshotId").is_none()
    {
        let workspace_uri = format!(
            "{}/creative-workspace",
            uri.trim_end_matches("/generation-runs")
        );
        let (_, workspace) = Box::pin(call(app, "GET", &workspace_uri, actor, None)).await;
        payload["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
        payload["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    }
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
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
    )
}

#[tokio::test]
async fn replan_uses_new_sources_without_reopening_attempts_or_refunding_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("replan.sqlite3");
    let app = router(
        task_fixture::state(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"title":"港城"})),
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status,initial) = call(&app,"POST",&format!("{root}/generation-runs"),AUTHOR,
        Some(json!({"expectedProjectRevision":0,"authorIntent":"写一章港城故事","targetChapters":1,"maxTaskAttempts":8,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600}))).await;
    assert_eq!(status, StatusCode::CREATED, "{initial}");
    let uri = format!("{root}/generation-runs/{}", initial["id"].as_str().unwrap());
    let (_, next) = call(
        &app,
        "POST",
        &format!("{uri}/next"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    let create = json!({"operation":"generateBrief","inputRevision":0,"threadId":"brief","target":next["action"]["target"],
        "authorIntent":"写港城故事","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600});
    let (status, task) = call(&app, "POST", &format!("{root}/tasks"), AUTHOR, Some(create)).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (_, claimed) = call(&app, "GET", &uri, AUTHOR, /*body*/ None).await;
    let (status, paused_active) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        AUTHOR,
        Some(json!({"expectedVersion":claimed["version"],"command":"pause"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused_active}");
    let request = json!({"expectedVersion":paused_active["version"],"expectedProjectRevision":0,
        "expectedSourceDigest":initial["sourceDigest"],"restartFrom":"brief","instruction":"继续"});
    let (status, rejected) = call(
        &app,
        "POST",
        &format!("{uri}/replan"),
        AUTHOR,
        Some(request),
    )
    .await;
    assert!(status.is_client_error(), "{rejected}");
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*body*/ None).await.1,
        paused_active
    );
    let (status, resumed_active) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        AUTHOR,
        Some(json!({"expectedVersion":paused_active["version"],"command":"resume"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed_active}");
    let (status,brief) = call(&app,"POST",&format!("{root}/artifacts/brief"),CANDIDATE,
        Some(json!({"payload":{"premise":"抄写员烧掉密函"},"expectedRevision":0,"expectedCandidateRevisionId":null,
            "idempotencyKey":"brief","taskId":task_id,"threadId":"brief"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{brief}");
    let (status, completed) = call(
        &app,
        "POST",
        &format!("{root}/tasks/{task_id}/complete"),
        CANDIDATE,
        Some(json!({"threadId":"brief","summary":"简报候选已保存"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let (_, completed_run) = call(&app, "GET", &uri, AUTHOR, /*body*/ None).await;
    let (status, paused_completed) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        AUTHOR,
        Some(json!({"expectedVersion":completed_run["version"],"command":"pause"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused_completed}");
    let mut budget = PersistentBudget::new(
        &path.with_extension("budget.sqlite3"),
        task_id,
        json!({"options":{"id":format!("budget-{task_id}")},"now":0}),
    )
    .unwrap();
    budget.complete("terminal").unwrap();
    let (status, charged) = call(
        &app,
        "POST",
        &format!("{uri}/provider-usage"),
        AUTHOR,
        Some(json!({"taskId":task_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    let project_id = ProjectId::parse(project.to_owned()).unwrap();
    let completed_record = SqliteStoryRepository::open(&path)
        .unwrap()
        .task(&project_id, task_id)
        .unwrap();
    let (status, resumed) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        AUTHOR,
        Some(json!({"expectedVersion":charged["version"],"command":"resume"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let (_, paused) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        AUTHOR,
        Some(json!({"expectedVersion":resumed["version"],"command":"pause"})),
    )
    .await;
    let (status,changed) = call(&app,"POST",&format!("{root}/artifacts/brief"),AUTHOR,
        Some(json!({"payload":{"premise":"抄写员藏好密函"},"expectedRevision":0,"expectedCandidateRevisionId":brief["revisionId"],"idempotencyKey":"author-edit"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{changed}");
    let (status, context) = call(
        &app,
        "GET",
        &format!("{uri}/replan"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let mut request = json!({"expectedVersion":paused["version"],"expectedProjectRevision":0,
        "expectedSourceDigest":paused["sourceDigest"],"restartFrom":"outline","instruction":"按最新藏信设定重新规划，仍只写一章"});
    let (status, rejected) = call(
        &app,
        "POST",
        &format!("{uri}/replan"),
        AUTHOR,
        Some(request.clone()),
    )
    .await;
    assert!(status.is_client_error(), "{rejected}");
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*body*/ None).await.1,
        paused
    );
    assert_eq!(context["run"], paused);
    assert_eq!(context["availableFrom"], json!(["brief", "outline"]));
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*body*/ None).await.1,
        paused
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{uri}/replan"),
            CANDIDATE,
            /*body*/ None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    request["expectedSourceDigest"] = context["sourceDigest"].clone();
    let mut missing_outline = request.clone();
    missing_outline["restartFrom"] = json!("rollingPlan");
    assert!(
        call(
            &app,
            "POST",
            &format!("{uri}/replan"),
            AUTHOR,
            Some(missing_outline)
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*body*/ None).await.1,
        paused
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{uri}/replan"),
            CANDIDATE,
            Some(request.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, replanned) = call(
        &app,
        "POST",
        &format!("{uri}/replan"),
        AUTHOR,
        Some(request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replanned}");
    for field in [
        "usedTaskAttempts",
        "maxTaskAttempts",
        "retryAttempts",
        "maxRetryAttempts",
        "deadlineAt",
        "completedChapters",
        "targetChapters",
        "feedbackRepairCount",
    ] {
        assert_eq!(replanned[field], paused[field], "{field}");
    }
    assert_eq!(replanned["usedTaskAttempts"], 1);
    assert_eq!(replanned["stage"], "outline");
    assert_eq!(
        replanned["replans"],
        json!([{"previousVersion":paused["version"],"previousStage":"outline","previousTaskId":null,
        "restartFrom":"outline","previousSourceDigest":paused["sourceDigest"],"previousSnapshotId":paused["baseSnapshotId"],"sourceDigest":request["expectedSourceDigest"],
        "baseSnapshotId":replanned["baseSnapshotId"],"intentGeneration":1,"createdAt":replanned["replans"][0]["createdAt"]}])
    );
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .task(&project_id, task_id)
            .unwrap(),
        completed_record
    );
    let (status, next) = call(
        &app,
        "POST",
        &format!("{uri}/next"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generateOutline");
    assert_eq!(next["action"]["target"]["intentGeneration"], 1);
    assert!(
        next["action"]["authorIntent"]
            .as_str()
            .unwrap()
            .contains("藏信")
    );
    assert!(
        call(
            &app,
            "POST",
            &format!("{uri}/replan"),
            AUTHOR,
            Some(request)
        )
        .await
        .0
        .is_client_error()
    );

    // Seed a paused checkpoint with spent budget; no request may mint new allowance.
    let mut exhausted = replanned.clone();
    exhausted["version"] = json!(replanned["version"].as_i64().unwrap() + 1);
    exhausted["status"] = json!("paused");
    exhausted["usedTaskAttempts"] = json!(8);
    let run_id = initial["id"].as_str().unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.update_generation_run(
        &project_id,
        run_id,
        replanned["version"].as_i64().unwrap(),
        &exhausted.to_string(),
    )
    .unwrap();
    let before = repo.generation_run(&project_id, run_id).unwrap();
    assert!(
        AuthoringService::new(&mut repo)
            .replan(ReplanRun {
                project_id: &project_id,
                run_id,
                expected_version: before.version,
                expected_project_revision: 0,
                expected_source_digest: replanned["sourceDigest"].as_str().unwrap(),
                restart_from: ReplanStage::Outline,
                instruction: "继续",
                now
            })
            .is_err()
    );
    assert_eq!(repo.generation_run(&project_id, run_id).unwrap(), before);
    let cancelled = AuthoringService::new(&mut repo)
        .control(&project_id, run_id, before.version, "cancel", now)
        .unwrap();
    let before = repo.generation_run(&project_id, run_id).unwrap();
    assert!(
        AuthoringService::new(&mut repo)
            .replan(ReplanRun {
                project_id: &project_id,
                run_id,
                expected_version: cancelled.version,
                expected_project_revision: 0,
                expected_source_digest: &cancelled.source_digest,
                restart_from: ReplanStage::Outline,
                instruction: "恢复已取消运行",
                now
            })
            .is_err()
    );
    assert_eq!(repo.generation_run(&project_id, run_id).unwrap(), before);
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
