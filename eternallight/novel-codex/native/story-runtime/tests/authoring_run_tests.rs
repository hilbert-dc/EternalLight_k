use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_core::{
    CreativeIntentRevision, ProjectId, RepositoryError, SqliteStoryRepository,
    generation_author_intent,
};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "authoring-author-token-1234567890123456";
const CANDIDATE: &str = "authoring-candidate-token-1234567890123";
const CAPABILITY: &str = "authoring-task-secret-12345678901234567890";

#[path = "support/authoring_selected_chapters.rs"]
mod selected_chapters;

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
        if method == "GET" && uri.contains("/task-source?") {
            let thread = uri
                .split('?')
                .nth(1)
                .unwrap()
                .split('&')
                .find_map(|field| field.strip_prefix("threadId="))
                .unwrap();
            request = request
                .header("x-novel-root-thread", thread)
                .header("x-novel-caller-thread", thread)
                .header("x-novel-tool-name", "read_task_source")
                .header("x-novel-turn", "authoring-fixture-source-turn")
                .header("x-novel-tool-call", "authoring-fixture-source-call");
        }
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
        task_fixture::work_control_response(uri, status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)))),
    )
}

async fn charge_budget(path: &std::path::Path, run_uri: &str, task_id: &str) -> Value {
    // Trusted fixture accounting reads only the immutable Native grant, like
    // settle_task_fixture. The live Host endpoint correctly closes at terminal.
    let db = rusqlite::Connection::open(path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    let grant: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(grant["taskId"], task_id);
    assert_eq!(grant["budgetId"], format!("budget-{task_id}"));
    assert_eq!(
        grant["workTaskId"],
        format!("work-{}", run_uri.rsplit('/').next().unwrap())
    );
    let mut budget = PersistentBudget::new(
        &path.with_extension("budget.sqlite3"),
        task_id,
        json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"]},"now":0}),
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

async fn read_section(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    context: &Value,
    section: &str,
) -> Value {
    let source = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["sectionPath"] == section)
        .unwrap_or_else(|| panic!("missing {section}: {context}"));
    assert_eq!(
        context.pointer(section).unwrap()["sourceId"],
        source["sourceId"]
    );
    let endpoint = format!(
        "{root}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let mut suffix = String::new();
    let mut text = String::new();
    loop {
        let (status, page) = call(path, "GET", &format!("{endpoint}{suffix}"), None, true).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source["sourceId"]);
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert_eq!(page["byteRange"]["start"], text.len());
        assert!(page.to_string().len() <= 6500);
        assert_model_item_bound(&page);
        let part = page["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        text.push_str(part);
        assert_eq!(page["byteRange"]["end"], text.len());
        assert_eq!(page["byteRange"]["total"], source["payloadBytes"]);
        if let Some(cursor) = page["nextCursor"].as_str() {
            let next = format!("&cursor={}", cursor.replace(':', "%3A"));
            assert_ne!(next, suffix);
            suffix = next;
        } else {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(source["payloadBytes"], text.len());
            break;
        }
    }
    assert_eq!(
        source["contentHash"],
        format!("{:x}", Sha256::digest(text.as_bytes()))
    );
    serde_json::from_str(&text).unwrap()
}

fn contains_text(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(text) => text == expected,
        Value::Array(items) => items.iter().any(|item| contains_text(item, expected)),
        Value::Object(fields) => fields.values().any(|item| contains_text(item, expected)),
        _ => false,
    }
}

fn assert_model_item_bound(value: &Value) {
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),
        "output":json!({"content":[{"type":"text","text":value.to_string()}]}).to_string()});
    let tokens = eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(tokens <= 9000, "model item uses {tokens} tokens");
}

