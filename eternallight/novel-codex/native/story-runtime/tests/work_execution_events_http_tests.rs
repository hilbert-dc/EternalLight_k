use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::{RuntimeState, router};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
#[path = "support/settle_task_fixture.rs"]
mod usage;
const AUTHOR: &str = "work-events-author-token-123456789";
const CANDIDATE: &str = "work-events-candidate-token-123456";
const CAP: &str = "work-events-root-capability-123456";
fn app(path: &std::path::Path) -> Router {
    router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    )
}
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("x-novel-task-capability", CAP)
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}
async fn project(app: &Router) -> (String, String, Value) {
    let (code,result)=call(app,"POST","/api/projects",json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"持久结果通知验收"}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{result}");
    let project = result["project"]["id"].as_str().unwrap().to_owned();
    let base = format!("/api/projects/{project}");
    let (code, workspace) = call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{workspace}");
    (project, base, workspace)
}
fn task_request(target: Value, operation: Value, intent: &str) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"threadId":"original-thread","operation":operation,"inputRevision":0,"target":target,"authorIntent":intent,"capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600})
}
async fn generation_root(app: &Router) -> (String, String, Value, Value) {
    generation_root_with_limit(app, json!(200_000)).await
}
async fn generation_root_with_limit(app: &Router, tokens: Value) -> (String, String, Value, Value) {
    let (project, base, workspace) = project(app).await;
    let (code,run)=call(app,"POST",&format!("{base}/generation-runs"),json!({"expectedProjectRevision":0,"expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"authorIntent":"生成第一章，方法由模型选择","targetChapters":1,"maxTaskAttempts":10,"maxProviderRequests":16,"maxProviderTokens":tokens,"deadlineAt":null}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{run}");
    let (code, next) = call(
        app,
        "POST",
        &format!(
            "{base}/generation-runs/{}/next",
            run["id"].as_str().unwrap()
        ),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{next}");
    let (code, task) = call(
        app,
        "POST",
        &format!("{base}/tasks"),
        task_request(
            next["action"]["target"].clone(),
            next["action"]["operation"].clone(),
            "生成第一章，方法由模型选择",
        ),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    (project, base, run, task)
}
async fn settle_generation(
    app: &Router,
    path: &std::path::Path,
    project: &str,
    base: &str,
    run: &Value,
    task: &Value,
    status: &str,
) {
    let db = rusqlite::Connection::open(path).unwrap();
    let stored:String=db.query_row("SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1 AND work_task_id=?2",rusqlite::params![task["id"].as_str().unwrap(),format!("work-{}",run["id"].as_str().unwrap())],|r|r.get(0)).unwrap();
    let grant: novel_codex_story_core::TaskProviderBudget = serde_json::from_str(&stored).unwrap();
    assert_eq!(grant.task_id, task["id"].as_str().unwrap());
    assert_eq!(
        SqliteStoryRepository::open(path)
            .unwrap()
            .task(&ProjectId::parse(project).unwrap(), &grant.task_id)
            .unwrap()
            .unwrap()
            .status,
        status
    );
    let mut engine=eternallight_engine_core::PersistentBudget::new(&path.with_extension("engine.sqlite3"),task["id"].as_str().unwrap(),json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"startedAt":1000},"now":1000})).unwrap();
    engine.complete(status).unwrap();
    let (code, result) = call(
        app,
        "POST",
        &format!(
            "{base}/generation-runs/{}/provider-usage",
            run["id"].as_str().unwrap()
        ),
        json!({"taskId":task["id"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
}
async fn pending_work(
    app: &Router,
    path: &std::path::Path,
) -> (String, String, Value, Value, Value) {
    let (project, base, workspace) = project(app).await;
    let mut request = task_request(
        json!({"type":"project"}),
        json!("advisory"),
        "整理人物背景并保存简报候选",
    );
    request["initialBudget"] = json!({"authorizationId":uuid::Uuid::new_v4().to_string(),"authorMessageId":uuid::Uuid::new_v4().to_string(),"expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"maxRequests":16,"maxTokens":200_000,"deadlineAt":null});
    let (code, task) = call(app, "POST", &format!("{base}/tasks"), request, AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let goal = json!({"id":"brief","title":"保存人物简报候选","acceptance":"真实候选保存后读取核对","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"","checkScope":"原作者要求的简报","approach":"按实际结果选用方法","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null});
    let (code,result)=call(app,"POST",&format!("{base}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedPlanVersion":0,"idempotencyKey":"current-method","plan":{"intentMode":"executeGoals","goals":[goal]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    let (code,action)=call(app,"POST",&format!("{base}/tasks/{id}/actions"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"brief-action","operation":"generateBrief","target":{"type":"artifact"},"instruction":"整理人物背景并保存候选"}),CANDIDATE).await;
    assert_eq!(code, StatusCode::CREATED, "{action}");
    let (code, result) = call(
        app,
        "POST",
        &format!(
            "{base}/tasks/{id}/actions/{}/approve",
            action["id"].as_str().unwrap()
        ),
        json!({"expectedIntentRevision":0}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    let (code, result) = call(
        app,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":"original-thread","summary":"已登记真实创作行动，候选尚待生成保存"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    usage::settle(path, &project, id);
    let (_, page) = call(
        app,
        "GET",
        &format!("{base}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let event = page["items"][0].clone();
    assert_eq!(event["sourceAttemptId"], id);
    assert_eq!(event["rootThreadId"], "original-thread");
    (project, base, task, action, event)
}
async fn claim(app: &Router, base: &str, event: &Value, session: &str) -> (StatusCode, Value) {
    let (_, workspace) = call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (_, work) = call(
        app,
        "GET",
        &format!(
            "{base}/work-tasks/{}",
            event["workTaskId"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    call(app,"POST",&format!("{base}/work-execution-events/{}/claim",event["eventId"].as_str().unwrap()),json!({"hostSessionId":session,"expectedWorkVersion":work["version"],"expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}),AUTHOR).await
}
async fn read_author_requirements(app: &Router, base: &str, task: &Value) {
    let id = task["id"].as_str().unwrap();
    let (code,context)=call(app,"GET",&format!("{base}/context?taskId={id}&threadId=original-thread&projectRevision={}&purpose=planner",task["inputRevision"]),Value::Null,CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{context}");
    let source = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sectionPath"] == "/authorRequirementsSource")
        .unwrap();
    let mut cursor = String::new();
    loop {
        let (code,page)=call(app,"GET",&format!("{base}/tasks/{id}/task-source?threadId=original-thread&expectedRevision={}&sourceId={}{cursor}",task["inputRevision"],source["sourceId"].as_str().unwrap()),Value::Null,CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        let Some(next) = page["nextCursor"].as_str() else {
            break;
        };
        cursor = format!("&cursor={next}");
    }
}
#[tokio::test]
async fn host_event_routes_reject_real_candidate_credentials_and_unknown_authority_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.sqlite3");
    let app = app(&path);
    let (_, base, _, _, event) = pending_work(&app, &path).await;
    for (method, suffix) in [
        ("GET", "work-execution-events"),
        ("POST", "work-execution-events/event/claim"),
        ("POST", "work-execution-events/event/ack"),
        ("POST", "work-execution-events/event/release"),
        ("POST", "work-tasks/work/execution-owner"),
        ("POST", "work-tasks/work/execution-owner/renew"),
        ("POST", "work-tasks/work/execution-owner/release"),
    ] {
        assert_eq!(
            call(
                &app,
                method,
                &format!("{base}/{suffix}"),
                json!({}),
                CANDIDATE
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED,
            "{suffix}"
        );
    }
    assert!(
        !call(
            &app,
            "GET",
            &format!("{base}/work-execution-events?limit=21"),
            Value::Null,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let endpoint = format!(
        "{base}/work-execution-events/{}/claim",
        event["eventId"].as_str().unwrap()
    );
    assert!(!call(&app,"POST",&endpoint,json!({"hostSessionId":"host","expectedWorkVersion":event["workVersion"],"expectedProjectRevision":0,"expectedSourceDigest":"invalid","budget":true}),AUTHOR).await.0.is_success());
    let (_, claim) = claim(&app, &base, &event, "host").await;
    assert_eq!(claim["admission"], "claimed");
    let endpoint = format!(
        "{base}/work-execution-events/{}/ack",
        event["eventId"].as_str().unwrap()
    );
    assert_eq!(call(&app,"POST",&endpoint,json!({"hostSessionId":"host","claimId":claim["event"]["claimId"],"leaseId":claim["lease"]["leaseId"]}),AUTHOR).await.0,StatusCode::UNPROCESSABLE_ENTITY);
}
#[tokio::test]
async fn restart_claim_replay_real_attached_child_ack_and_stale_lease_release() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restart.sqlite3");
    let original = app(&path);
    let (project, base, task, action, event) = pending_work(&original, &path).await;
    let restarted = app(&path);
    let (code, first) = claim(&restarted, &base, &event, "host-a").await;
    assert_eq!(code, StatusCode::OK, "{first}");
    assert_eq!(first["admission"], "claimed");
    let (_, replay) = claim(&restarted, &base, &event, "host-a").await;
    assert_eq!(replay["event"]["claimId"], first["event"]["claimId"]);
    assert_eq!(
        claim(&restarted, &base, &event, "host-b").await.1["blockedReason"],
        "activeOwner"
    );
    let ack_uri = format!(
        "{base}/work-execution-events/{}/ack",
        event["eventId"].as_str().unwrap()
    );
    let ack = json!({"hostSessionId":"host-a","claimId":first["event"]["claimId"],"leaseId":first["lease"]["leaseId"],"consumerStepId":action["id"]});
    assert!(
        !call(&restarted, "POST", &ack_uri, ack.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    let mut target = action["target"].clone();
    target["requestedActionId"] = action["id"].clone();
    target["requestedByTaskId"] = task["id"].clone();
    let (code, child) = call(
        &restarted,
        "POST",
        &format!("{base}/tasks"),
        task_request(target, json!("generateBrief"), "整理人物背景并保存简报候选"),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{child}");
    let (code, consumed) = call(&restarted, "POST", &ack_uri, ack, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{consumed}");
    assert_eq!(consumed["state"], "consumed");
    let owner_uri = format!(
        "{base}/work-tasks/{}/execution-owner/release",
        event["workTaskId"].as_str().unwrap()
    );
    assert_eq!(
        call(
            &restarted,
            "POST",
            &owner_uri,
            json!({"hostSessionId":"host-b","leaseId":"wrong"}),
            AUTHOR
        )
        .await
        .1["released"],
        false
    );
    assert_eq!(
        call(
            &restarted,
            "POST",
            &owner_uri,
            json!({"hostSessionId":"host-a","leaseId":first["lease"]["leaseId"]}),
            AUTHOR
        )
        .await
        .1["released"],
        true
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_author_messages_v1 WHERE work_task_id=?1",
            [event["workTaskId"].as_str().unwrap()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .tasks(&ProjectId::parse(project).unwrap())
            .unwrap()
            .len(),
        2
    );
}
#[tokio::test]
async fn generation_ownership_waits_for_real_thread_and_terminal_usage_is_not_assumed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("generation.sqlite3");
    let app = app(&path);
    let (project, base, workspace) = project(&app).await;
    let (code,run)=call(&app,"POST",&format!("{base}/generation-runs"),json!({"expectedProjectRevision":0,"expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"authorIntent":"生成第一章，方法由模型选择","targetChapters":1,"maxTaskAttempts":10,"maxProviderRequests":16,"maxProviderTokens":200_000,"deadlineAt":null}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{run}");
    let work_id = format!("work-{}", run["id"].as_str().unwrap());
    let owner_uri = format!("{base}/work-tasks/{work_id}/execution-owner");
    let (_, work) = call(
        &app,
        "GET",
        &format!("{base}/work-tasks/{work_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["threadId"], "");
    assert!(
        !call(
            &app,
            "POST",
            &owner_uri,
            json!({"hostSessionId":"host","expectedWorkVersion":work["version"]}),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (_, next) = call(
        &app,
        "POST",
        &format!(
            "{base}/generation-runs/{}/next",
            run["id"].as_str().unwrap()
        ),
        json!({}),
        AUTHOR,
    )
    .await;
    let (code, task) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        task_request(
            next["action"]["target"].clone(),
            next["action"]["operation"].clone(),
            "生成第一章，方法由模型选择",
        ),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let (_, work) = call(
        &app,
        "GET",
        &format!("{base}/work-tasks/{work_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["threadId"], "original-thread");
    let (code, lease) = call(
        &app,
        "POST",
        &owner_uri,
        json!({"hostSessionId":"host","expectedWorkVersion":work["version"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{lease}");
    SqliteStoryRepository::open(&path)
        .unwrap()
        .update_task(
            &ProjectId::parse(project).unwrap(),
            task["id"].as_str().unwrap(),
            "original-thread",
            novel_codex_story_core::TaskUpdate {
                status: "failed".into(),
                error: Some("provider_output_limit".into()),
                result_summary: None,
                now: 1,
            },
        )
        .unwrap();
    let (_, page) = call(
        &app,
        "GET",
        &format!("{base}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (_, result) = claim(&app, &base, &page["items"][0], "restart-host").await;
    assert_eq!(result["blockedReason"], "unsettledUsage");
    assert_eq!(result["event"]["state"], "pending");
}

#[tokio::test]
async fn actual_generation_completion_uses_original_source_budget_and_obeys_pause_and_cancel() {
    for command in ["pause", "cancel"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("generation-completion.sqlite3");
        let app = app(&path);
        let (project, base, run, task) = generation_root(&app).await;
        let id = task["id"].as_str().unwrap();
        let (code, mut read) = call(
            &app,
            "GET",
            &format!("{base}/tasks/{id}/work-decision?threadId=original-thread&expectedRevision=0"),
            Value::Null,
            CANDIDATE,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{read}");
        let mut cursor = read["pagination"]["nextCursor"].clone();
        while let Some(value) = cursor.as_str() {
            let (code,page)=call(&app,"GET",&format!("{base}/tasks/{id}/work-decision?threadId=original-thread&expectedRevision=0&cursor={value}"),Value::Null,CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{page}");
            read["readyActions"]
                .as_array_mut()
                .unwrap()
                .extend(page["readyActions"].as_array().unwrap().iter().cloned());
            cursor = page["pagination"]["nextCursor"].clone();
        }
        let action = read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["operation"] == "generateBrief")
            .expect("current permitted creative action");
        let (code,selected)=call(&app,"POST",&format!("{base}/tasks/{id}/work-decision/actions"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":action["actionKey"],"instruction":"读取原要求并先整理实际需要的背景资料","idempotencyKey":"selected-method"}),CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{selected}");
        let (code,result)=call(&app,"POST",&format!("{base}/tasks/{id}/complete"),json!({"threadId":"original-thread","summary":"已选择符合原授权的创作行动，结果尚未生成"}),CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{result}");
        let (_, page) = call(
            &app,
            "GET",
            &format!("{base}/work-execution-events"),
            Value::Null,
            AUTHOR,
        )
        .await;
        let event = page["items"][0].clone();
        assert_eq!(
            claim(&app, &base, &event, "host").await.1["blockedReason"],
            "unsettledUsage"
        );
        settle_generation(&app, &path, &project, &base, &run, &task, "completed").await;
        let (code, claimed) = claim(&app, &base, &event, "host").await;
        assert_eq!(code, StatusCode::OK, "{claimed}");
        assert_eq!(claimed["admission"], "claimed");
        assert_eq!(
            claimed["work"]["id"],
            format!("work-{}", run["id"].as_str().unwrap())
        );
        assert_eq!(claimed["lease"]["rootThreadId"], "original-thread");
        assert_eq!(claimed["event"]["sourceAttemptId"], id);
        let (_, current) = call(
            &app,
            "GET",
            &format!("{base}/generation-runs/{}", run["id"].as_str().unwrap()),
            Value::Null,
            AUTHOR,
        )
        .await;
        let (code, result) = call(
            &app,
            "POST",
            &format!(
                "{base}/generation-runs/{}/control",
                run["id"].as_str().unwrap()
            ),
            json!({"expectedVersion":current["version"],"command":command}),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{result}");
        assert_eq!(
            claim(&app, &base, &event, "restart-host").await.1["blockedReason"],
            "authorStopped"
        );
        let (code, _) = call(
            &app,
            "POST",
            &format!(
                "{base}/work-tasks/{}/execution-owner/renew",
                event["workTaskId"].as_str().unwrap()
            ),
            json!({"hostSessionId":"host","leaseId":claimed["lease"]["leaseId"]}),
            AUTHOR,
        )
        .await;
        assert!(!code.is_success());
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM story_author_messages_v1 WHERE work_task_id=?1",
                [event["workTaskId"].as_str().unwrap()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(result["providerBudget"]["maxRequests"], 16);
        assert_eq!(
            result["providerBudget"]["charges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
#[tokio::test]
async fn known_zero_generation_failure_returns_to_original_model_decision_without_author_resume() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("generation-retryable.sqlite3");
    let app = app(&path);
    let (project, base, run, task) = generation_root(&app).await;
    let (code, result) = call(
        &app,
        "PATCH",
        &format!("{base}/tasks/{}", task["id"].as_str().unwrap()),
        json!({"threadId":"original-thread","status":"failed","error":"provider_output_limit"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    settle_generation(&app, &path, &project, &base, &run, &task, "failed").await;
    let (_, page) = call(
        &app,
        "GET",
        &format!("{base}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let event = &page["items"][0];
    let (_, claimed) = claim(&app, &base, event, "restart-host").await;
    assert_eq!(claimed["admission"], "claimed", "{claimed}");
    assert_eq!(claimed["work"]["status"], "running");
    assert_eq!(claimed["work"]["kind"], "generation");
    assert_eq!(claimed["event"]["sourceAttemptId"], task["id"]);
    let (_, failed_work) = call(
        &app,
        "GET",
        &format!(
            "{base}/work-tasks/{}",
            event["workTaskId"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert!(
        failed_work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["currentAttemptId"] == task["id"] && step["status"] == "blocked")
    );
    let owner_uri = format!(
        "{base}/work-tasks/{}/execution-owner",
        event["workTaskId"].as_str().unwrap()
    );
    let (code, owned) = call(
        &app,
        "POST",
        &owner_uri,
        json!({"hostSessionId":"restart-host","expectedWorkVersion":claimed["work"]["version"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{owned}");
    assert_eq!(owned["leaseId"], claimed["lease"]["leaseId"]);
    let (code, renewed) = call(
        &app,
        "POST",
        &format!("{owner_uri}/renew"),
        json!({"hostSessionId":"restart-host","leaseId":owned["leaseId"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{renewed}");
    let (code, next) = call(
        &app,
        "POST",
        &format!(
            "{base}/generation-runs/{}/next",
            run["id"].as_str().unwrap()
        ),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{next}");
    assert_eq!(next["run"]["status"], "running");
    assert_eq!(next["action"]["operation"], "advisory");
    assert_eq!(
        next["action"]["target"]["workDecision"]["workTaskId"],
        event["workTaskId"]
    );
    let (code, child) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        task_request(
            next["action"]["target"].clone(),
            next["action"]["operation"].clone(),
            "生成第一章，方法由模型选择",
        ),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{child}");
    assert_ne!(child["id"], task["id"]);
    assert_eq!(child["threadId"], "original-thread");
    let repo = SqliteStoryRepository::open(&path).unwrap();
    let current = repo
        .work_task(
            &ProjectId::parse(project.clone()).unwrap(),
            event["workTaskId"].as_str().unwrap(),
        )
        .unwrap();
    let successor = current
        .steps
        .iter()
        .find(|step| step.current_attempt_id.as_deref() == child["id"].as_str())
        .unwrap();
    let (code, acked) = call(&app,"POST",&format!("{base}/work-execution-events/{}/ack",event["eventId"].as_str().unwrap()),
        json!({"hostSessionId":"restart-host","claimId":claimed["event"]["claimId"],"leaseId":owned["leaseId"],"consumerStepId":successor.id}),AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{acked}");
    assert_eq!(acked["state"], "consumed");
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_author_messages_v1 WHERE work_task_id=?1",
            [event["workTaskId"].as_str().unwrap()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        repo.tasks(&ProjectId::parse(project).unwrap())
            .unwrap()
            .len(),
        2
    );
    assert_eq!(next["run"]["providerBudget"]["maxRequests"], 16);
    assert_eq!(
        next["run"]["providerBudget"]["charges"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn real_generation_failed_attempt_and_receipt_do_not_resume_blocked_run() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("generation-blocked.sqlite3");
    let app = app(&path);
    let (project, base, run, task) = generation_root(&app).await;
    let (code, result) = call(
        &app,
        "PATCH",
        &format!("{base}/tasks/{}", task["id"].as_str().unwrap()),
        json!({"threadId":"original-thread","status":"failed","error":"provider_transport_failed"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    settle_generation(&app, &path, &project, &base, &run, &task, "failed").await;
    let (code, blocked) = call(
        &app,
        "POST",
        &format!(
            "{base}/generation-runs/{}/next",
            run["id"].as_str().unwrap()
        ),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{blocked}");
    assert_eq!(blocked["run"]["status"], "blocked");
    let (_, page) = call(
        &app,
        "GET",
        &format!("{base}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let event = &page["items"][0];
    let (_, result) = claim(&app, &base, event, "restart-host").await;
    assert_eq!(result["blockedReason"], "generationNotRunning");
    assert_eq!(result["event"]["state"], "pending");
    let (_, work) = call(
        &app,
        "GET",
        &format!(
            "{base}/work-tasks/{}",
            event["workTaskId"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert!(
        !call(
            &app,
            "POST",
            &format!(
                "{base}/work-tasks/{}/execution-owner",
                event["workTaskId"].as_str().unwrap()
            ),
            json!({"hostSessionId":"host","expectedWorkVersion":work["version"]}),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .tasks(&ProjectId::parse(project).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn real_pending_handoff_is_preserved_and_cannot_be_acked_as_final() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("handoff-event.sqlite3");
    let app = app(&path);
    let (project, base, workspace) = project(&app).await;
    let mut request = task_request(
        json!({"type":"project"}),
        json!("advisory"),
        "写一章中文悬疑小说，先提出创作范围等待批准",
    );
    request["initialBudget"] = json!({"authorizationId":uuid::Uuid::new_v4().to_string(),"authorMessageId":uuid::Uuid::new_v4().to_string(),"expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"maxRequests":16,"maxTokens":200_000,"deadlineAt":null});
    let (code, task) = call(&app, "POST", &format!("{base}/tasks"), request, AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let mut goal = json!({"id":"scope","title":"提出创作范围并等待作者批准","acceptance":"登记真实范围提案，不把未来执行当作交付","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"","checkScope":"创作范围提案","approach":"依据作者目标登记必要行动","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null});
    let (code,result)=call(&app,"POST",&format!("{base}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedPlanVersion":0,"idempotencyKey":"scope-plan","plan":{"intentMode":"executeGoals","goals":[goal.clone()]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    let (code,proposal)=call(&app,"POST",&format!("{base}/tasks/{id}/generation-handoff"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,"targetChapters":1,"instruction":"从失踪的钟表匠开始写一章候选","idempotencyKey":"scope-handoff"}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{proposal}");
    goal["status"] = json!("blocked");
    goal["assessment"] = json!("范围提案已登记，作者尚未批准正文执行");
    let (code,result)=call(&app,"POST",&format!("{base}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedPlanVersion":1,"idempotencyKey":"await-scope","plan":{"intentMode":"executeGoals","goals":[goal]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    let (code, status) = call(
        &app,
        "GET",
        &format!("{base}/tasks/{id}/work-decision?threadId=original-thread&expectedRevision=0"),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{status}");
    read_author_requirements(&app, &base, &task).await;
    let (code,result)=call(&app,"POST",&format!("{base}/tasks/{id}/complete"),json!({"threadId":"original-thread","summary":"范围提案已登记，等待作者批准；尚未生成正文","deliveryReview":{"expectedWorkVersion":status["version"],"expectedSourceDigest":status["sourceDigest"],"goalStatus":"partial","goalAssessment":"当前只有范围提案，正文执行尚未开始","checkedScope":"提案登记与作者批准状态","unresolved":[{"issue":{"issueId":"awaiting-author-scope","newIssue":{"summary":"正文执行等待作者批准范围"},"resolutionOf":[]},"observationId":null,"reason":"正文执行等待作者批准范围"}],"issueResolutions":[],"authorDecisions":["是否批准该创作范围"],"evidence":[]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    usage::settle(&path, &project, id);
    let (_, page) = call(
        &app,
        "GET",
        &format!("{base}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let event = &page["items"][0];
    let (code, claimed) = claim(&app, &base, event, "host").await;
    assert_eq!(code, StatusCode::OK, "{claimed}");
    assert_eq!(claimed["admission"], "claimed");
    assert!(!call(&app,"POST",&format!("{base}/work-execution-events/{}/ack",event["eventId"].as_str().unwrap()),json!({"hostSessionId":"host","claimId":claimed["event"]["claimId"],"leaseId":claimed["lease"]["leaseId"],"consumerStepId":null}),AUTHOR).await.0.is_success());
    let db = rusqlite::Connection::open(&path).unwrap();
    let consumed: Option<String> = db
        .query_row(
            "SELECT consumed_run_id FROM story_generation_handoffs_v1 WHERE proposal_id=?1",
            [proposal["proposal"]["proposalId"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(consumed.is_none());
    assert_eq!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .tasks(&ProjectId::parse(project).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn metering_only_unknown_failure_wakes_model_but_finite_budget_and_author_pause_hold() {
    for (limit, paused) in [(Value::Null, false), (json!(200_000), false), (Value::Null, true)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unknown-wake.sqlite3");
        let app = app(&path);
        let (_, base, run, task) = generation_root_with_limit(&app, limit.clone()).await;
        let failure = json!({"error":{"code":"task_provider_usage_unknown","type":"budget_unknown","retryable":false,"message":"task provider usage is unknown"}}).to_string();
        let (code, result) = call(&app,"PATCH",&format!("{base}/tasks/{}", task["id"].as_str().unwrap()),
            json!({"threadId":"original-thread","status":"failed","error":failure}),AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{result}");
        // The Host observes failure before the terminal receipt arrives. This
        // must pause safely, then recover on the durable settlement event.
        let (_, waiting) = call(&app,"POST",&format!("{base}/generation-runs/{}/next",run["id"].as_str().unwrap()),json!({}),AUTHOR).await;
        assert_eq!(waiting["run"]["status"],"blocked","{waiting}");
        let mut engine = eternallight_engine_core::PersistentBudget::new(&path.with_extension("engine.sqlite3"),task["id"].as_str().unwrap(),
            json!({"options":{"id":format!("budget-{}",task["id"].as_str().unwrap()),"maxRequests":16,"maxTokens":limit,"startedAt":1000},"now":1000})).unwrap();
        let reservation = engine.apply("budget.reserve",json!({"parameters":{"estimatedInputTokens":20,"maxOutputTokens":50},"now":1001})).unwrap();
        engine.apply("budget.settle",json!({"id":reservation,"usage":null,"now":1002})).unwrap();
        engine.complete("failed").unwrap();
        let (_, charged) = call(&app,"POST",&format!("{base}/generation-runs/{}/provider-usage",run["id"].as_str().unwrap()),json!({"taskId":task["id"]}),AUTHOR).await;
        assert_eq!(charged["providerBudget"]["unknownTokens"],70,"{charged}");
        if paused {
            let (code, result) = call(&app,"POST",&format!("{base}/generation-runs/{}/control",run["id"].as_str().unwrap()),
                json!({"expectedVersion":charged["version"],"command":"pause"}),AUTHOR).await;
            assert_eq!(code, StatusCode::OK,"{result}");
        }
        let (_, page) = call(&app,"GET",&format!("{base}/work-execution-events"),Value::Null,AUTHOR).await;
        let (_, claimed) = claim(&app,&base,&page["items"][0],"recovering-host").await;
        if paused || !limit.is_null() {
            assert_eq!(claimed["admission"],"blocked","{claimed}");
            continue;
        }
        assert_eq!(claimed["admission"],"claimed","{claimed}");
        let (code, next) = call(&app,"POST",&format!("{base}/generation-runs/{}/next",run["id"].as_str().unwrap()),json!({}),AUTHOR).await;
        assert_eq!(code,StatusCode::OK,"{next}");
        assert_eq!(next["action"]["operation"],"advisory","{next}");
        assert_eq!(next["run"]["providerBudget"]["unknownTokens"],70);
        assert_eq!(next["action"]["target"]["workDecision"]["workTaskId"],page["items"][0]["workTaskId"]);
        // A cached stop label alone is not current no-progress evidence.
        // The Native episode projection must re-evaluate actual failed attempts.
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE story_generation_runs_v1 SET state_json=json_set(state_json,'$.status','blocked','$.blockedReason','recoveryStalled') WHERE run_id=?1",
            [run["id"].as_str().unwrap()]).unwrap();
        let (_, projected) = call(&app,"GET",&format!("{base}/work-tasks/{}",page["items"][0]["workTaskId"].as_str().unwrap()),Value::Null,AUTHOR).await;
        assert_eq!(projected["resumeBlockReason"],Value::Null,"{projected}");
    }
}

#[tokio::test]
async fn execution_conditions_are_host_owned_and_do_not_resume_author_stops() {
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("conditions.sqlite3");let app=app(&path);
    let (_,base,run,_)=generation_root(&app).await;
    let work=format!("work-{}",run["id"].as_str().unwrap());
    let endpoint=format!("{base}/work-tasks/{work}/execution-condition");
    let digest="a".repeat(64);
    assert_eq!(call(&app,"POST",&endpoint,json!({"digest":digest}),CANDIDATE).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app,"POST",&endpoint,json!({"digest":"model says repaired"}),AUTHOR).await.0,StatusCode::BAD_REQUEST);
    let before=call(&app,"GET",&format!("{base}/work-tasks/{work}"),Value::Null,AUTHOR).await.1;
    for _ in 0..2 { assert_eq!(call(&app,"POST",&endpoint,json!({"digest":digest}),AUTHOR).await.0,StatusCode::OK); }
    let after=call(&app,"GET",&format!("{base}/work-tasks/{work}"),Value::Null,AUTHOR).await.1;
    assert_eq!(before,after,"condition registration grants no execution or state transition");
    let run_path=format!("{base}/generation-runs/{}",run["id"].as_str().unwrap());
    let current=call(&app,"GET",&run_path,Value::Null,AUTHOR).await.1;
    assert_eq!(call(&app,"POST",&format!("{run_path}/control"),json!({"expectedVersion":current["version"],"command":"pause"}),AUTHOR).await.0,StatusCode::OK);
    assert_eq!(call(&app,"POST",&endpoint,json!({"digest":"b".repeat(64)}),AUTHOR).await.0,StatusCode::OK);
    assert_eq!(call(&app,"GET",&run_path,Value::Null,AUTHOR).await.1["status"],"paused");
    assert_eq!(call(&app,"POST",&format!("{base}/work-tasks/nonexistent/execution-condition"),json!({"digest":digest}),AUTHOR).await.0,StatusCode::NOT_FOUND);
}
