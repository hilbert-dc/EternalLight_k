use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, RepositoryError, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "authoring-author-token-1234567890123456";
const CANDIDATE: &str = "authoring-candidate-token-1234567890123";
const CAPABILITY: &str = "authoring-task-secret-12345678901234567890";

#[path = "support/authoring_commitments.rs"]
mod commitments;

#[path = "support/generation_delivery.rs"]
mod delivery;

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    candidate: bool,
) -> (StatusCode, Value) {
    let mut payload = payload;
    if method == "POST"
        && uri.ends_with("/generation-runs")
        && let Some(body) = payload.as_mut()
        && body.get("expectedBaseSnapshotId").is_none()
    {
        let workspace_uri = format!(
            "{}/creative-workspace",
            uri.trim_end_matches("/generation-runs")
        );
        let (_, workspace) = Box::pin(call(path, "GET", &workspace_uri, None, false)).await;
        body["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
        body["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    }
    let app: Router = router(
        task_fixture::state(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}

async fn charge_budget(path: &std::path::Path, run_uri: &str, task_id: &str) -> Value {
    let mut budget = PersistentBudget::new(
        &path.with_extension("budget.sqlite3"),
        task_id,
        json!({"options":{"id":format!("budget-{task_id}")},"now":0}),
    )
    .unwrap();
    budget.complete("terminal").unwrap();
    let (status, run) = call(
        path,
        "POST",
        &format!("{run_uri}/provider-usage"),
        Some(json!({"taskId":task_id})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    run
}

#[tokio::test]
async fn generation_start_rejects_stale_candidate_sources_even_when_formal_revision_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stale-start.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"候选改稿防旧许可"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}");
    let (_, before) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        None,
        false,
    )
    .await;
    let (status, brief) = call(
        &path,
        "POST",
        &format!("{base}/artifacts/brief"),
        Some(
            json!({"expectedRevision":before["revision"],"expectedCandidateRevisionId":null,
            "idempotencyKey":"stale-start-brief","payload":{"premise":"林夕记录港城的遗忘。"}}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{brief}");
    let (_, after) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(before["revision"], after["revision"]);
    assert_ne!(before["headSnapshotId"], after["headSnapshotId"]);
    assert_ne!(before["sourceDigest"], after["sourceDigest"]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut permit = json!({"expectedProjectRevision":after["revision"],
        "expectedBaseSnapshotId":before["headSnapshotId"],"expectedSourceDigest":after["sourceDigest"],
        "authorIntent":"沿最新正文继续","targetChapters":1,"maxTaskAttempts":7,
        "maxProviderRequests":80,"maxProviderTokens":2000000,"deadlineAt":now+600});
    let runs_uri = format!("{base}/generation-runs");
    let (status, rejected) = call(&path, "POST", &runs_uri, Some(permit.clone()), false).await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    permit["expectedBaseSnapshotId"] = after["headSnapshotId"].clone();
    permit["expectedSourceDigest"] = before["sourceDigest"].clone();
    let (status, rejected) = call(&path, "POST", &runs_uri, Some(permit.clone()), false).await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    permit["expectedSourceDigest"] = after["sourceDigest"].clone();
    let (status, run) = call(&path, "POST", &runs_uri, Some(permit), false).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["baseSnapshotId"], after["headSnapshotId"]);
    assert_eq!(run["sourceDigest"], after["sourceDigest"]);
}

#[tokio::test]
async fn existing_candidate_plan_requires_intent_reconciliation_before_first_chapter() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing-plan.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"已有近章规划的港城"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}");
    let (status, plan) = call(
        &path,
        "POST",
        &format!("{base}/artifacts/plan"),
        Some(json!({"payload":{"chapters":[{"chapterPlanId":"plan-1",
            "chapterNumber":1,"goal":"追查白塔记录","scenes":[{"id":"scene-1",
            "goal":"看见灯光","beats":[],"expectedEffects":[{"id":"effect-1",
            "dimension":"knowledge","description":"林夕看见白塔灯光",
            "commitmentRefs":[]}]}]}]},"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":"existing-plan"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{plan}");
    let (status, position) = call(
        &path,
        "GET",
        &format!("{base}/generation-start-position"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{position}");
    assert_eq!(
        position,
        json!({"completedChapters":0,"firstStage":"rollingPlan"})
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("{base}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(
            json!({"expectedProjectRevision":0,"authorIntent":"改为海上逃亡，旧白塔规划不能直接照搬",
            "targetChapters":1,"maxTaskAttempts":11,"maxProviderRequests":80,
            "maxProviderTokens":2000000,"deadlineAt":now+600}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["stage"], "rollingPlan");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generatePlan");
    assert_eq!(next["action"]["target"]["planFromChapter"], 1);
    assert_eq!(
        next["action"]["authorIntent"],
        "[要求版本 0] 改为海上逃亡，旧白塔规划不能直接照搬"
    );
    let (status, workspace) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    assert!(workspace["chapters"].as_array().unwrap().is_empty());
    assert!(
        workspace["chapterPlanBindings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        workspace["artifacts"]["plan"]["candidate"]["id"],
        plan["revisionId"]
    );
    let thread_id = "existing-plan-reconcile";
    let (status, task) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        Some(
            json!({"operation":"generatePlan","inputRevision":workspace["revision"],
            "threadId":thread_id,"target":next["action"]["target"],
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":now+600,"authorIntent":"改为海上逃亡，旧白塔规划不能直接照搬"}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let (status, context) = call(
        &path,
        "GET",
        &format!("{base}/context?taskId={}&threadId={thread_id}&projectRevision={}&purpose=planner&budget=24&byteBudget=60000",
            task["id"].as_str().unwrap(),workspace["revision"].as_i64().unwrap()),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["plannedEffects"][0]["value"]["sourceRevisionId"],
        plan["revisionId"]
    );
    assert_eq!(
        context["plannedEffects"][0]["value"]["chapters"][0]["goal"],
        "追查白塔记录"
    );
}

#[tokio::test]
async fn existing_chapter_continues_without_forcing_brief_or_full_outline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing-chapter.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"已有正文的港城"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}");
    let body = "林夕站在白塔前。";
    let (status, draft) = call(
        &path,
        "POST",
        &format!("{base}/drafts/checkpoints"),
        Some(json!({"payload":{"chapterNumber":1,"title":"灯","body":body},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"existing-draft"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, batch) = call(
        &path,
        "POST",
        &format!("{base}/modeling-batches"),
        Some(json!({"objectId":format!("draft-{}",draft["chapterId"].as_str().unwrap()),
            "sourceRevisionId":draft["revisionId"],"expectedRevision":0,"extractorVersion":"existing-chapter-test"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status, task) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        Some(json!({"operation":"modelKnowledge","inputRevision":0,"threadId":"existing-model",
            "target":{"batchId":batch["id"]},"authorIntent":"提取已有正文证据",
            "capabilityExpiresAt":now+600,"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes()))})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let evidence = json!({"quote":"林夕","start":0,"end":2});
    let (status, published) = call(
        &path,
        "POST",
        &format!("{base}/modeling-batches/{}/publish", batch["id"].as_str().unwrap()),
        Some(json!({"taskId":task["id"],"threadId":"existing-model","expectedRevision":0,
            "entities":[{"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":evidence}],
            "assertions":[{"kind":"assertion","subjectId":"linxi","predicate":"atLamp","value":true,"evidence":evidence}]})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{}/complete", task["id"].as_str().unwrap()),
        Some(json!({"threadId":"existing-model","summary":"已有正文证据就绪"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let (status, workspace) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    assert!(workspace["artifacts"]["brief"]["candidate"].is_null());
    assert!(workspace["artifacts"]["outline"]["candidate"].is_null());
    let runs_uri = format!("{base}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({"expectedProjectRevision":workspace["revision"],
            "authorIntent":"第二章继续调查白塔","targetChapters":1,
            "maxTaskAttempts":11,"maxProviderRequests":80,"maxProviderTokens":2000000,
            "deadlineAt":now+600})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["stage"], "rollingPlan");
    assert_eq!(run["startChapterNumber"], 2);
    let (status, next) = call(
        &path,
        "POST",
        &format!("{runs_uri}/{}/next", run["id"].as_str().unwrap()),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generatePlan");
    assert_eq!(next["action"]["target"]["planFromChapter"], 2);
}

#[tokio::test]
async fn existing_outline_can_begin_rolling_plan_without_a_brief() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing-outline.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"已有大纲的港城"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}");
    let (status, outline) = call(
        &path,
        "POST",
        &format!("{base}/artifacts/outline"),
        Some(json!({"payload":{"nodes":[{"id":"book-1","kind":"book","position":1,
            "title":"白塔","purpose":"保留证词","summary":"港城逐渐遗忘海上人名",
            "dependencyKeys":[],"certainty":"tentative","protection":"free"}]},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"existing-outline"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{outline}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("{base}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(
            json!({"expectedProjectRevision":0,"authorIntent":"从大纲写第一章",
            "targetChapters":1,"maxTaskAttempts":11,"maxProviderRequests":80,
            "maxProviderTokens":2000000,"deadlineAt":now+600}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["stage"], "rollingPlan");
    let (status, next) = call(
        &path,
        "POST",
        &format!("{runs_uri}/{}/next", run["id"].as_str().unwrap()),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generatePlan");
    assert_eq!(next["action"]["target"]["planFromChapter"], 1);
}

#[tokio::test]
async fn generation_permit_includes_automatic_commitment_extraction_in_its_minimum_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commitment-budget.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"承诺预算"})),
        /*candidate*/ false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let uri = format!("/api/projects/{project}/generation-runs");
    let mut permit = json!({"expectedProjectRevision":0,"authorIntent":"写前三章并处理叙事承诺提案","targetChapters":3,"maxTaskAttempts":16,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600});
    let (status, rejected) = call(
        &path,
        "POST",
        &uri,
        Some(permit.clone()),
        /*candidate*/ false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let (_, list) = call(
        &path, "GET", &uri, /*payload*/ None, /*candidate*/ false,
    )
    .await;
    assert_eq!(list["runs"], json!([]));
    permit["maxTaskAttempts"] = json!(17);
    let (status, run) = call(&path, "POST", &uri, Some(permit), /*candidate*/ false).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["usedTaskAttempts"], 0);
    assert_eq!(run["maxTaskAttempts"], 17);
}

#[tokio::test]
async fn generation_blocks_when_formal_chapter_directory_changes_without_new_object_head() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("generation-directory.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title": "港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("/api/projects/{project_id}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(
            json!({"expectedProjectRevision":0,"authorIntent":"记忆消失的港城",
            "targetChapters":1,"maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let (status, chapter) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/chapters/materialize-standalone"),
        Some(json!({"title":"作者新建的一章","expectedRevision":0,"idempotencyKey":"external-chapter"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chapter}");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["run"]["status"], "blocked");
    assert_eq!(next["run"]["blockedReason"], "source_heads_changed");
    assert!(next["action"].is_null());
    let (status, cancelled) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": next["run"]["version"], "command": "cancel"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    let mut stale_state = run.clone();
    stale_state["id"] = json!("generation-stale-start");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse(project_id.to_owned()).unwrap();
    assert!(matches!(
        repository.create_generation_run(
            &project,
            "generation-stale-start",
            &stale_state.to_string()
        ),
        Err(RepositoryError::InvalidContent(_))
    ));
    assert!(matches!(
        repository.generation_run(&project, "generation-stale-start"),
        Err(RepositoryError::AuthoringRunNotFound)
    ));
    let (status, unready) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 1, "authorIntent": "接着写下一章",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{unready}");
}

#[tokio::test]
async fn generation_run_binds_a_codex_task_and_recovers_after_output_before_advancement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authoring.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title": "Harbor"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("/api/projects/{project_id}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "写一部关于记忆的小说",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["stage"], "brief");
    let (status, listed) = call(&path, "GET", &runs_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["runs"].as_array().unwrap().len(), 1);
    assert_eq!(listed["runs"][0]["id"], run["id"]);
    let (status, duplicate) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "另一条重复运行",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{duplicate}");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let (status, _) = call(&path, "GET", &run_uri, None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, action) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{action}");
    assert_eq!(action["action"]["operation"], "generateBrief");
    assert_eq!(action["action"]["target"]["runVersion"], 0);

    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "authoring-brief-thread", "target": action["action"]["target"],
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": now + 600, "authorIntent": "写一部关于记忆的小说"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    assert_eq!(task["threadId"], "authoring-brief-thread");
    assert_eq!(task["inputRevision"], 0);
    assert_eq!(task["requiredOutputKind"], "brief");
    assert_eq!(task["thread_id"], Value::Null);
    let (status, atomically_claimed) = call(&path, "GET", &run_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{atomically_claimed}");
    assert_eq!(atomically_claimed["activeTaskId"], task_id);
    assert_eq!(atomically_claimed["version"], 1);
    assert_eq!(atomically_claimed["usedTaskAttempts"], 1);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{run_uri}/advance-task"),
        Some(json!({
            "expectedVersion": 1, "taskId": task_id
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{run_uri}/future-intents"),
        Some(json!({
            "expectedVersion": 1, "instruction": "中途更改正在生成的章节"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, waiting) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(waiting["action"]["kind"], "awaitTask");

    let (status, draft) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload": {"premise": "港城正在失去记忆"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": null, "idempotencyKey": "authoring-brief-1",
            "taskId": task_id, "threadId": "authoring-brief-thread"
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, completed) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{task_id}/complete"),
        Some(json!({
            "threadId": "authoring-brief-thread", "summary": "brief saved"
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let charged = charge_budget(&path, &run_uri, task_id).await;
    let (status, turn_bound) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{task_id}/turn"),
        Some(json!({"threadId": "authoring-brief-thread", "turnId": "turn-after-mcp-completion"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{turn_bound}");
    assert_eq!(turn_bound["turnId"], "turn-after-mcp-completion");
    let (status, still_waiting) =
        call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_waiting["action"]["taskStatus"], "completed");
    let (status, advanced) = call(
        &path,
        "POST",
        &format!("{run_uri}/advance-task"),
        Some(json!({
            "expectedVersion": charged["version"], "taskId": task_id
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{advanced}");
    assert_eq!(advanced["stage"], "outline");
    assert_eq!(advanced["usedTaskAttempts"], 1);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{run_uri}/advance-task"),
        Some(json!({
            "expectedVersion": 1, "taskId": task_id
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generateOutline");
    let (status, changed) = call(
        &path,
        "POST",
        &format!("{run_uri}/future-intents"),
        Some(json!({
            "expectedVersion": next["run"]["version"], "instruction": "让林夕的选择推动情节"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["intentGeneration"], 1);
    assert_eq!(changed["intentRevisions"].as_array().unwrap().len(), 2);
    let (status, revised_action) =
        call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{revised_action}");
    assert_eq!(revised_action["action"]["target"]["intentGeneration"], 1);
    assert_ne!(
        revised_action["action"]["target"]["stepToken"],
        next["action"]["target"]["stepToken"]
    );
    assert!(
        revised_action["action"]["authorIntent"]
            .as_str()
            .unwrap()
            .contains("林夕的选择")
    );
    let (status, stale_task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateOutline", "inputRevision": 0,
            "threadId": "outdated-outline-thread", "target": next["action"]["target"],
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": now + 600, "authorIntent": "写一部关于记忆的小说"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(stale_task["error"]["message"], "TaskBindingInvalid");
}

#[tokio::test]
async fn cancelling_a_generation_revokes_its_active_task_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancel-generation.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title": "Harbor"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let project = ProjectId::parse(project_id.to_owned()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("/api/projects/{project_id}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "写一部关于记忆的小说",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let (_, action) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "cancel-brief-thread", "target": action["action"]["target"],
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": now + 600, "authorIntent": "写一部关于记忆的小说"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();

    let (status, stale) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": 0, "command": "cancel"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository.task(&project, task_id).unwrap().unwrap().status,
        "queued"
    );

    let (status, cancelled) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": 1, "command": "cancel"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");
    let repository = SqliteStoryRepository::open(&path).unwrap();
    let cancelled_task = repository.task(&project, task_id).unwrap().unwrap();
    assert_eq!(cancelled_task.status, "cancelled");
    assert!(cancelled_task.capability_revoked_at.is_some());
    let (status, late_write) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload": {"premise": "迟到的任务不能写入"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": null, "idempotencyKey": "late-brief-after-cancel",
            "taskId": task_id, "threadId": "cancel-brief-thread"
        })),
        true,
    )
    .await;
    assert_ne!(status, StatusCode::CREATED, "{late_write}");
    assert_eq!(
        repository
            .generation_run(&project, run["id"].as_str().unwrap())
            .unwrap()
            .version,
        2
    );
}

#[tokio::test]
async fn paused_generation_reconciles_its_own_completed_output_on_resume() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paused-authoring.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title": "Harbor"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("/api/projects/{project_id}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "写一部关于记忆的小说",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let (_, action) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "paused-brief-thread", "target": action["action"]["target"],
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": now + 600, "authorIntent": "写一部关于记忆的小说"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (status, paused) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": 1, "command": "pause"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    let (status, draft) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload": {"premise": "港城正在失去记忆"}, "expectedRevision": 0,
            "expectedCandidateRevisionId": null, "idempotencyKey": "paused-brief-save",
            "taskId": task_id, "threadId": "paused-brief-thread"
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, completed) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{task_id}/complete"),
        Some(json!({"threadId": "paused-brief-thread", "summary": "brief saved"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let charged = charge_budget(&path, &run_uri, task_id).await;
    let (status, resumed) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": charged["version"], "command": "resume"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert_eq!(resumed["status"], "running");
    assert_eq!(resumed["stage"], "outline");
    assert_eq!(resumed["activeTaskId"], Value::Null);
    assert_eq!(resumed["usedTaskAttempts"], 1);
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generateOutline");
}

#[tokio::test]
async fn failed_agent_task_blocks_run_instead_of_waiting_forever() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("failed-authoring.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title": "Harbor"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("/api/projects/{project_id}/generation-runs");
    let (_, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "写一部关于记忆的小说",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let (_, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "failed-brief-thread", "target": next["action"]["target"],
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": now + 600, "authorIntent": "写一部关于记忆的小说"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (status, failed) = call(
        &path,
        "PATCH",
        &format!("/api/projects/{project_id}/tasks/{task_id}"),
        Some(json!({
            "threadId": "failed-brief-thread", "status": "failed", "error": "model disconnected"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    charge_budget(&path, &run_uri, task_id).await;
    let (status, blocked) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{blocked}");
    assert_eq!(blocked["run"]["status"], "blocked");
    assert_eq!(blocked["run"]["blockedReason"], "agent_task_failed");
    assert_eq!(blocked["run"]["usedTaskAttempts"], 1);
    assert!(blocked["action"].is_null());
    let (status, duplicate) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "不应绕过仍可重试的运行",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{duplicate}");
    let (status, retried) = call(
        &path,
        "POST",
        &format!("{run_uri}/retry-failed-task"),
        Some(json!({
            "expectedVersion": blocked["run"]["version"]
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{retried}");
    assert_eq!(retried["status"], "running");
    assert_eq!(retried["retryAttempts"], 1);
    assert_eq!(retried["usedTaskAttempts"], 1);
    let (_, retry_action) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(retry_action["action"]["operation"], "generateBrief");
    assert_ne!(
        retry_action["action"]["target"]["stepToken"],
        next["action"]["target"]["stepToken"]
    );
    let (status, retry_task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateBrief", "inputRevision": 0,
            "threadId": "retried-brief-thread", "target": retry_action["action"]["target"],
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": now + 600, "authorIntent": "写一部关于记忆的小说"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{retry_task}");
    let (_, charged_retry) = call(&path, "GET", &run_uri, None, false).await;
    assert_eq!(charged_retry["usedTaskAttempts"], 2);
    let (status, work_task) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/tasks/{task_id}/work-task"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{work_task}");
    assert_eq!(work_task["steps"].as_array().unwrap().len(), 1);
    assert_eq!(work_task["steps"][0]["status"], "running");
    assert_eq!(
        work_task["steps"][0]["attempts"].as_array().unwrap().len(),
        2
    );
    assert_eq!(work_task["steps"][0]["attempts"][0]["id"], task_id);
    assert_eq!(work_task["steps"][0]["attempts"][1]["id"], retry_task["id"]);
    let (status, cancelled) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": charged_retry["version"], "command": "cancel"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    let (status, replacement) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "作者显式取消后重新开始",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{replacement}");
    assert_ne!(replacement["id"], run["id"]);
}

#[tokio::test]
async fn one_intent_advances_through_three_chapters_only_after_bound_codex_outputs() {
    three_chapters_with_feedback("  请补足第二场的证据🌙\n不改结局  ").await;
}

#[tokio::test]
async fn feedback_without_author_note_is_a_decision_not_an_invented_message() {
    three_chapters_with_feedback("").await;
}

async fn three_chapters_with_feedback(repair_instruction: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("three-chapters.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title": "港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let runs_uri = format!("/api/projects/{project_id}/generation-runs");
    let (status, run) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": 0, "authorIntent": "记忆逐渐消失的港城",
            "targetChapters": 3, "maxTaskAttempts": 24,"maxProviderRequests":64,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let mut draft_revisions = std::collections::HashMap::new();
    let mut assessment_revisions = std::collections::HashMap::new();
    let mut plan_revision: Option<String> = None;
    let mut operations = Vec::new();
    let mut repaired_first_chapter = false;
    let mut failed_feedback_repair_once = false;

    for step in 0..40 {
        let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
        assert_eq!(status, StatusCode::OK, "{next}");
        if next["run"]["status"] == "completed" {
            break;
        }
        let action = &next["action"];
        let version = next["run"]["version"].as_i64().unwrap();
        match action["kind"].as_str().unwrap() {
            "materializeChapter" | "checkKnowledge" => {
                let endpoint = if action["kind"] == "materializeChapter" {
                    "materialize"
                } else {
                    "check"
                };
                let (status, result) = call(
                    &path,
                    "POST",
                    &format!("{run_uri}/{endpoint}"),
                    Some(json!({"expectedVersion": version})),
                    false,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{result}");
                if endpoint == "check" {
                    assert_eq!(result["status"], "blocked", "{result}");
                    assert_eq!(result["blockedReason"], "scene_feedback_needs_author");
                    let chapter_id = next["run"]["currentChapterId"].as_str().unwrap();
                    let (status, assessment) = call(
                        &path,
                        "GET",
                        &format!(
                            "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments"
                        ),
                        None,
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{assessment}");
                    let (status, rejected) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/resolve-feedback"),
                        Some(json!({"expectedVersion": result["version"],
                            "assessmentRevisionId": "old-assessment", "choice":"continue", "instruction":""})),
                        false,
                    ).await;
                    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
                    let repair_first =
                        !repaired_first_chapter && next["run"]["currentChapterNumber"] == 1;
                    if next["run"]["currentChapterNumber"] == 3 {
                        let (status, budget_rejected) = call(
                            &path,
                            "POST",
                            &format!("{run_uri}/resolve-feedback"),
                            Some(json!({"expectedVersion": result["version"],
                                "assessmentRevisionId": assessment["revisionId"],
                                "choice":"repairDraft", "instruction":""})),
                            false,
                        )
                        .await;
                        assert_eq!(status, StatusCode::BAD_REQUEST, "{budget_rejected}");
                    }
                    let choice = if repair_first {
                        "repairDraft"
                    } else {
                        "continue"
                    };
                    let db = rusqlite::Connection::open(&path).unwrap();
                    let message_count: i64 = db
                        .query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
                            row.get(0)
                        })
                        .unwrap();
                    if repair_first {
                        let decision_request = json!({"expectedVersion":result["version"],
                            "assessmentRevisionId":assessment["revisionId"],"choice":choice,"instruction":repair_instruction});
                        assert_eq!(
                            call(
                                &path,
                                "POST",
                                &format!("{run_uri}/resolve-feedback"),
                                Some(decision_request.clone()),
                                /*candidate*/ true
                            )
                            .await
                            .0,
                            StatusCode::UNAUTHORIZED
                        );
                        let mut injected = decision_request.clone();
                        injected["decisionId"] = json!("forged-decision");
                        assert_eq!(
                            call(
                                &path,
                                "POST",
                                &format!("{run_uri}/resolve-feedback"),
                                Some(injected),
                                /*candidate*/ false
                            )
                            .await
                            .0,
                            StatusCode::UNPROCESSABLE_ENTITY
                        );
                        for table in [
                            "story_author_messages_v1",
                            "story_generation_runs_v1",
                            "story_work_tasks_v1",
                        ] {
                            if table == "story_author_messages_v1" && repair_instruction.is_empty()
                            {
                                continue;
                            }
                            let operation = if table == "story_author_messages_v1" {
                                "INSERT"
                            } else {
                                "UPDATE"
                            };
                            let work_before: (i64,String) = db.query_row("SELECT version,(SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=w.id) FROM story_work_tasks_v1 w WHERE id=?1",[format!("work-{}",run["id"].as_str().unwrap())],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
                            db.execute_batch(&format!("CREATE TRIGGER reject_feedback BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'feedback failure'); END;")).unwrap();
                            let (status, rejected) = call(
                                &path,
                                "POST",
                                &format!("{run_uri}/resolve-feedback"),
                                Some(decision_request.clone()),
                                /*candidate*/ false,
                            )
                            .await;
                            assert!(!status.is_success(), "{table}: {rejected}");
                            assert_eq!(
                                call(
                                    &path, "GET", &run_uri, /*payload*/ None,
                                    /*candidate*/ false
                                )
                                .await,
                                (StatusCode::OK, result.clone())
                            );
                            let work_after: (i64,String) = db.query_row("SELECT version,(SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=w.id) FROM story_work_tasks_v1 w WHERE id=?1",[format!("work-{}",run["id"].as_str().unwrap())],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
                            assert_eq!(work_after, work_before);
                            assert_eq!(
                                db.query_row(
                                    "SELECT COUNT(*) FROM story_author_messages_v1",
                                    [],
                                    |row| row.get::<_, i64>(0)
                                )
                                .unwrap(),
                                message_count
                            );
                            db.execute_batch("DROP TRIGGER reject_feedback;").unwrap();
                        }
                    }
                    let (status, accepted) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/resolve-feedback"),
                        Some(json!({"expectedVersion": result["version"],
                            "assessmentRevisionId": assessment["revisionId"], "choice":choice,
                            "instruction":if repair_first { repair_instruction } else { "" }})),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{accepted}");
                    assert_eq!(
                        accepted["feedbackDecisions"]
                            .as_array()
                            .unwrap()
                            .last()
                            .unwrap()["choice"],
                        choice
                    );
                    if repair_first {
                        assert_eq!(accepted["stage"], "draft");
                        assert_eq!(accepted["feedbackRepairCount"], 1);
                        let decision = accepted["feedbackDecisions"]
                            .as_array()
                            .unwrap()
                            .last()
                            .unwrap();
                        let intent = accepted["intentRevisions"]
                            .as_array()
                            .unwrap()
                            .last()
                            .unwrap();
                        assert_eq!(
                            intent["source"],
                            json!({"type":"feedbackDecision","decisionId":decision["id"]})
                        );
                        assert_eq!(intent["instruction"], repair_instruction);
                        assert_eq!(decision["instruction"], repair_instruction);
                        assert_eq!(decision["chapterNumber"], 1);
                        assert_eq!(accepted["providerBudget"], result["providerBudget"]);
                        if repair_instruction.is_empty() {
                            assert_eq!(decision["authorMessageId"], Value::Null);
                        } else {
                            let raw: (String, i64, i64) = db.query_row("SELECT author_text,intent_revision,created_at FROM story_author_messages_v1 WHERE message_id=?1",
                                [decision["authorMessageId"].as_str().unwrap()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
                            assert_eq!(
                                raw,
                                (
                                    repair_instruction.into(),
                                    intent["generation"].as_i64().unwrap(),
                                    decision["decidedAt"].as_i64().unwrap()
                                )
                            );
                        }
                        assert_eq!(
                            db.query_row(
                                "SELECT COUNT(*) FROM story_author_messages_v1",
                                [],
                                |row| row.get::<_, i64>(0)
                            )
                            .unwrap(),
                            message_count + i64::from(!repair_instruction.is_empty())
                        );
                        let mut repo = SqliteStoryRepository::open(&path).unwrap();
                        let project = ProjectId::parse(project_id).unwrap();
                        let saved = repo
                            .generation_run(&project, accepted["id"].as_str().unwrap())
                            .unwrap();
                        let mut forged: Value = serde_json::from_str(&saved.state_json).unwrap();
                        forged["version"] = json!(saved.version + 1);
                        forged["feedbackDecisions"][0]["instruction"] = json!("模型改写作者决定");
                        assert!(
                            repo.update_generation_run(
                                &project,
                                &saved.run_id,
                                saved.version,
                                &forged.to_string()
                            )
                            .is_err()
                        );
                        assert_eq!(repo.generation_run(&project, &saved.run_id).unwrap(), saved);
                        let (status, repair_next) =
                            call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
                        assert_eq!(status, StatusCode::OK, "{repair_next}");
                        assert_eq!(repair_next["action"]["operation"], "generateChapter");
                        let author_intent = repair_next["action"]["authorIntent"].as_str().unwrap();
                        assert!(author_intent.contains("按本章场景评估修订工作稿"));
                        if !repair_instruction.is_empty() {
                            assert!(author_intent.contains(repair_instruction));
                        }
                        assert_eq!(
                            repair_next["action"]["candidateInstruction"],
                            accepted["feedbackRepairPrompt"]
                        );
                        repaired_first_chapter = true;
                        continue;
                    }
                    assert_eq!(
                        db.query_row("SELECT COUNT(*) FROM story_author_messages_v1", [], |row| {
                            row.get::<_, i64>(0)
                        })
                        .unwrap(),
                        message_count
                    );
                    delivery::before_commit(&path, project_id, &accepted);
                    let (status, checked) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/check"),
                        Some(json!({"expectedVersion": accepted["version"]})),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{checked}");
                    assert_ne!(checked["status"], "blocked", "{checked}");
                    delivery::after_commit(&path, project_id, &accepted, &checked);
                    continue;
                }
                assert_ne!(result["status"], "blocked", "{result}");
            }
            "agentTask" => {
                let operation = action["operation"].as_str().unwrap();
                operations.push(operation.to_owned());
                if operation == "generatePlan" {
                    assert_eq!(
                        action["target"]["planFromChapter"],
                        next["run"]["completedChapters"].as_i64().unwrap() + 1
                    );
                    if action["target"]["planFromChapter"] == 2 {
                        assert!(
                            !action["authorIntent"]
                                .as_str()
                                .unwrap()
                                .contains("作者决定版本")
                        );
                        assert!(
                            action["authorIntent"]
                                .as_str()
                                .unwrap()
                                .contains("第二章转向调查白塔")
                        );
                    }
                }
                if operation == "generateChapter"
                    && action["target"]["chapterNumber"].as_i64() == Some(2)
                {
                    assert!(
                        action["authorIntent"]
                            .as_str()
                            .unwrap()
                            .contains("第二章转向调查白塔")
                    );
                }
                let (_, workspace) = call(
                    &path,
                    "GET",
                    &format!("/api/projects/{project_id}/creative-workspace"),
                    None,
                    false,
                )
                .await;
                let revision = workspace["revision"].as_i64().unwrap();
                let mut target = action["target"].clone();
                let chapter_id = target["chapterId"].as_str().map(str::to_owned);
                let chapter_number = target["chapterNumber"].as_i64();
                let first_chapter_draft = operation == "generateChapter"
                    && chapter_number == Some(1)
                    && draft_revisions.is_empty();
                let second_chapter_first_draft = operation == "generateChapter"
                    && chapter_number == Some(2)
                    && chapter_id
                        .as_ref()
                        .is_some_and(|id| !draft_revisions.contains_key(id));
                let feedback_repair_first_draft = operation == "generateChapter"
                    && chapter_number == Some(1)
                    && next["run"]["feedbackRepairCount"] == 1
                    && !failed_feedback_repair_once;
                if operation == "generateChapter"
                    && chapter_number == Some(2)
                    && !second_chapter_first_draft
                {
                    assert!(
                        action["authorIntent"]
                            .as_str()
                            .unwrap()
                            .contains("第二章先找到航海日志")
                    );
                }
                if operation == "generateChapter"
                    && chapter_number == Some(1)
                    && chapter_id
                        .as_ref()
                        .is_some_and(|id| draft_revisions.contains_key(id))
                {
                    assert!(
                        action["authorIntent"]
                            .as_str()
                            .unwrap()
                            .contains("第一章补写白塔证据")
                    );
                }
                let batch_id = if operation == "modelKnowledge" {
                    let chapter_id = chapter_id.as_deref().unwrap();
                    let (status, batch) = call(
                        &path,
                        "POST",
                        &format!("/api/projects/{project_id}/modeling-batches"),
                        Some(json!({
                            "objectId": format!("draft-{chapter_id}"),
                            "sourceRevisionId": draft_revisions[chapter_id],
                            "expectedRevision": revision, "extractorVersion": "test-v1"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::CREATED, "{batch}");
                    let id = batch["id"].as_str().unwrap().to_owned();
                    target["batchId"] = json!(id);
                    Some(id)
                } else {
                    None
                };
                let thread_id = format!("generation-step-{step}");
                if operation == "extractCommitments" {
                    let mut wrong_target = target.clone();
                    wrong_target["chapterNumber"] = json!(chapter_number.unwrap() + 1);
                    let (status,rejected)=call(&path,"POST",&format!("/api/projects/{project_id}/tasks"),Some(json!({
                        "operation":operation,"inputRevision":revision,"threadId":"wrong-chapter",
                        "target":wrong_target,"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
                        "capabilityExpiresAt":now+600,"authorIntent":"检验错误章节领取"
                    })),/*candidate*/ false).await;
                    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
                    let (_, unchanged) = call(
                        &path, "GET", &run_uri, /*payload*/ None, /*candidate*/ false,
                    )
                    .await;
                    assert_eq!(
                        unchanged["usedTaskAttempts"],
                        next["run"]["usedTaskAttempts"]
                    );
                    assert!(unchanged["activeTaskId"].is_null());
                }
                let (status, task) = call(&path, "POST", &format!("/api/projects/{project_id}/tasks"), Some(json!({
                    "operation": operation, "inputRevision": revision, "threadId": thread_id,
                    "target": target, "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                    "capabilityExpiresAt": now + 600, "authorIntent": "记忆逐渐消失的港城"
                })), false).await;
                assert_eq!(
                    status,
                    StatusCode::CREATED,
                    "step {step} {operation}: {task}; run={}; target={target}",
                    next["run"]
                );
                let task_id = task["id"].as_str().unwrap();
                assert_eq!(
                    task["inputManifest"]["authorIntent"],
                    action["authorIntent"]
                );
                assert_eq!(
                    task["inputManifest"]["candidateInstruction"],
                    action["candidateInstruction"]
                );
                let (status, claimed) = call(&path, "GET", &run_uri, None, false).await;
                assert_eq!(status, StatusCode::OK, "{operation}: {claimed}");
                assert_eq!(claimed["version"], version + 1);
                assert_eq!(claimed["activeTaskId"], task_id);
                if operation == "generatePlan"
                    && action["target"]["planFromChapter"].as_i64().unwrap() > 1
                {
                    let (status, context) = call(
                        &path,
                        "GET",
                        &format!(
                            "/api/projects/{project_id}/context?taskId={task_id}&threadId={thread_id}&projectRevision={revision}&purpose=planner&budget=24&byteBudget=60000"
                        ),
                        None,
                        true,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{context}");
                    let knowledge = &context["operationContext"]["workingKnowledge"];
                    let proposals = context["candidateCommitmentProposals"].as_array().unwrap();
                    assert_eq!(
                        proposals.len(),
                        action["target"]["planFromChapter"].as_u64().unwrap() as usize - 1
                    );
                    assert!(proposals.iter().all(|proposal| {
                        proposal["chapterNumber"].as_i64().unwrap()
                            < action["target"]["planFromChapter"].as_i64().unwrap()
                            && proposal["authority"] == "candidateProposal"
                            && proposal["sourceCurrent"] == true
                    }));
                    assert!(proposals.iter().any(|proposal| {
                        draft_revisions
                            .values()
                            .any(|revision| proposal["sourceRevisionId"] == *revision)
                    }));
                    assert_eq!(knowledge["coverage"]["complete"], true, "{context}");
                    let entries = knowledge["entries"].as_array().unwrap();
                    assert!(
                        !entries.is_empty(),
                        "rolling plan lost completed chapter knowledge"
                    );
                    assert!(entries.iter().all(|entry| {
                        entry["sourceChapter"].as_i64().unwrap()
                            < action["target"]["planFromChapter"].as_i64().unwrap()
                    }));
                    let links = context["operationContext"]["scenePlanDraftLinks"]
                        .as_array()
                        .unwrap();
                    let previous_chapter =
                        action["target"]["planFromChapter"].as_i64().unwrap() - 1;
                    let previous = links
                        .iter()
                        .find(|link| link["chapterNumber"] == previous_chapter)
                        .expect(
                            "rolling planner must receive the previous scene's plan/draft lineage",
                        );
                    assert_eq!(previous["sceneId"], format!("scene-{previous_chapter}"));
                    assert_eq!(
                        previous["plannedExpectedEffects"][0]["commitmentRefs"],
                        if previous_chapter > 1 {
                            commitments::plan_refs(&path, project_id, previous_chapter - 1)
                        } else {
                            json!([])
                        }
                    );
                    assert_eq!(
                        previous["plannedBeats"][0]["id"],
                        format!("beat-{previous_chapter}")
                    );
                    assert_eq!(
                        previous["actualSceneGroupIds"],
                        json!([format!("group-{previous_chapter}")])
                    );
                    assert_eq!(previous["assessmentStatus"], "assessed");
                    assert_eq!(previous["sceneOutcomeAssessment"]["verdict"], "realized");
                    assert_eq!(
                        previous["sceneOutcomeAssessment"]["authority"],
                        "candidateAnalysis"
                    );
                    assert!(context["sceneOutcomeRefs"].as_array().is_some_and(|refs| {
                        refs.iter().any(|reference| {
                            reference["sceneId"] == format!("scene-{previous_chapter}")
                                && reference["verdict"] == "realized"
                        })
                    }));
                    assert_eq!(previous["actualEventRefs"][0]["predicate"], "sawLight");
                    assert_eq!(
                        previous["actualEventRefs"][0]["confirmationState"],
                        "candidate"
                    );
                    assert!(previous["actualEventRefs"][0]["modelingBatchId"].is_string());
                    assert_eq!(previous["actualEventRefs"][0]["assertionIndex"], 0);
                    let other_scene = links
                        .iter()
                        .find(|link| {
                            link["chapterNumber"] == previous_chapter
                                && link["sceneId"] == format!("scene-{previous_chapter}-other")
                        })
                        .expect("second scene must remain separately linked");
                    assert_eq!(
                        other_scene["actualSceneGroupIds"],
                        json!([format!("group-{previous_chapter}-other")])
                    );
                    assert_eq!(other_scene["actualEventRefs"], json!([]));
                    assert_eq!(other_scene["assessmentStatus"], "assessed");
                    assert_eq!(
                        other_scene["sceneOutcomeAssessment"]["verdict"],
                        "insufficientEvidence"
                    );
                    assert!(
                        links
                            .iter()
                            .all(|link| link["chapterNumber"].as_i64().unwrap()
                                < action["target"]["planFromChapter"].as_i64().unwrap())
                    );
                }
                let mut advance_version = claimed["version"].as_i64().unwrap();
                if first_chapter_draft {
                    let (status, current) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/current-chapter-revision"),
                        Some(json!({
                            "expectedVersion": advance_version, "instruction": "第一章补写白塔证据"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{current}");
                    assert_eq!(current["pendingCurrentIntentGeneration"], 1);
                    assert_eq!(current["intentRevisions"][1]["effectiveFromChapter"], 1);
                    assert_eq!(current["activeTaskId"], task_id);
                    advance_version = current["version"].as_i64().unwrap();
                    let (status, duplicate) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/current-chapter-revision"),
                        Some(json!({
                            "expectedVersion": advance_version, "instruction": "重复改写当前章"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
                    let (status, adjusted) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/future-intents"),
                        Some(json!({
                            "expectedVersion": advance_version, "instruction": "第二章转向调查白塔"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{adjusted}");
                    assert_eq!(adjusted["intentRevisions"][2]["effectiveFromChapter"], 2);
                    assert_eq!(adjusted["activeTaskId"], task_id);
                    assert_eq!(adjusted["stepToken"], claimed["stepToken"]);
                    assert_eq!(adjusted["authorMessageId"], claimed["authorMessageId"]);
                    assert_ne!(
                        adjusted["intentRevisions"][2]["source"]["authorMessageId"],
                        claimed["authorMessageId"]
                    );
                    assert_eq!(adjusted["providerBudget"], claimed["providerBudget"]);
                    let (status, frozen_task) = call(
                        &path,
                        "GET",
                        &format!("/api/projects/{project_id}/tasks/{task_id}"),
                        /*payload*/ None,
                        /*candidate*/ false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{frozen_task}");
                    assert_eq!(frozen_task["inputManifest"], task["inputManifest"]);
                    advance_version = adjusted["version"].as_i64().unwrap();
                    let (status, paused) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/control"),
                        Some(json!({
                            "expectedVersion": advance_version, "command": "pause"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{paused}");
                    assert_eq!(paused["status"], "paused");
                    let (status, resumed) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/control"),
                        Some(json!({
                            "expectedVersion": paused["version"], "command": "resume"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{resumed}");
                    assert_eq!(resumed["intentRevisions"][2]["effectiveFromChapter"], 2);
                    advance_version = resumed["version"].as_i64().unwrap();
                }
                if second_chapter_first_draft {
                    let (status, adjusted) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/current-chapter-revision"),
                        Some(json!({
                            "expectedVersion": advance_version, "instruction": "第二章先找到航海日志"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{adjusted}");
                    assert_eq!(
                        adjusted["pendingCurrentIntentGeneration"], 4,
                        "{adjusted}; operations={operations:?}"
                    );
                    advance_version = adjusted["version"].as_i64().unwrap();
                }

                if operation == "extractCommitments" {
                    if commitments::submit(commitments::Step {
                        path: &path,
                        project_id,
                        run_uri: &run_uri,
                        task_id,
                        thread_id: &thread_id,
                        target: &target,
                        revision,
                        version: advance_version,
                        recovery: if chapter_number == Some(1)
                            && next["run"]["feedbackRepairCount"] == 0
                        {
                            commitments::Recovery::ResumeAfterOutput
                        } else {
                            commitments::Recovery::Normal
                        },
                    })
                    .await
                    {
                        continue;
                    }
                } else if operation == "assessSceneOutcome" {
                    let chapter_id = chapter_id.as_deref().unwrap();
                    let number = action["target"]["chapterNumber"].as_i64().unwrap();
                    let plan = json!({"question":"场景是否落实灯光事件", "byteBudget":9000, "ranking":{"type":"literal"}, "steps":[
                        {"id":"text", "operation":{"type":"text", "query":"看见灯光", "limit":1}},
                        {"id":"name", "operation":{"type":"resolveEntity", "name":"林夕"}},
                        {"id":"state", "operation":{"type":"state", "entityId":"linxi", "predicate":"sawLight", "perspective":"world"}},
                        {"id":"graph", "operation":{"type":"relations", "entityId":"linxi", "direction":"outgoing", "maxHops":1, "limit":1}}
                    ]});
                    let encoded: String = plan
                        .to_string()
                        .as_bytes()
                        .iter()
                        .map(|byte| format!("%{byte:02X}"))
                        .collect();
                    let (status, context) = call(
                        &path,
                        "GET",
                        &format!("/api/projects/{project_id}/context?taskId={task_id}&threadId={thread_id}&projectRevision={revision}&purpose=critic&budget=24&byteBudget=60000&queryPlan={encoded}"),
                        None,
                        true,
                    ).await;
                    assert_eq!(status, StatusCode::OK, "{context}");
                    assert_eq!(context["queryScope"]["scope"], "sceneAssessment");
                    let linked = context["operationContext"]["scenePlanDraftLinks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|link| link["sceneId"] == format!("scene-{number}"))
                        .unwrap();
                    assert_eq!(
                        linked["plannedExpectedEffects"][0]["commitmentRefs"],
                        if number > 1 {
                            commitments::plan_refs(&path, project_id, number - 1)
                        } else {
                            json!([])
                        }
                    );
                    assert_eq!(context["queryReport"]["policy"], context["queryScope"]);
                    let commitment_assessments: Vec<Value> = context["operationContext"]["boundCommitmentInputs"]["entries"]
                        .as_array().unwrap().iter().map(|input| {
                            assert_eq!(input["referenceState"], "current");
                            assert!(!input["fulfillmentCriteria"].as_str().unwrap().is_empty());
                            json!({"reference":input["reference"],"referenceState":input["referenceState"],
                                "outcome":"advanced","rationale":"灯光事件推进了原规划绑定的承诺，但没有证明完全兑现",
                                "actualEventRefs":[{"modelingBatchId":action["target"]["knowledgeBatchId"],"assertionIndex":0}]})
                        }).collect();
                    assert_eq!(
                        context["queryReport"]["steps"][0]["results"][0]["sourceRevisionId"],
                        action["target"]["draftRevisionId"]
                    );
                    assert!(
                        context["queryReport"]["steps"][2]["results"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|entry| entry["sourceChapter"].as_i64().unwrap() <= number)
                    );
                    assert!(
                        context["operationContext"]["scenePlanDraftLinks"]
                            .as_array()
                            .is_some_and(|links| links
                                .iter()
                                .any(|link| link["sceneId"] == format!("scene-{number}")))
                    );
                    let payload = json!({
                        "chapterId": chapter_id,
                        "planRevisionId": action["target"]["planRevisionId"],
                        "draftRevisionId": action["target"]["draftRevisionId"],
                        "knowledgeBatchId": action["target"]["knowledgeBatchId"],
                        "inputSnapshotId": action["target"]["inputSnapshotId"],
                        "intentGeneration": action["target"]["intentGeneration"],
                        "assessments": [
                            {"sceneId":format!("scene-{number}"),"verdict":"realized",
                             "rationale":"正文中出现灯光事件","recommendation":"continue",
                             "effectAssessments":[{"effectId":format!("effect-{number}"),"verdict":"realized",
                               "rationale":"灯光事件见于正文","actualEventRefs":[{"modelingBatchId":action["target"]["knowledgeBatchId"],"assertionIndex":0}],"commitmentAssessments":commitment_assessments}]},
                            {"sceneId":format!("scene-{number}-other"),"verdict":"insufficientEvidence",
                             "rationale":"本场没有可核验的事件","recommendation":"askAuthor",
                             "effectAssessments":[{"effectId":format!("effect-{number}-other"),"verdict":"insufficientEvidence",
                               "rationale":"缺少可定位的事件","actualEventRefs":[],"commitmentAssessments":[]}]}
                        ]
                    });
                    let (status, saved) = call(&path, "POST",
                        &format!("/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments"),
                        Some(json!({"taskId":task_id,"threadId":thread_id,"expectedRevision":revision,
                            "expectedCandidateRevisionId":assessment_revisions.get(chapter_id),"idempotencyKey":format!("assessment-{step}"),
                            "payload":payload})), true).await;
                    assert_eq!(status, StatusCode::CREATED, "{saved}");
                    assessment_revisions.insert(
                        chapter_id.to_owned(),
                        saved["revisionId"].as_str().unwrap().to_owned(),
                    );
                    let (status, read) = call(
                        &path,
                        "GET",
                        &format!(
                            "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments"
                        ),
                        None,
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{read}");
                    assert_eq!(read["state"], "current");
                    assert_eq!(read["authority"], "candidateAnalysis");
                } else if let Some(batch_id) = batch_id {
                    let number = next["run"]["currentChapterNumber"].as_i64().unwrap();
                    let (status, published) = call(&path, "POST", &format!("/api/projects/{project_id}/modeling-batches/{batch_id}/publish"), Some(json!({
                        "taskId": task_id, "threadId": thread_id, "expectedRevision": revision,
                        "entities": [{"id":"linxi","kind":"character","displayName":"林夕","aliases":[],
                            "evidence":{"quote":"林夕","start":0,"end":2}},
                            {"id":format!("saw-light-{number}"),"kind":"event","displayName":"看见灯光","aliases":[],"evidence":{"quote":"看见灯光","start":2,"end":6}}],
                        "assertions": [{"kind":"event","subjectId":format!("saw-light-{number}"),"predicate":"sawLight",
                            "value":{"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null},"storyTime":number,"evidence":{"quote":"看见灯光","start":2,"end":6}}]
                    })), true).await;
                    assert_eq!(status, StatusCode::OK, "{published}");
                } else {
                    let (kind, payload) = match operation {
                        "generateBrief" => ("brief", json!({"premise":"港城正在失去记忆"})),
                        "generateOutline" => (
                            "outline",
                            json!({"nodes":[{"id":"book-1","kind":"book","position":0,"title":"港城的记忆","purpose":"记录遗忘","summary":"三章序幕"}]}),
                        ),
                        "generatePlan" => {
                            let start = action["target"]["planFromChapter"].as_i64().unwrap();
                            let refs = if start > 1 {
                                commitments::plan_refs(&path, project_id, start - 1)
                            } else {
                                json!([])
                            };
                            (
                                "plan",
                                json!({"chapters": (start..=3).map(|number| json!({
                                "chapterPlanId": format!("plan-{number}"),
                                "chapterNumber": number,
                                "goal": format!("第 {number} 章根据已写正文继续"),
                                "scenes": [
                                    {"id":format!("scene-{number}"), "goal":"发现白塔线索", "beats":[{"id":format!("beat-{number}"),"description":"灯光引发疑问"}],
                                     "expectedEffects":[{"id":format!("effect-{number}"),"dimension":"knowledge","description":"林夕看见白塔灯光","commitmentRefs":refs}]},
                                    {"id":format!("scene-{number}-other"), "goal":"留下未解问题", "beats":[],
                                     "expectedEffects":[{"id":format!("effect-{number}-other"),"dimension":"narrative","description":"张衡离开而未解释动机","commitmentRefs":[]} ]}
                                ]
                            })).collect::<Vec<_>>()}),
                            )
                        }
                        "generateChapter" => {
                            let number = chapter_number.unwrap();
                            (
                                "draft",
                                json!({"chapterId":chapter_id,"chapterNumber":number,
                                "title":format!("第 {number} 章"),"body":"林夕看见灯光。\n\n---\n\n张衡离开。",
                                "sceneGroups":[
                                    {"id":format!("group-{number}"), "sceneId":format!("scene-{number}"), "body":"林夕看见灯光。", "locked":false},
                                    {"id":format!("group-{number}-other"), "sceneId":format!("scene-{number}-other"), "body":"张衡离开。", "locked":false}
                                ]}),
                            )
                        }
                        _ => panic!("unexpected operation {operation}"),
                    };
                    let uri = if kind == "draft" {
                        format!("/api/projects/{project_id}/drafts/checkpoints")
                    } else {
                        format!("/api/projects/{project_id}/artifacts/{kind}")
                    };
                    let expected_candidate = if kind == "plan" {
                        plan_revision.as_ref()
                    } else {
                        chapter_id.as_ref().and_then(|id| draft_revisions.get(id))
                    };
                    if kind == "plan" && plan_revision.is_none() {
                        let mut incomplete = payload.clone();
                        incomplete["chapters"][0]["scenes"][0]["expectedEffects"] = json!([]);
                        let (status, rejected) = call(
                            &path,
                            "POST",
                            &uri,
                            Some(json!({
                                "payload":incomplete,"expectedRevision":revision,
                                "expectedCandidateRevisionId":null,
                                "idempotencyKey":format!("invalid-plan-effect-{step}"),
                                "taskId":task_id,"threadId":thread_id
                            })),
                            true,
                        )
                        .await;
                        assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
                    }
                    let (status, saved) = call(&path, "POST", &uri, Some(json!({
                        "payload":payload,"expectedRevision":revision,"expectedCandidateRevisionId":expected_candidate,
                        "idempotencyKey":format!("generation-{step}"),"taskId":task_id,"threadId":thread_id
                    })), true).await;
                    assert_eq!(status, StatusCode::CREATED, "{operation}: {saved}");
                    if kind == "draft" {
                        let (_, current_workspace) = call(
                            &path,
                            "GET",
                            &format!("/api/projects/{project_id}/creative-workspace"),
                            None,
                            false,
                        )
                        .await;
                        let draft_chapter_id = chapter_id.as_deref().unwrap();
                        let binding = current_workspace["chapterPlanBindings"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|binding| binding["chapter_id"] == draft_chapter_id)
                            .expect("generated chapter must bind its plan");
                        assert_eq!(
                            binding["plan_revision_id"],
                            plan_revision.as_deref().unwrap()
                        );
                        let chapter = current_workspace["chapters"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|chapter| chapter["id"] == draft_chapter_id)
                            .unwrap();
                        assert_eq!(
                            chapter["candidate"]["sourcePlanRefs"]["planRevisionId"],
                            plan_revision.as_deref().unwrap()
                        );
                        assert_eq!(
                            chapter["candidate"]["sourcePlanRefs"]["chapterPlanId"],
                            binding["chapter_plan_id"]
                        );
                        assert_eq!(chapter["candidate"]["sourcePlanRefs"]["localOnly"], false);
                        draft_revisions.insert(
                            chapter_id.unwrap(),
                            saved["revisionId"].as_str().unwrap().to_owned(),
                        );
                    } else if kind == "plan" {
                        plan_revision = Some(saved["revisionId"].as_str().unwrap().to_owned());
                    }
                }
                if second_chapter_first_draft || feedback_repair_first_draft {
                    let (status, failed) = call(
                        &path,
                        "PATCH",
                        &format!("/api/projects/{project_id}/tasks/{task_id}"),
                        Some(json!({
                            "threadId": thread_id, "status": "failed", "error": "interrupted after checkpoint"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{failed}");
                    charge_budget(&path, &run_uri, task_id).await;
                    let (status, blocked) =
                        call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
                    assert_eq!(status, StatusCode::OK, "{blocked}");
                    assert_eq!(
                        blocked["run"]["blockedReason"],
                        "agent_task_failed_after_output"
                    );
                    let (status, retried) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/retry-failed-task"),
                        Some(json!({"expectedVersion": blocked["run"]["version"]})),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{retried}");
                    assert_eq!(retried["stage"], "draft");
                    assert!(retried["pendingCurrentIntentGeneration"].is_null());
                    if feedback_repair_first_draft {
                        failed_feedback_repair_once = true;
                        assert_eq!(
                            retried["feedbackRepairPrompt"],
                            next["run"]["feedbackRepairPrompt"]
                        );
                    } else {
                        assert_eq!(retried["usedTaskAttempts"], 15);
                    }
                    continue;
                }
                let final_assessment =
                    operation == "assessSceneOutcome" && action["target"]["chapterNumber"] == 3;
                if final_assessment {
                    delivery::exhaust_final_assessment_allowance(&path, project_id, task_id).await;
                }
                let (status, completed) = call(
                    &path,
                    "POST",
                    &format!("/api/projects/{project_id}/tasks/{task_id}/complete"),
                    Some(json!({
                        "threadId":thread_id,"summary":"candidate saved"
                    })),
                    true,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{operation}: {completed}");
                let charged = if final_assessment {
                    let (status, charged) = call(
                        &path,
                        "POST",
                        &format!("{run_uri}/provider-usage"),
                        Some(json!({"taskId":task_id})),
                        /*candidate*/ false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{charged}");
                    charged
                } else {
                    charge_budget(&path, &run_uri, task_id).await
                };
                advance_version = charged["version"].as_i64().unwrap();
                let (status, advanced) = call(
                    &path,
                    "POST",
                    &format!("{run_uri}/advance-task"),
                    Some(json!({
                        "expectedVersion":advance_version,"taskId":task_id
                    })),
                    false,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{operation}: {advanced}");
                if first_chapter_draft {
                    assert_eq!(advanced["stage"], "draft");
                    assert!(advanced["pendingCurrentIntentGeneration"].is_null());
                }
            }
            other => panic!("unexpected action {other}: {next}"),
        }
    }
    let (_, finished) = call(&path, "GET", &run_uri, None, false).await;
    assert_eq!(finished["status"], "completed", "{finished}");
    assert_eq!(finished["completedChapters"], 3);
    assert_eq!(finished["usedTaskAttempts"], 24);
    assert_eq!(
        finished["providerBudget"]["usedRequests"],
        finished["providerBudget"]["maxRequests"]
    );
    assert_eq!(
        operations,
        [
            "generateBrief",
            "generateOutline",
            "generatePlan",
            "generateChapter",
            "generateChapter",
            "modelKnowledge",
            "extractCommitments",
            "assessSceneOutcome",
            "generateChapter",
            "generateChapter",
            "modelKnowledge",
            "extractCommitments",
            "assessSceneOutcome",
            "generatePlan",
            "generateChapter",
            "generateChapter",
            "modelKnowledge",
            "extractCommitments",
            "assessSceneOutcome",
            "generatePlan",
            "generateChapter",
            "modelKnowledge",
            "extractCommitments",
            "assessSceneOutcome"
        ]
    );
    assert_eq!(draft_revisions.len(), 3);

    let (_, workspace) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/creative-workspace"),
        None,
        false,
    )
    .await;
    let (status, continuation) = call(
        &path,
        "POST",
        &runs_uri,
        Some(json!({
            "expectedProjectRevision": workspace["revision"],
            "authorIntent": "第四章让林夕追查白塔记录",
            "targetChapters": 1, "maxTaskAttempts": 7,"maxProviderRequests":256,"maxProviderTokens":4000000, "deadlineAt": now + 600
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{continuation}");
    assert_eq!(continuation["stage"], "rollingPlan");
    assert_eq!(continuation["startChapterNumber"], 4);
    assert_eq!(continuation["completedChapters"], 3);
    assert_eq!(continuation["targetChapters"], 4);
    assert_eq!(
        continuation["intentRevisions"][0]["effectiveFromChapter"],
        4
    );
    let continuation_uri = format!("{runs_uri}/{}", continuation["id"].as_str().unwrap());
    let mut fourth_draft = None;
    let mut continuation_operations = Vec::new();
    let mut commitment_feedback_required = false;
    for step in 0..10 {
        let (status, next) = call(
            &path,
            "POST",
            &format!("{continuation_uri}/next"),
            None,
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        if next["run"]["status"] == "completed" {
            break;
        }
        let action = &next["action"];
        let version = next["run"]["version"].as_i64().unwrap();
        match action["kind"].as_str().unwrap() {
            "materializeChapter" | "checkKnowledge" => {
                let endpoint = if action["kind"] == "materializeChapter" {
                    "materialize"
                } else {
                    "check"
                };
                let (status, result) = call(
                    &path,
                    "POST",
                    &format!("{continuation_uri}/{endpoint}"),
                    Some(json!({"expectedVersion": version})),
                    false,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{result}");
                if endpoint == "check" && result["status"] == "blocked" {
                    assert_eq!(result["blockedReason"], "scene_feedback_needs_author");
                    let chapter_id = next["run"]["currentChapterId"].as_str().unwrap();
                    let (status, assessment) = call(
                        &path,
                        "GET",
                        &format!(
                            "/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments"
                        ),
                        None,
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{assessment}");
                    assert_eq!(
                        assessment["assessment"]["assessments"][0]["effectAssessments"][0]["verdict"],
                        "realized"
                    );
                    assert_eq!(
                        assessment["assessment"]["assessments"][0]["verdict"],
                        "deviated"
                    );
                    let (status, accepted) = call(&path, "POST", &format!("{continuation_uri}/resolve-feedback"),
                        Some(json!({"expectedVersion":result["version"],"assessmentRevisionId":assessment["revisionId"],
                            "choice":"continue","instruction":""})), false).await;
                    assert_eq!(status, StatusCode::OK, "{accepted}");
                    assert_eq!(
                        accepted["feedbackDecisions"]
                            .as_array()
                            .unwrap()
                            .last()
                            .unwrap()["choice"],
                        "continue"
                    );
                    commitment_feedback_required = true;
                }
            }
            "agentTask" => {
                let operation = action["operation"].as_str().unwrap();
                continuation_operations.push(operation.to_owned());
                let (_, workspace) = call(
                    &path,
                    "GET",
                    &format!("/api/projects/{project_id}/creative-workspace"),
                    None,
                    false,
                )
                .await;
                let revision = workspace["revision"].as_i64().unwrap();
                let mut target = action["target"].clone();
                let chapter_id = target["chapterId"].as_str().map(str::to_owned);
                let batch_id = if operation == "modelKnowledge" {
                    let (status, batch) = call(
                        &path,
                        "POST",
                        &format!("/api/projects/{project_id}/modeling-batches"),
                        Some(json!({
                            "objectId": format!("draft-{}", chapter_id.as_deref().unwrap()),
                            "sourceRevisionId": fourth_draft.as_deref().unwrap(),
                            "expectedRevision": revision, "extractorVersion": "test-v1"
                        })),
                        false,
                    )
                    .await;
                    assert_eq!(status, StatusCode::CREATED, "{batch}");
                    let id = batch["id"].as_str().unwrap().to_owned();
                    target["batchId"] = json!(id);
                    Some(id)
                } else {
                    None
                };
                let thread_id = format!("continuation-step-{step}");
                let (status, task) = call(
                    &path,
                    "POST",
                    &format!("/api/projects/{project_id}/tasks"),
                    Some(json!({
                        "operation": operation, "inputRevision": revision,
                        "threadId": thread_id, "target": target,
                        "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
                        "capabilityExpiresAt": now + 600,
                        "authorIntent": "第四章让林夕追查白塔记录"
                    })),
                    false,
                )
                .await;
                assert_eq!(status, StatusCode::CREATED, "{operation}: {task}");
                let task_id = task["id"].as_str().unwrap();
                if operation == "generatePlan" {
                    assert_eq!(target["planFromChapter"], 4);
                    let (status, context) = call(
                        &path,
                        "GET",
                        &format!("/api/projects/{project_id}/context?taskId={task_id}&threadId={thread_id}&projectRevision={revision}&purpose=planner&budget=24&byteBudget=60000"),
                        None,
                        true,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{context}");
                    let proposals = context["candidateCommitmentProposals"].as_array().unwrap();
                    assert_eq!(proposals.len(), 3);
                    assert_eq!(proposals[0]["chapterNumber"], 3);
                    assert!(proposals.iter().all(|proposal| proposal["authority"]
                        == "candidateProposal"
                        && proposal["sourceCurrent"] == true
                        && proposal["established"] == false
                        && proposal["confirmationState"] == "unconfirmed"));
                    assert_eq!(context["relevantCommitments"], json!([]));
                    assert_eq!(
                        context["operationContext"]["workingKnowledge"]["coverage"]["complete"],
                        true
                    );
                    assert_eq!(
                        context["operationContext"]["workingKnowledge"]["entries"]
                            .as_array()
                            .unwrap()
                            .len(),
                        3
                    );
                }
                if operation == "extractCommitments" {
                    assert!(
                        !commitments::submit(commitments::Step {
                            path: &path,
                            project_id,
                            run_uri: &continuation_uri,
                            task_id,
                            thread_id: &thread_id,
                            target: &target,
                            revision,
                            version: version + 1,
                            recovery: commitments::Recovery::Normal,
                        })
                        .await
                    );
                } else if operation == "assessSceneOutcome" {
                    let chapter_id = chapter_id.as_deref().unwrap();
                    let (status, context) = call(&path, "GET",
                        &format!("/api/projects/{project_id}/context?taskId={task_id}&threadId={thread_id}&projectRevision={revision}&purpose=critic&budget=24&byteBudget=60000"), None, true).await;
                    assert_eq!(status, StatusCode::OK, "{context}");
                    let inputs = context["operationContext"]["boundCommitmentInputs"]["entries"]
                        .as_array()
                        .unwrap();
                    assert!(!inputs.is_empty());
                    let commitment_assessments: Vec<Value> = inputs.iter().map(|input| json!({
                        "reference":input["reference"],"referenceState":input["referenceState"],
                        "outcome":"contradicted","rationale":"事件实现了场景效果，但与承诺的原兑现标准矛盾",
                        "actualEventRefs":[{"modelingBatchId":target["knowledgeBatchId"],"assertionIndex":0}]
                    })).collect();
                    let (status, saved) = call(
                        &path,
                        "POST",
                        &format!("/api/projects/{project_id}/chapters/{chapter_id}/scene-assessments"),
                        Some(json!({
                            "taskId":task_id,"threadId":thread_id,"expectedRevision":revision,
                            "expectedCandidateRevisionId":null,"idempotencyKey":format!("continuation-assessment-{step}"),
                            "payload":{
                                "chapterId":chapter_id,
                                "planRevisionId":target["planRevisionId"],
                                "draftRevisionId":target["draftRevisionId"],
                                "knowledgeBatchId":target["knowledgeBatchId"],
                                "inputSnapshotId":target["inputSnapshotId"],
                                "intentGeneration":target["intentGeneration"],
                                "assessments":[{"sceneId":"scene-4","verdict":"deviated",
                                    "rationale":"灯光事件与绑定承诺矛盾","recommendation":"askAuthor",
                                    "effectAssessments":[{"effectId":"effect-4","verdict":"realized",
                                        "rationale":"灯光事件见于正文","actualEventRefs":[{"modelingBatchId":target["knowledgeBatchId"],"assertionIndex":0}],"commitmentAssessments":commitment_assessments}]}]
                            }
                        })),
                        true,
                    ).await;
                    assert_eq!(status, StatusCode::CREATED, "{saved}");
                } else if let Some(batch_id) = batch_id {
                    let (status, published) = call(
                        &path,
                        "POST",
                        &format!("/api/projects/{project_id}/modeling-batches/{batch_id}/publish"),
                        Some(json!({
                            "taskId": task_id, "threadId": thread_id, "expectedRevision": revision,
                            "entities": [{"id":"linxi","kind":"character","displayName":"林夕","aliases":[],
                                "evidence":{"quote":"林夕","start":0,"end":2}},
                                {"id":"saw-light","kind":"event","displayName":"看见灯光","aliases":[],"evidence":{"quote":"看见灯光","start":2,"end":6}}],
                            "assertions": [{"kind":"event","subjectId":"saw-light","predicate":"sawLight",
                                "value":{"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null},"storyTime":4,"evidence":{"quote":"看见灯光","start":2,"end":6}}]
                        })),
                        true,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{published}");
                } else {
                    let (kind, payload, previous) = if operation == "generatePlan" {
                        (
                            "plan",
                            json!({"chapters":[{"chapterPlanId":"plan-4","chapterNumber":4,"goal":"追查白塔记录","scenes":[{"id":"scene-4","goal":"看见灯光","beats":[],"expectedEffects":[{"id":"effect-4","dimension":"knowledge","description":"林夕看见白塔灯光","commitmentRefs":commitments::plan_refs(&path,project_id,3)}]}]}]}),
                            plan_revision.as_deref(),
                        )
                    } else {
                        assert_eq!(operation, "generateChapter");
                        assert_eq!(target["chapterNumber"], 4);
                        (
                            "draft",
                            json!({"chapterId":chapter_id,"chapterNumber":4,"title":"第四章","body":"林夕看见灯光。\u{0}😀","sceneGroups":[{"id":"group-4","sceneId":"scene-4","body":"林夕看见灯光。\u{0}😀","locked":false}]}),
                            None,
                        )
                    };
                    let uri = if kind == "draft" {
                        format!("/api/projects/{project_id}/drafts/checkpoints")
                    } else {
                        format!("/api/projects/{project_id}/artifacts/{kind}")
                    };
                    let (status, saved) = call(
                        &path,
                        "POST",
                        &uri,
                        Some(json!({
                            "payload": payload, "expectedRevision": revision,
                            "expectedCandidateRevisionId": previous,
                            "idempotencyKey": format!("continuation-{step}"),
                            "taskId": task_id, "threadId": thread_id
                        })),
                        true,
                    )
                    .await;
                    assert_eq!(status, StatusCode::CREATED, "{operation}: {saved}");
                    if kind == "draft" {
                        fourth_draft = Some(saved["revisionId"].as_str().unwrap().to_owned());
                    }
                }
                let (status, completed) = call(
                    &path,
                    "POST",
                    &format!("/api/projects/{project_id}/tasks/{task_id}/complete"),
                    Some(json!({"threadId": thread_id, "summary": "candidate saved"})),
                    true,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{completed}");
                let charged = charge_budget(&path, &continuation_uri, task_id).await;
                let (status, advanced) = call(
                    &path,
                    "POST",
                    &format!("{continuation_uri}/advance-task"),
                    Some(json!({"expectedVersion": charged["version"], "taskId": task_id})),
                    false,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{advanced}");
            }
            other => panic!("unexpected continuation action {other}: {next}"),
        }
    }
    let (_, completed) = call(&path, "GET", &continuation_uri, None, false).await;
    assert_eq!(completed["status"], "completed", "{completed}");
    assert_eq!(completed["completedChapters"], 4);
    assert!(commitment_feedback_required);
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert!(
        repository
            .object_heads(&ProjectId::parse(project_id).unwrap())
            .unwrap()
            .iter()
            .all(|head| head.object_kind != "commitment")
    );
    assert_eq!(
        continuation_operations,
        [
            "generatePlan",
            "generateChapter",
            "modelKnowledge",
            "extractCommitments",
            "assessSceneOutcome"
        ]
    );
    assert!(fourth_draft.is_some());
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
