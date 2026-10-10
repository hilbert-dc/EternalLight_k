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

#[tokio::test]
async fn checkpoint_rebind_preserves_author_identity_and_rejects_creative_instructions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rebind.sqlite3");
    let app = router(
        task_fixture::state(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"续写"})),
    )
    .await;
    let root = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let (status, initial) = call(&app,"POST",&format!("{root}/generation-runs"),AUTHOR,
        Some(json!({"expectedProjectRevision":0,"authorIntent":"按我的要求写两章，每章约3000字，不改已有原文", "targetChapters":2,
            "maxTaskAttempts":12,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":null}))).await;
    assert_eq!(status, StatusCode::CREATED, "{initial}");
    let uri = format!("{root}/generation-runs/{}", initial["id"].as_str().unwrap());
    let (_, paused) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        AUTHOR,
        Some(json!({"expectedVersion":initial["version"],"command":"pause"})),
    )
    .await;
    let (status, saved) = call(&app,"POST",&format!("{root}/artifacts/brief"),AUTHOR,
        Some(json!({"payload":{"premise":"当前简报"},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"source-edit"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let (_, preview) = call(&app, "GET", &format!("{uri}/replan"), AUTHOR, None).await;
    let request = json!({"expectedVersion":paused["version"],"expectedProjectRevision":preview["projectRevision"],
        "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"rollingPlan"});
    let mut forged = request.clone();
    forged["instruction"] = json!("降低每章篇幅要求");
    assert_eq!(
        call(&app, "POST", &format!("{uri}/rebind"), AUTHOR, Some(forged))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(call(&app, "GET", &uri, AUTHOR, None).await.1, paused);
    let (status, rebound) = call(
        &app,
        "POST",
        &format!("{uri}/rebind"),
        AUTHOR,
        Some(request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rebound}");
    for field in [
        "intentGeneration",
        "intentRevisions",
        "authorIntent",
        "targetChapters",
        "completedChapters",
        "providerBudget",
        "usedTaskAttempts",
        "retryAttempts",
        "maxRetryAttempts",
        "maxTaskAttempts",
        "deadlineAt",
    ] {
        assert_eq!(
            rebound[field], paused[field],
            "rebind must preserve {field}"
        );
    }
    assert_eq!(rebound["sourceDigest"], preview["sourceDigest"]);
    assert_eq!(rebound["status"], "running");
    assert!(
        call(
            &app,
            "POST",
            &format!("{uri}/rebind"),
            AUTHOR,
            Some(request)
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(call(&app, "GET", &uri, AUTHOR, None).await.1, rebound);
}

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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
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
    let create = json!({"operation":"advisory","inputRevision":0,"threadId":"brief","target":next["action"]["target"],
        "authorIntent":"写港城故事","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600});
    let (status, task) = call(&app, "POST", &format!("{root}/tasks"), AUTHOR, Some(create)).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (status, grant) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task_id}/provider-budget"),
        AUTHOR,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    assert_eq!(grant["taskId"], task_id);
    assert_eq!(grant["budgetId"], format!("budget-{task_id}"));
    assert_eq!(grant["maxRequests"], 256);
    assert_eq!(grant["maxTokens"], 4000000);
    assert_eq!(grant["deadlineAt"], now + 600);
    // The checkpoint replan below starts from a settled terminal attempt.
    // Active author pauses cancel the attempt and are covered by continuous recovery tests.
    let (status, mut decision) = call(
        &app,
        "GET",
        &format!("{root}/tasks/{task_id}/work-decision?threadId=brief&expectedRevision=0"),
        CANDIDATE,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    assert_eq!(decision["pagination"]["page"], 0);
    assert_eq!(decision["threadId"], "brief");
    assert_eq!(decision["workTaskId"], grant["workTaskId"]);
    assert_eq!(decision["coverage"]["actionsOffset"], 0);
    let action_count = decision["coverage"]["availableActions"].as_u64().unwrap();
    let mut cursor = decision["pagination"]["nextCursor"].clone();
    let mut seen_cursors = std::collections::BTreeSet::new();
    let mut expected_page = 1_u64;
    while let Some(current) = cursor.as_str() {
        assert!(seen_cursors.insert(current.to_owned()), "repeated cursor");
        let (status, page) = call(
            &app,
            "GET",
            &format!(
                "{root}/tasks/{task_id}/work-decision?threadId=brief&expectedRevision=0&cursor={}",
                current.replace(':', "%3A")
            ),
            CANDIDATE,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        for field in ["workTaskId", "threadId", "version", "sourceDigest"] {
            assert_eq!(page[field], decision[field], "page fence {field}");
        }
        assert_eq!(page["pagination"]["page"], expected_page);
        assert_eq!(page["coverage"]["availableActions"], action_count);
        assert_eq!(
            page["coverage"]["actionsOffset"],
            (expected_page * 2).min(action_count)
        );
        assert_eq!(
            page["pagination"]["hasMore"],
            !page["pagination"]["nextCursor"].is_null()
        );
        decision["readyActions"]
            .as_array_mut()
            .unwrap()
            .extend(page["readyActions"].as_array().unwrap().iter().cloned());
        cursor = page["pagination"]["nextCursor"].clone();
        expected_page += 1;
    }
    let all_actions = decision["readyActions"].as_array().unwrap();
    assert_eq!(all_actions.len() as u64, action_count);
    let action_keys = all_actions
        .iter()
        .map(|action| action["actionKey"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(action_keys.len(), all_actions.len());
    let action = decision["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "generateBrief")
        .unwrap();
    let (status, selected) = call(&app, "POST", &format!("{root}/tasks/{task_id}/work-decision/actions"), CANDIDATE,
        Some(json!({"taskId":task_id,"threadId":"brief","expectedRevision":0,
          "expectedWorkVersion":decision["version"],"expectedSourceDigest":decision["sourceDigest"],
          "actionKey":action["actionKey"],"instruction":"先整理港城构想","idempotencyKey":"choose-brief"}))).await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    let (status, completed) = call(
        &app,
        "POST",
        &format!("{root}/tasks/{task_id}/complete"),
        CANDIDATE,
        Some(json!({"threadId":"brief","summary":"已检查当前资料，等待作者调整方向"})),
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
        json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"]},"now":0}),
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
        Some(json!({"payload":{"premise":"抄写员藏好密函"},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"author-edit"}))).await;
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
    assert_eq!(
        context["availableFrom"],
        json!(["brief", "outline", "rollingPlan"])
    );
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
    assert_eq!(replanned["stage"], "coordinate");
    assert_eq!(
        replanned["replans"],
        json!([{"previousVersion":paused["version"],"previousStage":"coordinate","previousTaskId":paused["activeTaskId"],
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
    assert_eq!(next["action"]["operation"], "advisory");
    assert_eq!(next["action"]["target"]["intentGeneration"], 1);
    assert!(
        replanned["intentRevisions"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["instruction"]
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