// Only the controlled test model picks the requested capability. Production
// supplies the actual action identity, scope, sources and original allowance.
async fn model_action(path: &std::path::Path, run_uri: &str, operation: &str) -> Value {
    let root = run_uri.split("/generation-runs/").next().unwrap();
    let (status, next) = call(path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "advisory", "{next}");
    let (_, workspace) = call(
        path,
        "GET",
        &format!("{root}/creative-workspace"),
        None,
        false,
    )
    .await;
    let intent = next["run"]["intentRevisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["instruction"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let (status,parent)=call(path,"POST",&format!("{root}/tasks"),Some(json!({
        "operation":"advisory","inputRevision":workspace["revision"],"threadId":"model-coordinator",
        "target":next["action"]["target"],"authorIntent":intent,
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":next["run"]["deadlineAt"]})),false).await;
    assert_eq!(status, StatusCode::CREATED, "{parent}");
    let id = parent["id"].as_str().unwrap();
    let (status,context)=call(path,"GET",&format!("{root}/context?taskId={id}&threadId=model-coordinator&projectRevision={}&purpose=planner&byteBudget=60000",parent["inputRevision"]),None,true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_model_item_bound(&context);
    let requirements =
        read_section(path, root, &parent, &context, "/authorRequirementsSource").await;
    let history: Vec<CreativeIntentRevision> =
        serde_json::from_value(next["run"]["intentRevisions"].clone()).unwrap();
    let chapter = next["run"]["currentChapterNumber"]
        .as_i64()
        .unwrap_or(next["run"]["completedChapters"].as_i64().unwrap() + 1);
    let rendered = generation_author_intent(&history, chapter).unwrap();
    assert_eq!(requirements["authority"], "frozenAuthorIntent");
    assert_eq!(requirements["authorText"], rendered);
    assert_eq!(
        parent["inputManifest"]["authorIntent"],
        requirements["authorText"]
    );
    let db = rusqlite::Connection::open(path).unwrap();
    for original in &history {
        if let novel_codex_story_core::CreativeIntentSource::AuthorMessage {
            author_message_id,
            ..
        } = &original.source
        {
            let original_text: String = db.query_row("SELECT author_text FROM story_author_messages_v1 WHERE message_id=?1 AND generation_run_id=?2", rusqlite::params![author_message_id,next["run"]["id"].as_str().unwrap()], |row| row.get(0)).unwrap();
            assert_eq!(original_text, original.instruction);
        }
    }
    let uri = format!(
        "{root}/tasks/{id}/work-decision?threadId=model-coordinator&expectedRevision={}",
        parent["inputRevision"]
    );
    let (status, mut decision) = call(path, "GET", &uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    let mut cursor = decision["pagination"]["nextCursor"].clone();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(c) = cursor.as_str() {
        assert!(seen.insert(c.to_owned()));
        let (status, page) = call(
            path,
            "GET",
            &format!("{uri}&cursor={}", c.replace(':', "%3A")),
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        for field in ["version", "sourceDigest", "workTaskId", "threadId"] {
            assert_eq!(page[field], decision[field]);
        }
        decision["readyActions"]
            .as_array_mut()
            .unwrap()
            .extend(page["readyActions"].as_array().unwrap().iter().cloned());
        cursor = page["pagination"]["nextCursor"].clone();
    }
    assert_eq!(
        decision["readyActions"].as_array().unwrap().len() as u64,
        decision["coverage"]["availableActions"].as_u64().unwrap()
    );
    let choice = decision["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == operation)
        .unwrap_or_else(|| panic!("missing {operation}: {decision}"));
    let (status,selected)=call(path,"POST",&format!("{root}/tasks/{id}/work-decision/actions"),Some(json!({
        "taskId":id,"threadId":"model-coordinator","expectedRevision":parent["inputRevision"],
        "expectedWorkVersion":decision["version"],"expectedSourceDigest":decision["sourceDigest"],
        "actionKey":choice["actionKey"],"instruction":"按实际作者要求处理当前作品","idempotencyKey":format!("choose-{operation}")})),true).await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    let (status, completed) = call(
        path,
        "POST",
        &format!("{root}/tasks/{id}/complete"),
        Some(json!({"threadId":"model-coordinator","summary":"已登记原作者范围内的真实行动"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let charged = charge_budget(path, run_uri, id).await;
    let (status, advanced) = call(
        path,
        "POST",
        &format!("{run_uri}/advance-task"),
        Some(json!({"expectedVersion":charged["version"],"taskId":id})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{advanced}");
    let (status, action) = call(path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{action}");
    assert_eq!(action["action"]["operation"], operation);
    action
}

#[tokio::test]
async fn generation_start_rejects_stale_candidate_sources_even_when_formal_revision_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stale-start.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"候选改稿防旧许可"})),
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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"已有近章规划的港城"})),
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
        json!({"completedChapters":0,"firstStage":"coordinate"})
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
    assert_eq!(run["stage"], "coordinate");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let next = model_action(&path, &run_uri, "generatePlan").await;
    let status = StatusCode::OK;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generatePlan");
    assert_eq!(next["action"]["target"]["planFromChapter"], 1);
    assert_eq!(
        next["run"]["intentRevisions"][0]["instruction"],
        "改为海上逃亡，旧白塔规划不能直接照搬"
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
    let planned = read_section(&path, &base, &task, &context, "/plannedEffects").await;
    assert_eq!(planned[0]["value"]["sourceRevisionId"], plan["revisionId"]);
    assert_eq!(planned[0]["value"]["chapters"][0]["goal"], "追查白塔记录");
}

#[tokio::test]
async fn existing_chapter_continues_without_forcing_brief_or_full_outline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing-chapter.sqlite3");
    let (status, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"已有正文的港城"})),
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
    assert_eq!(run["stage"], "coordinate");
    assert_eq!(run["startChapterNumber"], 2);
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let next = model_action(&path, &run_uri, "generatePlan").await;
    let status = StatusCode::OK;
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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"已有大纲的港城"})),
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
    assert_eq!(run["stage"], "coordinate");
    let run_uri = format!("{runs_uri}/{}", run["id"].as_str().unwrap());
    let next = model_action(&path, &run_uri, "generatePlan").await;
    let status = StatusCode::OK;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "generatePlan");
    assert_eq!(next["action"]["target"]["planFromChapter"], 1);
}

#[tokio::test]
async fn generation_permit_bounds_actual_attempts_without_a_mandatory_creative_pipeline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commitment-budget.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"承诺预算"})),
        /*candidate*/ false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let uri = format!("/api/projects/{project}/generation-runs");
    let mut permit = json!({"expectedProjectRevision":0,"authorIntent":"写前三章并处理叙事承诺提案","targetChapters":3,"maxTaskAttempts":0,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600});
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
    permit["maxTaskAttempts"] = json!(16);
    let (status, run) = call(&path, "POST", &uri, Some(permit), /*candidate*/ false).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["usedTaskAttempts"], 0);
    assert_eq!(run["maxTaskAttempts"], 16);
    assert_eq!(run["stage"], "coordinate");
}

#[tokio::test]
async fn generation_rejects_changed_chapter_directory_without_mutating_the_run() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("generation-directory.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "港城"})),
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
    assert_eq!(status, StatusCode::CONFLICT, "{next}");
    assert_eq!(
        next["error"],
        json!({"code":"conflict","details":{},"message":"AuthoringRunStale","recovery":null})
    );
    let (status, retained) = call(&path, "GET", &run_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{retained}");
    assert_eq!(retained, run);
    assert_eq!(retained["usedTaskAttempts"], 0);
    let (status, cancelled) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": retained["version"], "command": "cancel"})),
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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Harbor"})),
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
    assert_eq!(run["stage"], "coordinate");
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
    let action = model_action(&path, &run_uri, "generateBrief").await;
    let status = StatusCode::OK;
    assert_eq!(status, StatusCode::OK, "{action}");
    assert_eq!(action["action"]["operation"], "generateBrief");
    assert_eq!(
        action["action"]["target"]["runVersion"],
        action["run"]["version"]
    );

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
    assert_eq!(
        atomically_claimed["version"],
        action["run"]["version"].as_i64().unwrap() + 1
    );
    assert_eq!(atomically_claimed["usedTaskAttempts"], 2);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{run_uri}/advance-task"),
        Some(json!({
            "expectedVersion": atomically_claimed["version"], "taskId": task_id
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
            "expectedVersion": atomically_claimed["version"], "instruction": "中途更改正在生成的章节"
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
    assert_eq!(advanced["stage"], "coordinate");
    assert_eq!(advanced["usedTaskAttempts"], 2);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{run_uri}/advance-task"),
        Some(json!({
            "expectedVersion": atomically_claimed["version"], "taskId": task_id
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "advisory");
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
        revised_action["run"]["intentRevisions"][1]["instruction"]
            .as_str()
            .unwrap()
            .contains("林夕的选择")
    );
    let (status, stale_task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "advisory", "inputRevision": 0,
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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Harbor"})),
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
            "operation": "advisory", "inputRevision": 0,
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
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Harbor"})),
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
    let action = model_action(&path, &run_uri, "generateBrief").await;
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
    let (status, paused) = call(
        &path,
        "POST",
        &format!("{run_uri}/control"),
        Some(json!({"expectedVersion": action["run"]["version"].as_i64().unwrap()+1, "command": "pause"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
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
    assert_eq!(resumed["stage"], "coordinate");
    assert_eq!(resumed["activeTaskId"], Value::Null);
    assert_eq!(resumed["usedTaskAttempts"], 2);
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], "advisory");
    assert_eq!(next["run"]["providerBudget"], resumed["providerBudget"]);
}

#[tokio::test]
async fn blocked_original_coordinator_can_retry_without_reopening_or_refunding_its_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("failed-authoring.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Harbor"})),
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
            "operation": "advisory", "inputRevision": 0,
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
    assert_eq!(blocked["run"]["blockedReason"], "executionBlocked");
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
    assert_eq!(retry_action["action"]["operation"], "advisory");
    assert_ne!(
        retry_action["action"]["target"]["stepToken"],
        next["action"]["target"]["stepToken"]
    );
    let (status, retry_task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "advisory", "inputRevision": 0,
            "threadId": "failed-brief-thread", "target": retry_action["action"]["target"],
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
    selected_chapters::three_chapters_with_feedback(repair_instruction).await;
}

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

#[tokio::test]
async fn generation_work_control_revision_survives_progress_and_direct_run_controls() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancel-generation.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Harbor"})),
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

    let work_uri = format!("/api/projects/{project_id}/work-tasks/work-{}", run["id"].as_str().unwrap());
    let (_, before) = call(&path, "GET", &work_uri, None, false).await;
    assert_eq!(before["controlRevision"], 0);
    let (status, _) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK);
    let (_, progressed) = call(&path, "GET", &work_uri, None, false).await;
    assert_eq!(progressed["controlRevision"], before["controlRevision"]);
    let (status, _) = call(&path, "POST", &format!("{work_uri}/control"),
        Some(json!({"command":"pause","expectedVersion":progressed["version"]})), false).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let pause_id = uuid::Uuid::new_v4().to_string();
    let pause_command = json!({"commandId":pause_id,"threadId":before["threadId"],"command":"pause","expectedControlRevision":before["controlRevision"]});
    let (status, paused) = call(&path, "POST", &format!("{work_uri}/control"),
        Some(pause_command.clone()), false).await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    assert_eq!(paused["controlRevision"], 1);
    let (status, _) = call(&path, "POST", &format!("{work_uri}/control"),
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":"","command":"cancel","expectedControlRevision":0})), false).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, paused_run) = call(&path, "GET", &run_uri, None, false).await;
    let (status, resumed) = call(&path, "POST", &format!("{run_uri}/control"),
        Some(json!({"command":"resume","expectedVersion":paused_run["version"]})), false).await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let (_, current) = call(&path, "GET", &work_uri, None, false).await;
    assert_eq!(current["controlRevision"], 2);
    let (status, receipt) = call(&path, "GET", &format!("{work_uri}/control-commands/{pause_id}"), None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["receipt"]["status"], "applied");
    assert_eq!(receipt["receipt"]["appliedControlRevision"], 1);
    let (status, _) = call(&path, "GET", &format!("{work_uri}/control-commands/{pause_id}"), None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, replay) = call(&path, "POST", &format!("{work_uri}/control"), Some(pause_command.clone()), false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["controlRevision"], 2);
    assert_eq!(replay["status"], "running");
    let mut changed = pause_command;
    changed["command"] = json!("cancel");
    let (status, _) = call(&path, "POST", &format!("{work_uri}/control"), Some(changed), false).await;
    assert!(!status.is_success());

    let (status, scheduled) = call(&path, "POST", &format!("{work_uri}/control"),
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":current["threadId"],"command":"pauseAfterStep","expectedControlRevision":current["controlRevision"]})), false).await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    assert_eq!(scheduled["status"], "paused");
    assert_eq!(scheduled["pauseAfterStep"]["remainingAttempts"], 0);
    let (_, drained_run) = call(&path, "GET", &run_uri, None, false).await;
    assert_eq!(drained_run["status"], "paused");
    let (status, next) = call(&path, "POST", &format!("{run_uri}/next"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert!(next["action"].is_null());
    let (status, resumed_after_drain) = call(&path, "POST", &format!("{run_uri}/control"),
        Some(json!({"command":"resume","expectedVersion":drained_run["version"]})), false).await;
    assert_eq!(status, StatusCode::OK, "{resumed_after_drain}");
    let (_, current) = call(&path, "GET", &work_uri, None, false).await;
    assert_eq!(current["status"], "running");
    assert!(current["pauseAfterStep"].is_null());

    let (status, cancelled) = call(&path, "POST", &format!("{work_uri}/control"),
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":current["threadId"],"command":"cancel","expectedControlRevision":current["controlRevision"]})), false).await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["controlRevision"], 5);
    assert_eq!(cancelled["status"], "cancelled");
}

#[tokio::test]
async fn generation_pause_after_step_preserves_failure_and_usage_until_explicit_resume() {
    let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("drain.sqlite3");
    let (_, created) = call(&path,"POST","/api/projects",Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"Drain"})),false).await;
    let project = created["project"]["id"].as_str().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let runs_uri = format!("/api/projects/{project}/generation-runs");
    let (status, run) = call(&path,"POST",&runs_uri,Some(json!({"expectedProjectRevision":0,"authorIntent":"写一部关于记忆的小说","targetChapters":1,"maxTaskAttempts":7,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{run}");
    let run_uri = format!("{runs_uri}/{}",run["id"].as_str().unwrap());
    let (_, next) = call(&path,"POST",&format!("{run_uri}/next"),None,false).await;
    let (status, task) = call(&path,"POST",&format!("/api/projects/{project}/tasks"),Some(json!({"operation":"advisory","inputRevision":0,"threadId":"drain-thread","target":next["action"]["target"],"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600,"authorIntent":"写一部关于记忆的小说"})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{task}");
    let task_id = task["id"].as_str().unwrap();
    let work_uri = format!("/api/projects/{project}/work-tasks/work-{}",run["id"].as_str().unwrap());
    let (_, before) = call(&path,"GET",&work_uri,None,false).await;
    let (status, pending) = call(&path,"POST",&format!("{work_uri}/control"),Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":before["threadId"],"command":"pauseAfterStep","expectedControlRevision":before["controlRevision"]})),false).await;
    assert_eq!(status,StatusCode::OK,"{pending}"); assert_eq!(pending["status"],"running"); assert_eq!(pending["pauseAfterStep"]["remainingAttempts"],1);
    let (_, gated) = call(&path,"POST",&format!("{run_uri}/next"),None,false).await;
    assert!(gated["action"].is_null()); assert_eq!(gated["run"]["activeTaskId"],task_id);
    let (status, failed) = call(&path,"PATCH",&format!("/api/projects/{project}/tasks/{task_id}"),Some(json!({"threadId":"drain-thread","status":"failed","error":"model disconnected"})),false).await;
    assert_eq!(status,StatusCode::OK,"{failed}");
    let (_, paused) = call(&path,"GET",&work_uri,None,false).await;
    assert_eq!(paused["status"],"paused"); assert_eq!(paused["pauseAfterStep"]["remainingAttempts"],0);
    let (_, paused_run) = call(&path,"GET",&run_uri,None,false).await;
    assert_eq!(paused_run["status"],"paused"); assert_eq!(paused_run["activeTaskId"],task_id);
    let (status, _) = call(&path,"POST",&format!("{run_uri}/control"),Some(json!({"command":"resume","expectedVersion":paused_run["version"]})),false).await;
    assert!(!status.is_success(),"Unsettled attempt must not resume");
    charge_budget(&path,&run_uri,task_id).await;
    let (_, charged) = call(&path,"GET",&run_uri,None,false).await;
    assert_eq!(charged["status"],"paused");
    let (status, resumed) = call(&path,"POST",&format!("{run_uri}/control"),Some(json!({"command":"resume","expectedVersion":charged["version"]})),false).await;
    assert_eq!(status,StatusCode::OK,"{resumed}"); assert_eq!(resumed["status"],"running");
    assert_eq!(resumed["providerBudget"],charged["providerBudget"]); assert_eq!(resumed["usedTaskAttempts"],1); assert_eq!(resumed["retryAttempts"],1);
    let (_, current) = call(&path,"GET",&work_uri,None,false).await;
    assert!(current["pauseAfterStep"].is_null());
    let (_, old) = call(&path,"GET",&format!("/api/projects/{project}/tasks/{task_id}"),None,false).await;
    assert_eq!(old["status"],"failed"); assert_eq!(old["error"],"model disconnected");
}
