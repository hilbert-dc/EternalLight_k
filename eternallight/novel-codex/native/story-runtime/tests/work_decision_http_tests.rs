use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository, TaskProviderBudget, TaskUpdate};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[path = "support/author_task_fixture.rs"]
mod fixture;
#[path = "../../../shared/novel-operation-digest.rs"]
mod operation_digest;
use fixture::AuthorTaskFixture;
const AUTHOR: &str = "continuation-author-12345678901234567890";
const CANDIDATE: &str = "continuation-candidate-12345678901234567";
const CAP: &str = "continuation-capability-123456789012345";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
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
    let decoded = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        assert!(
            status.is_client_error() || status.is_server_error(),
            "{method} {uri} returned non-JSON success {status}: {error}; body: {text}"
        );
        Value::String(text)
    });
    (status, decoded)
}
fn task(operation: &str, target: Value) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"operation":operation,"threadId":"parent-thread","inputRevision":0,"target":target,"authorIntent":"只调整创作方向；不改变其他规划或正文","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600})
}
async fn setup(path: &std::path::Path) -> (Router, String, String, Value) {
    setup_with_intent(path, "只调整创作方向；不改变其他规划或正文").await
}
async fn setup_with_intent(
    path: &std::path::Path,
    intent: &str,
) -> (Router, String, String, Value) {
    let app = router(fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"同额度协调"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap().to_owned();
    let base = format!("/api/projects/{project}");
    let (status, root) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        {
            let mut request = task("advisory", json!({"type":"project"}));
            request["authorIntent"] = json!(intent);
            request
        },
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{root}");
    (app, base, project, root)
}
fn terminal_receipt(path: &std::path::Path, id: &str, status: &str) -> (TaskProviderBudget, Value) {
    let db = rusqlite::Connection::open(path).unwrap();
    let encoded: String = db
        .query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [id],
            |r| r.get(/*idx*/ 0),
        )
        .unwrap();
    let grant: TaskProviderBudget = serde_json::from_str(&encoded).unwrap();
    let engine = path.with_extension("engine.sqlite3");
    let mut budget=eternallight_engine_core::PersistentBudget::new(&engine,id,json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"startedAt":1000},"now":1000})).unwrap();
    budget.complete(status).unwrap();
    let receipt = eternallight_engine_core::PersistentBudget::receipt(&engine, id).unwrap();
    (grant, receipt)
}
fn settle(path: &std::path::Path, project: &str, id: &str, status: &str) {
    let (grant, receipt) = terminal_receipt(path, id, status);
    SqliteStoryRepository::open(path)
        .unwrap()
        .charge_work_provider_usage(
            &ProjectId::parse(project).unwrap(),
            &grant.work_task_id,
            id,
            &receipt,
        )
        .unwrap();
}
async fn complete(app: &Router, base: &str, id: &str) {
    let (task_status, current) = call(
        app,
        "GET",
        &format!("{base}/tasks/{id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(task_status, StatusCode::OK, "{current}");
    let mut body = json!({"threadId":"parent-thread","summary":"真实步骤结果"});
    if current["inputManifest"]["workDecision"].is_object() {
        let mut first = status(app, base, &current).await;
        if first["deliveryReviewRequired"] == true {
            let mut page = first.clone();
            let mut evidence = Vec::new();
            loop {
                for receipt in page["receipts"]["items"].as_array().unwrap() {
                    if receipt["status"] != "completed" {
                        continue;
                    }
                    for output in receipt["outputs"].as_array().unwrap() {
                        if output["revisionId"].is_string() {
                            evidence.push(json!({"taskId":receipt["taskId"],"revisionId":output["revisionId"]}));
                        }
                    }
                }
                let Some(cursor) = page["pagination"]["nextCursor"].as_str() else {
                    break;
                };
                let (code, next) = call(app, "GET", &format!("{base}/tasks/{id}/work-decision?threadId=parent-thread&expectedRevision={}&cursor={}", current["inputRevision"], cursor.replace(':', "%3A")), Value::Null, CANDIDATE).await;
                assert_eq!(code, StatusCode::OK, "{next}");
                assert_eq!(next["version"], first["version"]);
                assert_eq!(next["sourceDigest"], first["sourceDigest"]);
                page = next;
            }
            if first["workPlan"]["activeGoalId"] == "fixture-work" && !evidence.is_empty() {
                acknowledge_fixture_goal(app, base, &current, json!([evidence[0]]), None).await;
                first = status(app, base, &current).await;
            }
            body["deliveryReview"] = json!({"expectedWorkVersion":first["version"],"expectedSourceDigest":first["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"本次授权范围的保存成果已核对","checkedScope":"实际保存回执；不声明文学质量认证","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":evidence});
        }
    }
    let (status, result) = call(
        app,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        body,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
}
async fn action(app: &Router, base: &str, parent: &str, operation: &str, key: &str) -> Value {
    let (_, task) = call(
        app,
        "GET",
        &format!("{base}/tasks/{parent}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    ensure_execution_plan(app, base, &task).await;
    let (status,result)=call(app,"POST",&format!("{base}/tasks/{parent}/actions"),json!({"taskId":parent,"threadId":"parent-thread","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":key,"operation":operation,"target":{"type":"artifact"},"instruction":"根据实际回执重新调整方向，保留其他内容"}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    result
}
async fn ensure_execution_plan(app: &Router, base: &str, task: &Value) {
    let id = task["id"].as_str().unwrap();
    let binding = format!(
        "threadId=parent-thread&expectedRevision={}",
        task["inputRevision"]
    );
    let (code, plan) = call(
        app,
        "GET",
        &format!("{base}/tasks/{id}/work-plan?{binding}"),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{plan}");
    if plan["version"] != 0 {
        return;
    }
    let (code,result)=call(app,"POST",&format!("{base}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":"parent-thread","expectedRevision":task["inputRevision"],"expectedPlanVersion":0,"idempotencyKey":"fixture-goals","plan":{"intentMode":"executeGoals","goals":[{"id":"fixture-work","title":"交付作者授权的候选成果","acceptance":"按原作者要求保存并核对真实产物","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"","checkScope":"本次授权候选","approach":"依真实保存回执选择必要行动","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null}]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
}
async fn acknowledge_fixture_goal(
    app: &Router,
    base: &str,
    task: &Value,
    evidence: Value,
    obstacle: Option<&str>,
) {
    let id = task["id"].as_str().unwrap();
    let binding = format!(
        "threadId=parent-thread&expectedRevision={}",
        task["inputRevision"]
    );
    let (code, plan) = call(
        app,
        "GET",
        &format!("{base}/tasks/{id}/work-plan?{binding}&goalId=fixture-work"),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{plan}");
    let mut goal = plan["goal"].clone();
    if goal["status"] == "satisfied" || goal["status"] == "blocked" {
        return;
    }
    goal["status"] = json!(if obstacle.is_some() {
        "blocked"
    } else {
        "satisfied"
    });
    goal["assessment"] = json!(obstacle.unwrap_or("当前已保存成果的回执已核对"));
    goal["evidence"] = if obstacle.is_some() {
        json!([])
    } else {
        evidence
    };
    let (code,result)=call(app,"POST",&format!("{base}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":"parent-thread","expectedRevision":task["inputRevision"],"expectedPlanVersion":plan["version"],"idempotencyKey":format!("goal-reviewed-{id}"),"plan":{"intentMode":"executeGoals","goals":[goal]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
}
async fn child(app: &Router, base: &str, parent: &str, action: &Value) -> Value {
    approved_child(app, base, parent, action).await
}
async fn approved_child(app: &Router, base: &str, parent: &str, action: &Value) -> Value {
    let id = action["id"].as_str().unwrap();
    let (status, approved) = call(
        app,
        "POST",
        &format!("{base}/tasks/{parent}/actions/{id}/approve"),
        json!({"expectedIntentRevision":0}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let mut target = action["target"].clone();
    target["requestedActionId"] = json!(id);
    target["requestedByTaskId"] = json!(parent);
    let (status, result) = call(
        app,
        "POST",
        &format!("{base}/tasks"),
        task(action["operation"].as_str().unwrap(), target),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    result
}
async fn save(app: &Router, base: &str, child: &Value, key: &str) {
    let id = child["id"].as_str().unwrap();
    let (status,result)=call(app,"POST",&format!("{base}/artifacts/brief"),json!({"taskId":id,"threadId":"parent-thread","expectedRevision":child["inputRevision"],"expectedCandidateRevisionId":child["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":key,"payload":{"premise":"来信改变港城，人物承担代价"}}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
}
async fn workspace(app: &Router, base: &str) -> Value {
    call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await
    .1
}
async fn decision(app: &Router, base: &str, source: &str) -> Value {
    let (_, work) = call(
        app,
        "GET",
        &format!("{base}/tasks/{source}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let workspace = workspace(app, base).await;
    let (status,result)=call(app,"POST",&format!("{base}/work-tasks/{}/decision",work["id"].as_str().unwrap()),json!({"expectedWorkVersion":work["version"],"expectedRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}),AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    result
}
async fn claim(app: &Router, base: &str, step: &Value, thread: &str) -> Value {
    let mut target = step["target"].clone();
    target["resumeStepId"] = step["id"].clone();
    let mut body = task(step["operation"].as_str().unwrap(), target);
    body["threadId"] = json!(thread);
    body["inputRevision"] = step["inputRevision"].clone();
    let (status, result) = call(app, "POST", &format!("{base}/tasks"), body, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{result}; claiming {step}");
    result
}
async fn status(app: &Router, base: &str, parent: &Value) -> Value {
    let id = parent["id"].as_str().unwrap();
    let (code, context) = call(
        app,
        "GET",
        &format!(
            "{base}/context?taskId={id}&threadId=parent-thread&projectRevision={}&purpose=planner",
            parent["inputRevision"]
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{context}");
    assert_eq!(context["taskSources"]["readTool"], "read_source_content");
    // A scripted decision observes the same original author source as a live
    // model. A context directory alone does not certify complete source reading.
    if let Some(source) = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["sectionPath"] == "/authorRequirementsSource")
    {
        let mut cursor = String::new();
        loop {
            let (code,page)=call(app,"GET",&format!("{base}/tasks/{id}/task-source?threadId=parent-thread&expectedRevision={}&sourceId={}{cursor}",parent["inputRevision"],source["sourceId"].as_str().unwrap()),Value::Null,CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{page}");
            let Some(next) = page["nextCursor"].as_str() else {
                break;
            };
            cursor = format!("&cursor={next}");
        }
    }
    let item = json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":json!({"content":[{"type":"text","text":context.to_string()}]}).to_string()});
    assert!(
        eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
            .unwrap()
            .as_u64()
            .unwrap()
            <= 9000
    );
    let (status, mut result) = call(
        app,
        "GET",
        &format!(
            "{base}/tasks/{id}/work-decision?threadId=parent-thread&expectedRevision={}",
            parent["inputRevision"]
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let mut cursor = result["pagination"]["nextCursor"].clone();
    while let Some(next) = cursor.as_str() {
        let (code,page) = call(app,"GET",&format!("{base}/tasks/{id}/work-decision?threadId=parent-thread&expectedRevision={}&cursor={next}",parent["inputRevision"]),Value::Null,CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        assert_eq!(page["version"], result["version"]);
        for field in ["requirements", "readyActions"] {
            result[field]
                .as_array_mut()
                .unwrap()
                .extend(page[field].as_array().unwrap().iter().cloned());
        }
        cursor = page["pagination"]["nextCursor"].clone();
    }
    result
}
async fn select(
    app: &Router,
    base: &str,
    parent: &Value,
    read: &Value,
    key: &str,
) -> (StatusCode, Value) {
    let id = parent["id"].as_str().unwrap();
    let mut request = json!({"taskId":id,"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":key,"instruction":"根据真实失败回执修正参数，保持原作者范围","idempotencyKey":"selected-action"});
    let action = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["actionKey"] == key)
        .unwrap_or(&Value::Null);
    if read["workTaskId"]
        .as_str()
        .unwrap()
        .starts_with("work-generation-")
        && matches!(
            action["operation"].as_str(),
            Some("generateChapter" | "writeNextChapter")
        )
    {
        request["chapterAcceptance"] = if read["chapterAcceptance"].is_object() {
            read["chapterAcceptance"].clone()
        } else {
            json!({"requirements":[{"id":"whole-chapter","text":"保存作者要求的章节"}],"length":{"requirement":"没有限定篇幅","minCharacters":null,"maxCharacters":null}})
        };
    }
    if action["operation"] == "finishChapter" {
        let (_,pack)=call(app,"GET",&format!("{base}/context?taskId={id}&threadId=parent-thread&projectRevision={}&purpose={}&byteBudget=60000",parent["inputRevision"],parent["purpose"].as_str().unwrap()),Value::Null,CANDIDATE).await;
        let source = pack["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["sectionPath"] == "/relevantText")
            .unwrap();
        let endpoint = format!(
            "{base}/tasks/{id}/task-source-content?threadId=parent-thread&expectedRevision={}&sourceId={}",
            parent["inputRevision"],
            source["sourceId"].as_str().unwrap()
        );
        let (_, index) = call(
            app,
            "GET",
            &format!("{endpoint}&view=index"),
            Value::Null,
            CANDIDATE,
        )
        .await;
        let record = index["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["identity"]["sourceRevisionId"] == action["sourceRevisionId"])
            .unwrap();
        let (_, page) = call(
            app,
            "GET",
            &format!(
                "{endpoint}&cursor={}",
                record["readCursor"].as_str().unwrap()
            ),
            Value::Null,
            CANDIDATE,
        )
        .await;
        let body = page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| {
                field["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("/body"))
            })
            .unwrap();
        let quote = body["text"]
            .as_str()
            .or_else(|| body["value"].as_str())
            .unwrap();
        let assessment = read["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "revisionOutcome")
            .unwrap()["assessment"]
            .clone();
        let (_, report) = call(
            app,
            "GET",
            &format!(
                "{base}/revision-outcomes/{}",
                assessment["writerTaskId"].as_str().unwrap()
            ),
            Value::Null,
            AUTHOR,
        )
        .await;
        request["chapterReview"] = json!({"sourceRevisionId":action["sourceRevisionId"],"assessmentRevisionId":report["revisionId"],"checks":read["chapterAcceptance"]["requirements"].as_array().unwrap().iter().map(|r|{let actual=report["assessment"]["outcomes"].as_array().unwrap().iter().find(|o|o["id"]==r["id"]).unwrap();json!({"criterionId":r["id"],"issueId":actual["issue"]["issueId"]})}).collect::<Vec<_>>(),"length":read["chapterAcceptance"]["length"]});
    }
    call(
        app,
        "POST",
        &format!("{base}/tasks/{id}/work-decision/actions"),
        request,
        CANDIDATE,
    )
    .await
}

// The provider's terminal outcome and the Native deliverable outcome are
// independent facts. These fixtures use the same public settlement path as Host;
// they never rewrite a completed charge to make a failed Task retryable.
async fn missing_output_worker(
    path: &std::path::Path,
) -> (Router, String, String, Value, Value, TaskProviderBudget) {
    let (app, base, project, root) =
        setup_with_intent(path, "整理人物背景并保存创作简报候选，不采用、不发布").await;
    let id = root["id"].as_str().unwrap();
    let first = action(&app, &base, id, "generateBrief", "original-brief").await;
    let root_grant = read_provider_grant(&app, &base, id).await;
    complete(&app, &base, id).await;
    let mut root_engine = recorded_provider(path, &root_grant, "empty");
    root_engine.complete("completed").unwrap();
    let (code, charged) = charge_provider(&app, &base, &root_grant, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{charged}");
    let worker = child(&app, &base, id, &first).await;
    let worker_id = worker["id"].as_str().unwrap();
    let grant = read_provider_grant(&app, &base, worker_id).await;
    let (code, bound) = call(
        &app,
        "POST",
        &format!("{base}/tasks/{worker_id}/turn"),
        json!({"threadId":"parent-thread","turnId":"provider-completed-turn"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{bound}");
    patch_missing_output_task(&app, &base, worker_id, "running", None).await;
    (app, base, project, root, worker, grant)
}

async fn read_provider_grant(app: &Router, base: &str, id: &str) -> TaskProviderBudget {
    let (code, grant) = call(
        app,
        "GET",
        &format!("{base}/tasks/{id}/provider-budget"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{grant}");
    serde_json::from_value(grant).unwrap()
}

fn recorded_provider(
    path: &std::path::Path,
    grant: &TaskProviderBudget,
    mode: &str,
) -> eternallight_engine_core::PersistentBudget {
    let budget_id = if mode == "wrongBudget" {
        format!("foreign-{}", grant.budget_id)
    } else {
        grant.budget_id.clone()
    };
    let mut engine = eternallight_engine_core::PersistentBudget::new(
        &path.with_extension("engine.sqlite3"),
        &grant.task_id,
        json!({"options":{"id":budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"startedAt":1000},"now":1000}),
    )
    .unwrap();
    if mode != "empty" {
        let reservation = engine
            .apply(
                "budget.reserve",
                json!({"parameters":{"estimatedInputTokens":20,"maxOutputTokens":50},"now":1001}),
            )
            .unwrap();
        if mode != "active" {
            let usage = match mode {
                "unknown" => json!({}),
                "overrun" => json!({"total_tokens":grant.max_tokens.unwrap()+1}),
                _ => json!({"total_tokens":125}),
            };
            engine
                .apply(
                    "budget.settle",
                    json!({"id":reservation,"usage":usage,"now":1002}),
                )
                .unwrap();
        }
        if mode == "known" {
            let second = engine
                .apply(
                    "budget.reserve",
                    json!({"parameters":{"estimatedInputTokens":20,"maxOutputTokens":50},"now":1003}),
                )
                .unwrap();
            engine
                .apply(
                    "budget.settle",
                    json!({"id":second,"usage":{"total_tokens":125},"now":1004}),
                )
                .unwrap();
        }
    }
    engine
}

async fn charge_provider(
    app: &Router,
    base: &str,
    grant: &TaskProviderBudget,
    actor: &str,
) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("{base}/work-tasks/{}/provider-usage", grant.work_task_id),
        json!({"taskId":grant.task_id}),
        actor,
    )
    .await
}

async fn patch_missing_output_task(
    app: &Router,
    base: &str,
    id: &str,
    status: &str,
    error: Option<&str>,
) {
    let (code, task) = call(
        app,
        "PATCH",
        &format!("{base}/tasks/{id}"),
        json!({"threadId":"parent-thread","status":status,"error":error}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{task}");
    assert_eq!(task["status"], status);
}

async fn reject_empty_completion(app: &Router, base: &str, id: &str) {
    let (code, rejected) = call(
        app,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":"parent-thread","summary":"模型请求已结束，但没有保存简报"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["code"], "conflict", "{rejected}");
    assert_eq!(
        rejected["error"]["message"], "TaskOutputMissing",
        "{rejected}"
    );
}

fn missing_output_rows(path: &std::path::Path) -> Value {
    use rusqlite::types::ValueRef;
    let db = rusqlite::Connection::open(path).unwrap();
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut result = serde_json::Map::new();
    for table in tables {
        let mut statement = db
            .prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                table.replace('"', "\"\"")
            ))
            .unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                Ok(Value::Array(
                    (0..columns)
                        .map(|index| match row.get_ref(index).unwrap() {
                            ValueRef::Null => Value::Null,
                            ValueRef::Integer(value) => json!(value),
                            ValueRef::Real(value) => json!(value),
                            ValueRef::Text(value) => json!(std::str::from_utf8(value).unwrap()),
                            ValueRef::Blob(value) => json!(value),
                        })
                        .collect(),
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        result.insert(table, json!(rows));
    }
    Value::Object(result)
}

fn missing_output_facts(path: &std::path::Path) -> Value {
    json!({"native":missing_output_rows(path),"engine":missing_output_rows(&path.with_extension("engine.sqlite3"))})
}

fn missing_output_authority(path: &std::path::Path) -> Value {
    let rows = missing_output_rows(path);
    json!({"authorMessages":rows["story_author_messages_v1"],"authorizations":rows["story_work_budget_authorizations_v1"]})
}

async fn missing_output_event(app: &Router, base: &str, source: &str) -> Value {
    let (code, events) = call(
        app,
        "GET",
        &format!("{base}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{events}");
    events["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["sourceAttemptId"] == source)
        .unwrap()
        .clone()
}

async fn claim_missing_output_event(app: &Router, base: &str, event: &Value) -> Value {
    let work_uri = format!(
        "{base}/work-tasks/{}",
        event["workTaskId"].as_str().unwrap()
    );
    let (code, work) = call(app, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{work}");
    let workspace = workspace(app, base).await;
    let (code, result) = call(
        app,
        "POST",
        &format!("{base}/work-execution-events/{}/claim", event["eventId"].as_str().unwrap()),
        json!({"hostSessionId":"missing-output-host","expectedWorkVersion":work["version"],"expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}),
        AUTHOR,
    ).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}

#[tokio::test]
async fn completed_provider_missing_native_output_relays_original_event_and_delivers_same_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp
        .path()
        .join("provider-completed-output-missing.sqlite3");
    let (app, base, project, root, worker, grant) = missing_output_worker(&path).await;
    let root_id = root["id"].as_str().unwrap();
    let worker_id = worker["id"].as_str().unwrap();
    let authority = missing_output_authority(&path);
    let initial_workspace = workspace(&app, &base).await;
    let mut engine = recorded_provider(&path, &grant, "known");
    engine.complete("completed").unwrap();
    let engine_receipt = eternallight_engine_core::PersistentBudget::receipt(
        &path.with_extension("engine.sqlite3"),
        worker_id,
    )
    .unwrap();
    assert_eq!(engine_receipt["status"], "completed");
    assert_eq!(engine_receipt["usage"]["requests"], 2);
    assert_eq!(engine_receipt["usage"]["tokens"], 250.0);
    reject_empty_completion(&app, &base, worker_id).await;
    patch_missing_output_task(&app, &base, worker_id, "failed", Some("TaskOutputMissing")).await;
    let event = missing_output_event(&app, &base, worker_id).await;
    assert_eq!(event["workTaskId"], grant.work_task_id);
    assert_eq!(event["rootThreadId"], "parent-thread");
    let before = missing_output_facts(&path);
    let blocked = claim_missing_output_event(&app, &base, &event).await;
    assert_eq!(blocked["admission"], "blocked");
    assert_eq!(blocked["blockedReason"], "unsettledUsage");
    assert_eq!(missing_output_facts(&path), before);
    assert_eq!(
        decision(&app, &base, root_id).await["stopReason"],
        "pendingRecovery"
    );

    // Only the trusted reader can apply the actual provider charge. Neither a
    // different identity nor model-supplied usage may waive these facts.
    let before = missing_output_facts(&path);
    assert_eq!(
        charge_provider(&app, &base, &grant, CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    for (work, body) in [
        (grant.work_task_id.clone(), json!({"taskId":"foreign-task"})),
        ("foreign-work".into(), json!({"taskId":worker_id})),
        (
            grant.work_task_id.clone(),
            json!({"taskId":worker_id,"usedTokens":0}),
        ),
    ] {
        let (code, rejection) = call(
            &app,
            "POST",
            &format!("{base}/work-tasks/{work}/provider-usage"),
            body,
            AUTHOR,
        )
        .await;
        assert!(!code.is_success(), "{rejection}");
        assert_eq!(missing_output_facts(&path), before);
    }
    let (code, charged) = charge_provider(&app, &base, &grant, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{charged}");
    assert_eq!(charged["usedRequests"], 2);
    assert_eq!(charged["usedTokens"], 250);
    let charge = charged["charges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|charge| charge["taskId"] == worker_id)
        .unwrap();
    assert_eq!(
        charge,
        &json!({"taskId":worker_id,"budgetId":grant.budget_id,"status":"completed","requests":2,"tokens":250,"unknownTokens":0,"overrun":false})
    );
    let settled = missing_output_facts(&path);
    assert_eq!(
        charge_provider(&app, &base, &grant, AUTHOR).await,
        (StatusCode::OK, charged.clone())
    );
    assert_eq!(missing_output_facts(&path), settled);
    let (code, failed) = call(
        &app,
        "GET",
        &format!("{base}/tasks/{worker_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["outputRefs"], json!([]));
    let unchanged_workspace = workspace(&app, &base).await;
    for field in [
        "revision",
        "sourceDigest",
        "headSnapshotId",
        "artifacts",
        "chapters",
    ] {
        assert_eq!(unchanged_workspace[field], initial_workspace[field]);
    }

    let claimed = claim_missing_output_event(&app, &base, &event).await;
    assert_eq!(claimed["admission"], "claimed");
    assert_eq!(claimed["blockedReason"], Value::Null);
    let replay = claim_missing_output_event(&app, &base, &event).await;
    assert_eq!(replay["event"]["claimId"], claimed["event"]["claimId"]);
    assert_eq!(replay["lease"]["leaseId"], claimed["lease"]["leaseId"]);
    let correction = decision(&app, &base, root_id).await;
    assert_eq!(correction["workTaskId"], grant.work_task_id);
    let parent = claim(&app, &base, &correction["step"], "parent-thread").await;
    assert_eq!(
        parent["inputManifest"]["authorIntent"],
        "整理人物背景并保存创作简报候选，不采用、不发布"
    );
    let acknowledgement = json!({"hostSessionId":"missing-output-host","claimId":claimed["event"]["claimId"],"leaseId":claimed["lease"]["leaseId"],"consumerStepId":correction["step"]["id"]});
    let ack_uri = format!(
        "{base}/work-execution-events/{}/ack",
        event["eventId"].as_str().unwrap()
    );
    let (code, ack) = call(&app, "POST", &ack_uri, acknowledgement.clone(), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{ack}");
    assert_eq!(ack["state"], "consumed");
    assert_eq!(
        call(&app, "POST", &ack_uri, acknowledgement, AUTHOR).await,
        (StatusCode::OK, ack)
    );
    let read = status(&app, &base, &parent).await;
    let receipt = read["receipts"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|receipt| receipt["taskId"] == worker_id)
        .unwrap();
    assert_eq!(receipt["failureCode"], "TaskOutputMissing");
    assert_eq!(receipt["outputs"], json!([]));
    assert_eq!(read["canFinish"], false);
    let retry = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["operation"] == "generateBrief")
        .unwrap();
    assert_eq!(retry["recovery"]["failedTaskId"], worker_id);
    assert_eq!(retry["recovery"]["sameConditionFailures"], 1);
    let (code, chosen) = select(
        &app,
        &base,
        &parent,
        &read,
        retry["actionKey"].as_str().unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{chosen}");
    complete(&app, &base, parent["id"].as_str().unwrap()).await;
    settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
    let retried = claim(&app, &base, &chosen["step"], "parent-thread").await;
    let retry_grant = read_provider_grant(&app, &base, retried["id"].as_str().unwrap()).await;
    assert_eq!(retry_grant.work_task_id, grant.work_task_id);
    assert_eq!(
        retry_grant.max_requests,
        grant.max_requests.map(|maximum| maximum - 2)
    );
    assert_eq!(
        retry_grant.max_tokens,
        grant.max_tokens.map(|maximum| maximum - 250)
    );
    assert_eq!(
        retried["inputManifest"]["authorIntent"],
        parent["inputManifest"]["authorIntent"]
    );
    assert_eq!(
        retried["inputManifest"]["expectedCandidateRevisionId"],
        worker["inputManifest"]["expectedCandidateRevisionId"]
    );
    save(&app, &base, &retried, "actual-recovered-brief").await;
    complete(&app, &base, retried["id"].as_str().unwrap()).await;
    let mut retry_engine = recorded_provider(&path, &retry_grant, "empty");
    retry_engine.complete("completed").unwrap();
    let (code, recovered_budget) = charge_provider(&app, &base, &retry_grant, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{recovered_budget}");
    assert_eq!(recovered_budget["usedRequests"], 2);
    assert_eq!(recovered_budget["usedTokens"], 250);
    assert_eq!(
        recovered_budget["charges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|charge| charge["taskId"] == worker_id),
        Some(charge)
    );
    let next = decision(&app, &base, root_id).await;
    let final_parent = claim(&app, &base, &next["step"], "parent-thread").await;
    let final_read = status(&app, &base, &final_parent).await;
    assert_eq!(final_read["canFinish"], true);
    let output = &final_read["receipts"]["items"][0]["outputs"][0];
    assert!(output["revisionId"].is_string());
    let actual = workspace(&app, &base).await;
    assert_eq!(
        actual["artifacts"]["brief"]["candidate"]["id"],
        output["revisionId"]
    );
    assert_eq!(
        actual["artifacts"]["brief"]["candidate"]["payload"]["premise"],
        "来信改变港城，人物承担代价"
    );
    assert!(actual["artifacts"]["brief"]["accepted"].is_null());
    complete(&app, &base, final_parent["id"].as_str().unwrap()).await;
    settle(
        &path,
        &project,
        final_parent["id"].as_str().unwrap(),
        "completed",
    );
    assert_eq!(
        decision(&app, &base, root_id).await["stopReason"],
        "completed"
    );
    assert_eq!(missing_output_authority(&path), authority);
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_work_tasks_v1", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_tasks_v2 WHERE task_id=?1 AND status='failed' AND error='TaskOutputMissing' AND output_refs_json='[]'", [worker_id], |row| row.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_outputs_v2 WHERE task_id=?1",
            [worker_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_tool_operations_v1 WHERE task_id=?1",
            [worker_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn completed_provider_missing_output_recovery_preserves_all_uncertainty_and_authority_fences()
{
    for fence in [
        "otherFailure",
        "unsettled",
        "activeProvider",
        "unknownUsage",
        "overrun",
        "runningTask",
        "cancelledTask",
        "acceptedWrite",
        "committedWrite",
        "savedOutput",
        "wrongBudget",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("completed-provider-fences.sqlite3");
        let (app, base, project, root, worker, grant) = missing_output_worker(&path).await;
        let worker_id = worker["id"].as_str().unwrap();
        let mode = match fence {
            "activeProvider" => "active",
            "unknownUsage" => "unknown",
            "overrun" => "overrun",
            "wrongBudget" => "wrongBudget",
            _ => "known",
        };
        let mut engine = recorded_provider(&path, &grant, mode);
        if fence != "activeProvider" {
            engine.complete("completed").unwrap();
        }
        if fence == "acceptedWrite" {
            SqliteStoryRepository::open(&path)
                .unwrap()
                .bind_recovery_operation(novel_codex_story_core::RecoveryOperationBinding {
                    project_id: ProjectId::parse(&project).unwrap(),
                    task_id: worker_id.into(),
                    thread_id: "parent-thread".into(),
                    turn_id: "provider-completed-turn".into(),
                    call_id: "interrupted-save".into(),
                    tool_name: "save_brief".into(),
                    request_hash: format!("{:x}", Sha256::digest(b"interrupted original save")),
                    idempotency_ref: None,
                    now: 1,
                })
                .unwrap();
        }
        if matches!(fence, "committedWrite" | "savedOutput") {
            if fence == "committedWrite" {
                let payload = json!({"taskId":worker_id,"threadId":"parent-thread","expectedRevision":worker["inputRevision"],"expectedCandidateRevisionId":worker["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":"actually-committed-brief","payload":{"premise":"真实保存后失败，不能当零产物重试"}});
                let mut arguments = payload.clone();
                arguments["projectId"] = json!(project);
                let response = app
                    .clone()
                    .author_fixture_request(
                        Request::builder()
                            .method("POST")
                            .uri(format!("{base}/artifacts/brief"))
                            .header("authorization", format!("Bearer {CANDIDATE}"))
                            .header("x-novel-task-capability", CAP)
                            .header("content-type", "application/json")
                            .header("x-novel-tool-name", "save_brief")
                            .header("x-novel-root-thread", "parent-thread")
                            .header("x-novel-caller-thread", "parent-thread")
                            .header("x-novel-turn", "provider-completed-turn")
                            .header("x-novel-tool-call", "actually-committed-save")
                            .header(
                                "x-novel-request-hash",
                                operation_digest::operation_digest("save_brief", &arguments)
                                    .unwrap(),
                            )
                            .body(Body::from(payload.to_string()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let code = response.status();
                let saved: Value = serde_json::from_slice(
                    &response.into_body().collect().await.unwrap().to_bytes(),
                )
                .unwrap();
                assert_eq!(code, StatusCode::CREATED, "{saved}");
            } else {
                save(&app, &base, &worker, "actual-partial-output").await;
            }
        } else if fence != "runningTask" {
            reject_empty_completion(&app, &base, worker_id).await;
        }
        if fence != "runningTask" {
            patch_missing_output_task(
                &app,
                &base,
                worker_id,
                if fence == "cancelledTask" {
                    "cancelled"
                } else {
                    "failed"
                },
                Some(if fence == "otherFailure" {
                    "invalid_tool_arguments"
                } else {
                    "TaskOutputMissing"
                }),
            )
            .await;
        }
        if fence != "unsettled" {
            let before = missing_output_facts(&path);
            let (code, charged) = charge_provider(&app, &base, &grant, AUTHOR).await;
            if matches!(fence, "activeProvider" | "runningTask" | "wrongBudget") {
                assert!(code.is_client_error(), "{fence}: {charged}");
                assert_eq!(missing_output_facts(&path), before);
            } else {
                assert_eq!(code, StatusCode::OK, "{fence}: {charged}");
                let charge = charged["charges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|charge| charge["taskId"] == worker_id)
                    .unwrap();
                assert_eq!(charge["status"], "completed");
                if fence == "unknownUsage" {
                    assert_eq!(charge["unknownTokens"], 70);
                }
                if fence == "overrun" {
                    assert_eq!(charge["overrun"], true);
                }
            }
        }
        let event = missing_output_event(
            &app,
            &base,
            if fence == "runningTask" {
                root["id"].as_str().unwrap()
            } else {
                worker_id
            },
        )
        .await;
        let before = missing_output_facts(&path);
        let blocked = claim_missing_output_event(&app, &base, &event).await;
        assert_eq!(blocked["admission"], "blocked", "{fence}: {blocked}");
        assert_eq!(
            blocked["blockedReason"],
            match fence {
                "runningTask" => "activeAttempt",
                "cancelledTask" => "authorStopped",
                "acceptedWrite" => "unknownWrite",
                "unsettled" | "activeProvider" | "wrongBudget" => "unsettledUsage",
                "unknownUsage" => "unknownUsage",
                "overrun" => "budgetExhausted",
                _ => "executionBlocked",
            },
            "{fence}: {blocked}"
        );
        assert_eq!(blocked["lease"], Value::Null);
        assert_eq!(missing_output_facts(&path), before);
        let correction = decision(&app, &base, root["id"].as_str().unwrap()).await;
        assert!(correction["step"].is_null(), "{fence}: {correction}");
        assert_eq!(
            correction["stopReason"],
            match fence {
                "runningTask" | "cancelledTask" => "workNotReady",
                "unsettled" | "activeProvider" | "acceptedWrite" | "wrongBudget" =>
                    "pendingRecovery",
                "unknownUsage" => "unknownUsage",
                _ => "executionBlocked",
            },
            "{fence}: {correction}"
        );
        assert_eq!(missing_output_facts(&path), before);
    }
}

#[tokio::test]
async fn author_resume_keeps_failed_attempt_for_same_work_model_correction() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("same-step.sqlite3");
    let (app, base, project, root) = setup(&path).await;
    let id = root["id"].as_str().unwrap();
    let first = action(&app, &base, id, "generateBrief", "retry-original").await;
    complete(&app, &base, id).await;
    settle(&path, &project, id, "completed");
    let failed = child(&app, &base, id, &first).await;
    let failed_id = failed["id"].as_str().unwrap();
    SqliteStoryRepository::open(&path)
        .unwrap()
        .update_task(
            &ProjectId::parse(&project).unwrap(),
            failed_id,
            "parent-thread",
            TaskUpdate {
                status: "failed".into(),
                error: Some("provider_output_limit".into()),
                result_summary: None,
                now: 1,
            },
        )
        .unwrap();
    settle(&path, &project, failed_id, "failed");
    let work_uri = format!("{base}/work-tasks/work-{id}");
    let (_, work) = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await;
    let (code, resumed) = call(
        &app,
        "POST",
        &format!("{work_uri}/control"),
        json!({"expectedVersion":work["version"],"command":"resume"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{resumed}");
    let step = resumed["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == first["id"])
        .unwrap();
    assert_eq!(step["status"], "blocked");
    assert_eq!(step["currentAttemptId"], failed_id);
    assert_eq!(step["attempts"].as_array().unwrap().len(), 1);
    let (replay_code, _) = call(
        &app,
        "POST",
        &format!("{work_uri}/control"),
        json!({"expectedVersion":work["version"],"command":"resume"}),
        AUTHOR,
    )
    .await;
    assert!(replay_code.is_client_error());
    let correction = decision(&app, &base, id).await;
    let parent = claim(&app, &base, &correction["step"], "parent-thread").await;
    let read = status(&app, &base, &parent).await;
    let retry = &read["readyActions"][0];
    assert_eq!(retry["recovery"]["planRequired"], true);
    let body = json!({"taskId":parent["id"],"threadId":"parent-thread", "expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":retry["actionKey"],"instruction":"缩减重复上下文，保存完整简报后交还", "idempotencyKey":"resume-correction","recoveryPlan":{"failedTaskId":failed_id,"cause":"单次模型输出达到上限，尚无保存结果","change":"减少重复上下文，只提交完整简报，保存后立即完成","verification":"读回简报与其保存回执"}});
    let (code, selected) = call(
        &app,
        "POST",
        &format!(
            "{base}/tasks/{}/work-decision/actions",
            parent["id"].as_str().unwrap()
        ),
        body,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{selected}");
    complete(&app, &base, parent["id"].as_str().unwrap()).await;
    settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
    let retried = claim(&app, &base, &selected["step"], "parent-thread").await;
    let retried_id = retried["id"].as_str().unwrap();
    save(&app, &base, &retried, "same-step-save").await;
    complete(&app, &base, retried_id).await;
    settle(&path, &project, retried_id, "completed");
    let db = rusqlite::Connection::open(&path).unwrap();
    let action_state: (String, String) = db
        .query_row(
            "SELECT status,result_task_id FROM story_task_action_requests_v1 WHERE action_id=?1",
            [first["id"].as_str().unwrap()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(action_state, ("failed".into(), failed_id.into()));
    // The new correction output remains authoritative even if the redundant
    // original candidate projection was interrupted and left queued.
    db.execute("UPDATE story_task_action_requests_v1 SET status='queued',result_task_id=NULL WHERE action_id=?1",[first["id"].as_str().unwrap()]).unwrap();
    drop(db);
    let next = decision(&app, &base, id).await;
    let parent = claim(&app, &base, &next["step"], "parent-thread").await;
    let read = status(&app, &base, &parent).await;
    assert_eq!(read["canFinish"], true, "{read}");
    assert_eq!(read["deliveryReviewRequired"], true);
    assert_eq!(read["coverage"]["pendingRequirements"], 0);
    complete(&app, &base, parent["id"].as_str().unwrap()).await;
    settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
    assert_eq!(decision(&app, &base, id).await["stopReason"], "completed");
}

#[tokio::test]
async fn saved_child_receipt_parent_and_finish_preserve_original_work_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("success.sqlite3");
    let (app, base, project, root) = setup(&path).await;
    let id = root["id"].as_str().unwrap();
    let mut goal = json!({"id":"brief","title":"创作简报","acceptance":"简报已保存并符合作者要求","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"故事承诺是什么","checkScope":"创作简报","approach":"先生成再核对","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null});
    let (code, saved_plan) = call(&app, "POST", &format!("{base}/tasks/{id}/work-plan"), json!({"taskId":id,"threadId":"parent-thread","expectedRevision":root["inputRevision"],"expectedPlanVersion":0,"idempotencyKey":"initial-plan","plan":{"intentMode":"executeGoals","goals":[goal]}}), CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{saved_plan}");
    let first = action(&app, &base, id, "generateBrief", "first").await;
    let read = status(&app, &base, &root).await;
    assert_eq!(read["registeredActions"][0]["id"], first["id"]);
    assert_eq!(read["registeredActions"][0]["operation"], "generateBrief");
    assert_eq!(read["registeredActions"][0]["status"], "proposed");

    complete(&app, &base, id).await;
    settle(&path, &project, id, "completed");
    let worker = child(&app, &base, id, &first).await;
    save(&app, &base, &worker, "saved").await;
    let worker_id = worker["id"].as_str().unwrap();
    complete(&app, &base, worker_id).await;
    assert_eq!(
        decision(&app, &base, id).await["stopReason"],
        "pendingRecovery"
    );
    settle(&path, &project, worker_id, "completed");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("INSERT INTO story_tool_operations_v1 VALUES ('unknown-completion',?1,?2,'parent-thread','turn','call','complete_task','hash',NULL,'accepted',NULL,1,1)", rusqlite::params![project,worker_id]).unwrap();
    drop(db);
    let next = decision(&app, &base, id).await;
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT state FROM story_tool_operations_v1 WHERE operation_id='unknown-completion'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "committed"
    );
    drop(db);
    assert_eq!(decision(&app, &base, id).await, next);
    let parent = claim(&app, &base, &next["step"], "parent-thread").await;
    assert_eq!(
        parent["inputManifest"]["authorIntent"],
        root["inputManifest"]["authorIntent"]
    );
    let read = status(&app, &base, &parent).await;
    assert_eq!(read["canFinish"], true);
    assert_eq!(read["canReportPartial"], true);
    assert_eq!(read["deliveryReviewRequired"], true);
    assert_eq!(read["workPlan"]["version"], 1);
    let parent_id = parent["id"].as_str().unwrap();
    let (code, persisted) = call(&app, "GET", &format!("{base}/tasks/{parent_id}/work-plan?threadId=parent-thread&expectedRevision={}&goalId=brief", parent["inputRevision"]), Value::Null, CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{persisted}");
    assert_eq!(persisted["goal"], goal);
    let evidence = json!([{"taskId":worker_id,"revisionId":read["receipts"]["items"][0]["outputs"][0]["revisionId"]}]);
    // Methods may remain unfinished; the final review still cannot omit a real
    // saved output. Plan status itself no longer blocks author delivery.
    let (code, rejected) = call(&app,"POST",&format!("{base}/tasks/{parent_id}/complete"),json!({"threadId":"parent-thread","summary":"错误地声称已完成","deliveryReview":{"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"完成","checkedScope":"简报","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":[]}}),CANDIDATE).await;
    assert!(!code.is_success(), "{rejected}");
    goal["status"] = json!("satisfied");
    goal["assessment"] = json!("实际保存内容满足故事承诺");
    goal["evidence"] = evidence;
    let (code, updated) = call(&app,"POST",&format!("{base}/tasks/{parent_id}/work-plan"),json!({"taskId":parent_id,"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedPlanVersion":1,"idempotencyKey":"reviewed-plan","plan":{"intentMode":"executeGoals","goals":[goal]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{updated}");
    let read = status(&app, &base, &parent).await;
    assert_eq!(read["canFinish"], true);
    assert_eq!(read["readyActions"], json!([]));
    assert_eq!(read["receipts"]["items"][0]["taskId"], worker_id);
    assert_eq!(read["receipts"]["items"][0]["outputs"][0]["kind"], "brief");
    assert!(read.to_string().len() <= 6500);
    assert_eq!(
        read["pagination"],
        json!({"page":0,"nextCursor":null,"hasMore":false})
    );
    let (cursor_status, _) = call(&app, "GET", &format!("{base}/tasks/{}/work-decision?threadId=parent-thread&expectedRevision={}&cursor=1%3Ainvalid", parent["id"].as_str().unwrap(), parent["inputRevision"]), Value::Null, CANDIDATE).await;
    assert_eq!(cursor_status, StatusCode::BAD_REQUEST);

    complete(&app, &base, parent["id"].as_str().unwrap()).await;
    settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
    assert_eq!(decision(&app, &base, id).await["stopReason"], "completed");
    let db = rusqlite::Connection::open(path).unwrap();
    let counts:(i64,i64,i64)=db.query_row("SELECT (SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?,r.get(/*idx*/2)?))).unwrap();
    assert_eq!(counts, (1, 1, 1));
}
#[tokio::test]
async fn known_zero_failure_agent_selects_exact_retry_then_saves_without_new_grant() {
    for (retry_succeeds, failure_code) in [false, true].into_iter().flat_map(|success| {
        [
            "invalid_tool_arguments",
            "tool_argument_correction_limit",
            "provider_output_limit",
        ]
        .map(|code| (success, code))
    }) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("failure.sqlite3");
        let (app, base, project, root) = setup(&path).await;
        let id = root["id"].as_str().unwrap();
        let first = action(&app, &base, id, "generateBrief", "first").await;
        complete(&app, &base, id).await;
        settle(&path, &project, id, "completed");
        let worker = child(&app, &base, id, &first).await;
        let worker_id = worker["id"].as_str().unwrap();
        SqliteStoryRepository::open(&path)
            .unwrap()
            .update_task(
                &ProjectId::parse(&project).unwrap(),
                worker_id,
                "parent-thread",
                TaskUpdate {
                    status: "failed".into(),
                    error: Some(format!("{failure_code}; private prose")),
                    result_summary: None,
                    now: 1,
                },
            )
            .unwrap();
        assert_eq!(
            decision(&app, &base, id).await["stopReason"],
            "pendingRecovery"
        );
        settle(&path, &project, worker_id, "failed");
        let next = decision(&app, &base, id).await;
        let parent = claim(&app, &base, &next["step"], "parent-thread").await;
        let read = status(&app, &base, &parent).await;
        assert_eq!(read["canFinish"], false);
        assert_eq!(read["receipts"]["items"][0]["outputs"], json!([]));
        assert_eq!(read["receipts"]["items"][0]["failureCode"], failure_code);
        assert!(!read.to_string().contains("private prose"));
        assert!(
            !select(&app, &base, &parent, &read, "forged-key")
                .await
                .0
                .is_success()
        );
        let key = read["readyActions"][0]["actionKey"].as_str().unwrap();
        assert_eq!(read["readyActions"][0]["operation"], "generateBrief");
        let (code, chosen) = if failure_code == "provider_output_limit" {
            assert_eq!(read["readyActions"][0]["recovery"]["planRequired"], true);
            assert!(
                !select(&app, &base, &parent, &read, key)
                    .await
                    .0
                    .is_success()
            );
            let endpoint = format!(
                "{base}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            );
            let body = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":key,"instruction":"精简上下文并保存完整简报","idempotencyKey":"output-limit-correction","recoveryPlan":{"failedTaskId":worker_id,"cause":"单次输出上限导致没有完成保存","change":"仅保留简报必需上下文，提交完整简报后立即完成","verification":"核对实际保存的完整简报与回执"}});
            let result = call(&app, "POST", &endpoint, body.clone(), CANDIDATE).await;
            assert_eq!(call(&app, "POST", &endpoint, body, CANDIDATE).await, result);
            result
        } else {
            let result = select(&app, &base, &parent, &read, key).await;
            assert_eq!(select(&app, &base, &parent, &read, key).await, result);
            result
        };
        assert_eq!(code, StatusCode::OK, "{chosen}");
        complete(&app, &base, parent["id"].as_str().unwrap()).await;
        settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
        let worker = claim(&app, &base, &chosen["step"], "parent-thread").await;
        if !retry_succeeds {
            let retry_id = worker["id"].as_str().unwrap();
            SqliteStoryRepository::open(&path)
                .unwrap()
                .update_task(
                    &ProjectId::parse(&project).unwrap(),
                    retry_id,
                    "parent-thread",
                    TaskUpdate {
                        status: "failed".into(),
                        error: Some("TaskOutputMissing".into()),
                        result_summary: None,
                        now: 2,
                    },
                )
                .unwrap();
            settle(&path, &project, retry_id, "failed");
            let replanning = decision(&app, &base, id).await;
            assert!(replanning["step"].is_object(), "{replanning}");
            let parent = claim(&app, &base, &replanning["step"], "parent-thread").await;
            let read = status(&app, &base, &parent).await;
            assert_eq!(read["canFinish"], false, "{read}");
            assert_eq!(
                read["canReportPartial"], true,
                "honest partial delivery does not require a method plan to be blocked"
            );
            let (code,retired)=call(&app,"POST",&format!("{base}/tasks/{}/work-plan",parent["id"].as_str().unwrap()),json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedPlanVersion":read["workPlan"]["version"],"idempotencyKey":"retire-methods","plan":{"intentMode":"planningOnly","goals":[]}}),CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{retired}");
            let read = status(&app, &base, &parent).await;
            assert_eq!(
                read["canFinish"], false,
                "retiring model methods cannot waive a failed author output: {read}"
            );
            let endpoint = format!("{base}/tasks/{}/complete", parent["id"].as_str().unwrap());
            let no_output = json!({"threadId":"parent-thread","summary":"没有保存简报","deliveryReview":{"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"只撤销工作方法，没有任何产物","checkedScope":"作者原话和无保存结果","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":[]}});
            assert!(
                call(&app, "POST", &endpoint, no_output, CANDIDATE)
                    .await
                    .0
                    .is_client_error()
            );
            let retry = &read["readyActions"][0];
            assert_eq!(retry["recovery"]["failedTaskId"], retry_id);
            assert_eq!(retry["recovery"]["sameConditionFailures"], 2);
            assert_eq!(retry["recovery"]["planRequired"], true);
            let key = retry["actionKey"].as_str().unwrap();
            assert!(
                !select(&app, &base, &parent, &read, key)
                    .await
                    .0
                    .is_success()
            );
            let plan = json!({"failedTaskId":retry_id,"cause":"工具参数与保存契约不符","change":"先读取保存结构，再只提交简报所需字段","verification":"核对回执与简报内容"});
            let request = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":key,"instruction":"修正参数后保存创作简报","idempotencyKey":"changed-strategy","recoveryPlan":plan});
            let endpoint = format!(
                "{base}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            );
            let (code, selected) = call(&app, "POST", &endpoint, request.clone(), CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{selected}");
            assert_eq!(selected["step"]["target"]["recoveryPlan"], plan);
            assert_eq!(
                call(&app, "POST", &endpoint, request.clone(), CANDIDATE).await,
                (StatusCode::OK, selected.clone())
            );
            let mut changed_replay = request;
            changed_replay["recoveryPlan"]["change"] = json!("另一种策略");
            assert!(
                !call(&app, "POST", &endpoint, changed_replay, CANDIDATE)
                    .await
                    .0
                    .is_success()
            );
            complete(&app, &base, parent["id"].as_str().unwrap()).await;
            settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
            let worker = claim(&app, &base, &selected["step"], "parent-thread").await;
            let recovery_id = worker["id"].as_str().unwrap();
            SqliteStoryRepository::open(&path)
                .unwrap()
                .update_task(
                    &ProjectId::parse(&project).unwrap(),
                    recovery_id,
                    "parent-thread",
                    TaskUpdate {
                        status: "failed".into(),
                        error: Some("TaskOutputMissing".into()),
                        result_summary: None,
                        now: 3,
                    },
                )
                .unwrap();
            settle(&path, &project, recovery_id, "failed");
            let next = decision(&app, &base, id).await;
            let parent = claim(&app, &base, &next["step"], "parent-thread").await;
            let read = status(&app, &base, &parent).await;
            let mut repeated = plan;
            repeated["failedTaskId"] = json!(recovery_id);
            let request = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":read["readyActions"][0]["actionKey"],"instruction":"再次尝试同一方法","idempotencyKey":"repeat-strategy","recoveryPlan":repeated});
            let endpoint = format!(
                "{base}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            );
            let (code, rejection) = call(&app, "POST", &endpoint, request, CANDIDATE).await;
            assert!(!code.is_success(), "{rejection}");
            assert!(
                rejection.to_string().contains("already failed"),
                "{rejection}"
            );
            let new_plan = json!({"failedTaskId":recovery_id,"cause":"未能完成保存","change":"先重新读取简报目标，再提交最小完整简报并立刻完成","verification":"核对完整简报回执"});
            let request = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":read["readyActions"][0]["actionKey"],"instruction":"换一种保存步骤","idempotencyKey":"different-strategy","recoveryPlan":new_plan});
            let (code, chosen) = call(&app, "POST", &endpoint, request, CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{chosen}");
            complete(&app, &base, parent["id"].as_str().unwrap()).await;
            settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
            let worker = claim(&app, &base, &chosen["step"], "parent-thread").await;
            let last_id = worker["id"].as_str().unwrap();
            SqliteStoryRepository::open(&path)
                .unwrap()
                .update_task(
                    &ProjectId::parse(&project).unwrap(),
                    last_id,
                    "parent-thread",
                    TaskUpdate {
                        status: "failed".into(),
                        error: Some("TaskOutputMissing".into()),
                        result_summary: None,
                        now: 4,
                    },
                )
                .unwrap();
            settle(&path, &project, last_id, "failed");
            let next = decision(&app, &base, id).await;
            let parent = claim(&app, &base, &next["step"], "parent-thread").await;
            let read = status(&app, &base, &parent).await;
            assert_eq!(
                read["readyActions"][0]["recovery"]["sameConditionFailures"],
                4
            );
            assert_eq!(read["readyActions"][0]["recovery"]["exhausted"], true);
            assert_eq!(read["canReportPartial"], true);
            let request = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":read["readyActions"][0]["actionKey"],"instruction":"再次换一种表述","idempotencyKey":"over-limit","recoveryPlan":{"failedTaskId":last_id,"cause":"未保存","change":"第三种不同的说法","verification":"核对回执"}});
            let endpoint = format!(
                "{base}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            );
            let (code, rejection) = call(&app, "POST", &endpoint, request, CANDIDATE).await;
            assert!(!code.is_success(), "{rejection}");
            assert!(
                rejection.to_string().contains("allowance exhausted"),
                "{rejection}"
            );
            let proposed = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"proposal":{"operation":"generateBrief","target":{"sourceTaskId":last_id}},"instruction":"改名为新任务也不能重置失败额度","idempotencyKey":"proposal-over-limit"});
            let (code, rejection) = call(&app, "POST", &endpoint, proposed, CANDIDATE).await;
            assert!(!code.is_success(), "{rejection}");
            assert!(
                rejection.to_string().contains("allowance exhausted"),
                "{rejection}"
            );
            let mut review = json!({"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"简报未能保存","checkedScope":"保存回执核对","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":[]});
            let endpoint = format!("{base}/tasks/{}/complete", parent["id"].as_str().unwrap());
            let body = json!({"threadId":"parent-thread","summary":"停止重复尝试，保留未完成事项","deliveryReview":review});
            assert!(
                !call(&app, "POST", &endpoint, body, CANDIDATE)
                    .await
                    .0
                    .is_success()
            );
            review["goalStatus"] = json!("partial");
            review["unresolved"] = json!([{"issue":{"issueId":"fixture-unfinished-0","newIssue":{"summary":"简报保存仍未完成，已停止重复尝试"},"resolutionOf":[]},"observationId":null,"reason":"简报保存仍未完成，已停止重复尝试"}]);
            assert_eq!(read["workPlan"]["pendingGoals"], 0);
            let (code,result)=call(&app,"POST",&endpoint,json!({"threadId":"parent-thread","summary":"已停止重复尝试","deliveryReview":review}),CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{result}");
            settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
            assert_eq!(decision(&app, &base, id).await["stopReason"], "completed");
            let db = rusqlite::Connection::open(&path).unwrap();
            let grants: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(grants, 1);
            continue;
        }
        save(&app, &base, &worker, "corrected").await;
        complete(&app, &base, worker["id"].as_str().unwrap()).await;
        settle(&path, &project, worker["id"].as_str().unwrap(), "completed");
        let next = decision(&app, &base, id).await;
        let parent = claim(&app, &base, &next["step"], "parent-thread").await;
        let read = status(&app, &base, &parent).await;
        assert_eq!(read["canFinish"], true);
        assert_eq!(read["deliveryReviewRequired"], true);
        let output = &read["receipts"]["items"][0]["outputs"][0];
        acknowledge_fixture_goal(
            &app,
            &base,
            &parent,
            json!([{"taskId":worker["id"],"revisionId":output["revisionId"]}]),
            None,
        )
        .await;
        assert_eq!(status(&app, &base, &parent).await["canFinish"], true);
    }
}

#[tokio::test]
async fn approved_planning_targets_resume_from_current_sources_in_same_work_without_prior_worker() {
    for finite_scope in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("planning-relay.sqlite3");
        let intent = "在原任务中生成简报、大纲和近章规划，保留现有正文，不采用、不发布；模型按当前内容决定下一步";
        let (app, base, project, root) = setup_with_intent(&path, intent).await;
        let root_id = root["id"].as_str().unwrap();
        let initial_digest = workspace(&app, &base).await["sourceDigest"].clone();
        let mut approved = Vec::new();
        for operation in ["generateBrief", "generateOutline", "generatePlan"] {
            approved.push(action(&app, &base, root_id, operation, operation).await);
        }
        if finite_scope {
            let request = json!({"actionIds":approved.iter().map(|a|a["id"].clone()).collect::<Vec<_>>(),"maxActions":3,"maxTaskAttempts":16,"expectedIntentRevision":0});
            let endpoint = format!("{base}/tasks/{root_id}/execution-scope");
            let (code, preview) = call(
                &app,
                "POST",
                &format!("{endpoint}/preview"),
                request.clone(),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{preview}");
            let (code, permit) = call(
                &app,
                "POST",
                &endpoint,
                json!({"request":request,"previewDigest":preview["previewDigest"]}),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{permit}");
        } else {
            for item in &approved {
                let (code, permit) = call(
                    &app,
                    "POST",
                    &format!(
                        "{base}/tasks/{root_id}/actions/{}/approve",
                        item["id"].as_str().unwrap()
                    ),
                    json!({"expectedIntentRevision":0}),
                    AUTHOR,
                )
                .await;
                assert_eq!(code, StatusCode::OK, "{permit}");
            }
        }
        complete(&app, &base, root_id).await;
        settle(&path, &project, root_id, "completed");
        let first = child(&app, &base, root_id, &approved[0]).await;
        save(&app, &base, &first, "brief").await;
        complete(&app, &base, first["id"].as_str().unwrap()).await;
        settle(&path, &project, first["id"].as_str().unwrap(), "completed");

        for (index, operation) in [(1, "generateOutline"), (2, "generatePlan")] {
            let db = rusqlite::Connection::open(&path).unwrap();
            let prior: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM story_tasks_v2 WHERE project_id=?1 AND operation=?2",
                    rusqlite::params![project, operation],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                prior, 0,
                "a previously created same-operation Task must not be required"
            );
            let initial:(String,i64,Option<String>)=db.query_row("SELECT status,input_revision,current_attempt_id FROM story_work_steps_v1 WHERE id=?1",[approved[index]["id"].as_str().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(initial, ("superseded".into(), 0, None));
            drop(db);
            let continuation = decision(&app, &base, root_id).await;
            assert!(continuation["step"].is_object(), "{continuation}");
            let parent = claim(&app, &base, &continuation["step"], "parent-thread").await;
            assert_eq!(parent["inputManifest"]["authorIntent"], intent);
            let read = status(&app, &base, &parent).await;
            assert_eq!(
                read["canFinish"], false,
                "superseding an unstarted Step cannot discharge its output: {read}"
            );
            let ready = read["readyActions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["operation"] == operation)
                .unwrap();
            assert_eq!(ready["sourceTaskId"], root_id);
            assert_eq!(ready["authorization"], "existingAuthorPlanningTarget");
            let endpoint = format!(
                "{base}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            );
            let request = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"proposal":{"operation":operation,"target":{"sourceTaskId":root_id}},"instruction":"阅读当前已保存资料，生成原已批准的规划目标，保持正文和预算","idempotencyKey":format!("select-{operation}")});
            for change in [
                json!({"operation":"generatePropertyRules","target":{"sourceTaskId":root_id}}),
                json!({"operation":operation,"target":{"sourceTaskId":"foreign-task"}}),
                json!({"operation":"generateChapter","target":{"chapterId":"unauthorized-chapter"}}),
            ] {
                let mut invalid = request.clone();
                invalid["proposal"] = change;
                assert!(
                    call(&app, "POST", &endpoint, invalid, CANDIDATE)
                        .await
                        .0
                        .is_client_error()
                );
            }
            let (code, chosen) = call(&app, "POST", &endpoint, request.clone(), CANDIDATE).await;
            assert_eq!(code, StatusCode::OK, "{chosen}");
            assert_eq!(
                call(&app, "POST", &endpoint, request, CANDIDATE).await,
                (StatusCode::OK, chosen.clone())
            );
            assert_eq!(chosen["step"]["id"], approved[index]["id"]);
            assert_eq!(
                chosen["step"]["instruction"],
                "阅读当前已保存资料，生成原已批准的规划目标，保持正文和预算"
            );
            assert_eq!(
                chosen["step"]["inputRevision"],
                workspace(&app, &base).await["revision"]
            );
            assert_ne!(read["sourceDigest"], initial_digest);
            complete(&app, &base, parent["id"].as_str().unwrap()).await;
            settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
            let planner = claim(&app, &base, &chosen["step"], "parent-thread").await;
            assert_eq!(planner["inputManifest"]["authorIntent"], intent);
            assert_eq!(
                planner["inputManifest"]["authorIntentBinding"]["sourceTaskId"],
                root_id
            );
            let (kind, payload) = if operation == "generateOutline" {
                (
                    "outline",
                    json!({"nodes":[{"id":"book","parentId":null,"kind":"book","position":0,"title":"港城来信","purpose":"展开简报的来信承诺","summary":"保存信件与署名","dependencyKeys":[],"certainty":"tentative","protection":"free","writerGuidance":"保留现有正文"}]}),
                )
            } else {
                (
                    "plan",
                    json!({"chapters":[{"chapterPlanId":"plan-1","chapterNumber":1,"goal":"找到署名","scenes":[{"id":"scene-1","goal":"保存信件","beats":[],"expectedEffects":[{"id":"effect-1","dimension":"knowledge","description":"发现署名","commitmentRefs":[]}]}]}]}),
                )
            };
            let (code,saved)=call(&app,"POST",&format!("{base}/artifacts/{kind}"),json!({"taskId":planner["id"],"threadId":"parent-thread","expectedRevision":planner["inputRevision"],"expectedCandidateRevisionId":planner["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":format!("save-{kind}"),"payload":payload}),CANDIDATE).await;
            assert_eq!(code, StatusCode::CREATED, "{saved}");
            complete(&app, &base, planner["id"].as_str().unwrap()).await;
            settle(
                &path,
                &project,
                planner["id"].as_str().unwrap(),
                "completed",
            );
            let db = rusqlite::Connection::open(&path).unwrap();
            let actual:(String,String,i64)=db.query_row("SELECT status,result_task_id,input_revision FROM story_task_action_requests_v1 WHERE action_id=?1",[approved[index]["id"].as_str().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(
                actual,
                (
                    "completed".into(),
                    planner["id"].as_str().unwrap().into(),
                    0
                ),
                "original permission retains its frozen revision; actual successor output completes its duty"
            );
        }
        let next = decision(&app, &base, root_id).await;
        let final_task = claim(&app, &base, &next["step"], "parent-thread").await;
        let read = status(&app, &base, &final_task).await;
        assert_eq!(read["coverage"]["pendingRequirements"], 0, "{read}");
        assert_eq!(read["canFinish"], true);
        complete(&app, &base, final_task["id"].as_str().unwrap()).await;
        settle(
            &path,
            &project,
            final_task["id"].as_str().unwrap(),
            "completed",
        );
        assert_eq!(
            decision(&app, &base, root_id).await["stopReason"],
            "completed"
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        let counts:(i64,i64,i64,i64)=db.query_row("SELECT (SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1),(SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id WHERE r.object_kind='draft')",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(counts, (1, 1, 1, 0));
        let permissions: i64 = db
            .query_row("SELECT COUNT(*) FROM story_action_permits_v1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(permissions, 3, "successor cannot grant a new author action");
    }
}

#[tokio::test]
async fn planning_reconsideration_preserves_attempts_unknown_writes_and_original_author_targets() {
    for attempted in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("planning-fences.sqlite3");
        let (app, base, project, root) =
            setup_with_intent(&path, "允许生成简报和大纲，保留正文；不允许其他规划类型").await;
        let id = root["id"].as_str().unwrap();
        let operation = if attempted {
            "generateOutline"
        } else {
            "generateBrief"
        };
        let first = action(&app, &base, id, operation, "first").await;
        let second = action(
            &app,
            &base,
            id,
            if attempted {
                "generateBrief"
            } else {
                "generateOutline"
            },
            "second",
        )
        .await;
        for approved in [&first, &second] {
            let (code, permit) = call(
                &app,
                "POST",
                &format!(
                    "{base}/tasks/{id}/actions/{}/approve",
                    approved["id"].as_str().unwrap()
                ),
                json!({"expectedIntentRevision":0}),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{permit}");
        }
        complete(&app, &base, id).await;
        settle(&path, &project, id, "completed");
        let writer = approved_child(&app, &base, id, &first).await;
        let writer_id = writer["id"].as_str().unwrap();
        if attempted {
            let (code,saved)=call(&app,"POST",&format!("{base}/artifacts/outline"),json!({"taskId":writer_id,"threadId":"parent-thread","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline-partial","payload":{"nodes":[{"id":"book","parentId":null,"kind":"book","position":0,"title":"港城","purpose":"寻找署名","summary":"候选大纲","dependencyKeys":[],"certainty":"tentative","protection":"free","writerGuidance":"保留正文"}]}}),CANDIDATE).await;
            assert_eq!(code, StatusCode::CREATED, "{saved}");
            SqliteStoryRepository::open(&path)
                .unwrap()
                .update_task(
                    &ProjectId::parse(&project).unwrap(),
                    writer_id,
                    "parent-thread",
                    TaskUpdate {
                        status: "failed".into(),
                        error: Some("invalid_tool_arguments".into()),
                        result_summary: None,
                        now: 1,
                    },
                )
                .unwrap();
            settle(&path, &project, writer_id, "failed");
            let db = rusqlite::Connection::open(&path).unwrap();
            let retained:(String,String,i64)=db.query_row("SELECT status,current_attempt_id,(SELECT COUNT(*) FROM story_work_attempts_v1 WHERE step_id=s.id) FROM story_work_steps_v1 s WHERE id=?1",[first["id"].as_str().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(retained, ("blocked".into(), writer_id.into(), 1));
            let action_status: String = db
                .query_row(
                    "SELECT status FROM story_task_action_requests_v1 WHERE action_id=?1",
                    [first["id"].as_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(action_status, "failed");
            drop(db);
            assert_eq!(
                decision(&app, &base, id).await["stopReason"],
                "executionBlocked",
                "a real partial/unknown attempt is never replaced as an unstarted planning choice"
            );
        } else {
            save(&app, &base, &writer, "brief").await;
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute("INSERT INTO story_tool_operations_v1 VALUES ('unknown-planning-write',?1,?2,'parent-thread','turn','call','save_outline','hash',NULL,'accepted',NULL,1,1)",rusqlite::params![project,writer_id]).unwrap();
            complete(&app, &base, writer_id).await;
            settle(&path, &project, writer_id, "completed");
            let untouched: (String, Option<String>) = db
                .query_row(
                    "SELECT status,current_attempt_id FROM story_work_steps_v1 WHERE id=?1",
                    [second["id"].as_str().unwrap()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(untouched, ("queued".into(), None));
            assert_eq!(
                decision(&app, &base, id).await["stopReason"],
                "workNotReady"
            );
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM story_work_decision_actions_v1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            // A trusted recovery fixture settles the exact operation before the
            // author route can reconsider the unstarted target. No model grants it.
            db.execute("UPDATE story_tool_operations_v1 SET state='committed',receipt_ref='runtime:fixture-known-save' WHERE operation_id='unknown-planning-write'",[]).unwrap();
            drop(db);
            let next = decision(&app, &base, id).await;
            assert!(next["step"].is_object(), "{next}");
            let parent = claim(&app, &base, &next["step"], "parent-thread").await;
            let read = status(&app, &base, &parent).await;
            assert_eq!(read["canFinish"], false);
            assert_eq!(read["readyActions"][0]["operation"], "generateOutline");
            let endpoint = format!(
                "{base}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            );
            let denied = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"proposal":{"operation":"generatePlan","target":{"sourceTaskId":id}},"instruction":"新增未允许的规划","idempotencyKey":"outside-planning"});
            assert!(
                call(&app, "POST", &endpoint, denied, CANDIDATE)
                    .await
                    .0
                    .is_client_error()
            );
            let db = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM story_work_decision_actions_v1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
    }
}

#[tokio::test]
async fn unresolved_failure_and_queued_sibling_never_allocate_decision() {
    for queued in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("blocked.sqlite3");
        let (app, base, project, root) = setup(&path).await;
        let id = root["id"].as_str().unwrap();
        let first = action(&app, &base, id, "generateBrief", "first").await;
        let second = if queued {
            Some(action(&app, &base, id, "generateOutline", "sibling").await)
        } else {
            None
        };
        complete(&app, &base, id).await;
        settle(&path, &project, id, "completed");
        let worker = child(&app, &base, id, &first).await;
        let worker_id = worker["id"].as_str().unwrap();
        if let Some(second) = second {
            let (code, result) = call(
                &app,
                "POST",
                &format!(
                    "{base}/tasks/{id}/actions/{}/approve",
                    second["id"].as_str().unwrap()
                ),
                json!({"expectedIntentRevision":0}),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{result}");
        }
        SqliteStoryRepository::open(&path)
            .unwrap()
            .update_task(
                &ProjectId::parse(&project).unwrap(),
                worker_id,
                "parent-thread",
                TaskUpdate {
                    status: "failed".into(),
                    error: Some("unknown outcome".into()),
                    result_summary: None,
                    now: 1,
                },
            )
            .unwrap();
        settle(&path, &project, worker_id, "failed");
        let next = decision(&app, &base, id).await;
        assert!(next["step"].is_null());
        assert_eq!(
            next["stopReason"],
            if queued {
                "workNotReady"
            } else {
                "executionBlocked"
            }
        );
    }
}

#[tokio::test]
async fn output_limit_recovery_keeps_zero_write_settlement_and_total_budget_fences() {
    for fence in [
        "unsettled",
        "unknownWrite",
        "savedOutput",
        "unknownUsage",
        "totalBudget",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("output-limit-fences.sqlite3");
        let (app, base, project, root) = setup(&path).await;
        let id = root["id"].as_str().unwrap();
        let first = action(&app, &base, id, "generateBrief", "before-limit").await;
        complete(&app, &base, id).await;
        settle(&path, &project, id, "completed");
        let worker = child(&app, &base, id, &first).await;
        let worker_id = worker["id"].as_str().unwrap();
        if fence == "savedOutput" {
            save(&app, &base, &worker, "saved-before-limit").await;
        }
        SqliteStoryRepository::open(&path)
            .unwrap()
            .update_task(
                &ProjectId::parse(&project).unwrap(),
                worker_id,
                "parent-thread",
                TaskUpdate {
                    status: "failed".into(),
                    error: Some("provider_output_limit".into()),
                    result_summary: None,
                    now: 1,
                },
            )
            .unwrap();
        if fence != "unsettled" {
            settle(&path, &project, worker_id, "failed");
        }
        let db = rusqlite::Connection::open(&path).unwrap();
        if fence == "unknownWrite" {
            db.execute("INSERT INTO story_tool_operations_v1 VALUES ('limit-unknown-write',?1,?2,'parent-thread','turn','save-call','save_brief','hash',NULL,'accepted',NULL,1,1)", rusqlite::params![project,worker_id]).unwrap();
        }
        if matches!(fence, "unknownUsage" | "totalBudget") {
            let work = format!("work-{id}");
            let encoded: String = db
                .query_row(
                    "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
                    [&work],
                    |r| r.get(0),
                )
                .unwrap();
            let mut ledger: Value = serde_json::from_str(&encoded).unwrap();
            let limit = ledger["maxRequests"].as_u64().unwrap();
            for charge in ledger["charges"].as_array_mut().unwrap() {
                if charge["taskId"] == worker_id {
                    if fence == "unknownUsage" {
                        charge["unknownTokens"] = json!(1);
                    } else {
                        charge["requests"] = json!(limit);
                    }
                }
            }
            if fence == "unknownUsage" {
                ledger["unknownTokens"] = json!(1);
            } else {
                ledger["usedRequests"] = json!(limit);
            }
            serde_json::from_value::<novel_codex_story_core::ProviderBudget>(ledger.clone())
                .unwrap();
            db.execute(
                "UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",
                rusqlite::params![ledger.to_string(), work],
            )
            .unwrap();
        }
        let before: i64 = db
            .query_row("SELECT COUNT(*) FROM story_work_steps_v1", [], |r| r.get(0))
            .unwrap();
        drop(db);
        let work_uri = format!("{base}/work-tasks/work-{id}");
        let (_, work) = call(&app, "GET", &work_uri, Value::Null, AUTHOR).await;
        let (code, resumed) = call(
            &app,
            "POST",
            &format!("{work_uri}/control"),
            json!({"expectedVersion":work["version"],"command":"resume"}),
            AUTHOR,
        )
        .await;
        if fence == "totalBudget" {
            assert_eq!(code, StatusCode::OK, "{resumed}");
            assert_eq!(resumed["steps"][1]["status"], "blocked");
            assert_eq!(resumed["steps"][1]["currentAttemptId"], worker_id);
        } else {
            assert!(code.is_client_error(), "{fence}: {resumed}");
            assert_eq!(
                call(&app, "GET", &work_uri, Value::Null, AUTHOR).await.1,
                work
            );
        }
        let correction = decision(&app, &base, id).await;
        assert!(correction["step"].is_null(), "{fence}: {correction}");
        assert_eq!(
            correction["stopReason"],
            match fence {
                "unsettled" | "unknownWrite" => "pendingRecovery",
                "unknownUsage" => "unknownUsage",
                "totalBudget" => "budgetExhausted",
                _ => "executionBlocked",
            }
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM story_work_steps_v1", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            before
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}

#[tokio::test]
async fn advisory_resume_keeps_its_existing_requeue_contract() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("advisory-resume.sqlite3");
    let (app, base, project, root) = setup(&path).await;
    let id = root["id"].as_str().unwrap();
    SqliteStoryRepository::open(&path)
        .unwrap()
        .update_task(
            &ProjectId::parse(&project).unwrap(),
            id,
            "parent-thread",
            TaskUpdate {
                status: "failed".into(),
                error: Some("provider_output_limit".into()),
                result_summary: None,
                now: 1,
            },
        )
        .unwrap();
    settle(&path, &project, id, "failed");
    let uri = format!("{base}/work-tasks/work-{id}");
    let (_, work) = call(&app, "GET", &uri, Value::Null, AUTHOR).await;
    let (code, resumed) = call(
        &app,
        "POST",
        &format!("{uri}/control"),
        json!({"expectedVersion":work["version"],"command":"resume"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{resumed}");
    assert_eq!(resumed["steps"][0]["operation"], "advisory");
    assert_eq!(resumed["steps"][0]["status"], "queued");
    assert!(resumed["steps"][0]["currentAttemptId"].is_null());
    assert_eq!(resumed["steps"][0]["attempts"][0]["id"], id);
}
async fn next(app: &Router, uri: &str) -> Value {
    let (code, result) = call(app, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}
async fn run_claim(app: &Router, base: &str, action: &Value) -> Value {
    let step = json!({"operation":action["operation"],"target":action["target"],"id":action["target"]["resumeStepId"],"inputRevision":workspace(app,base).await["revision"]});
    let task = claim(app, base, &step, "parent-thread").await;
    if task["operation"] == "advisory" {
        ensure_execution_plan(app, base, &task).await;
    }
    task
}
async fn advance(
    app: &Router,
    path: &std::path::Path,
    _project: &str,
    uri: &str,
    worker: &Value,
) -> Value {
    let id = worker["id"].as_str().unwrap();
    complete(app, uri.rsplit_once("/generation-runs/").unwrap().0, id).await;
    let (grant, _) = terminal_receipt(path, id, "completed");
    assert_eq!(
        grant.work_task_id,
        format!("work-{}", uri.rsplit_once('/').unwrap().1)
    );
    let (code, charged) = call(
        app,
        "POST",
        &format!("{uri}/provider-usage"),
        json!({"taskId":id}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{charged}");
    let (_, run) = call(app, "GET", uri, Value::Null, AUTHOR).await;
    let (code, result) = call(
        app,
        "POST",
        &format!("{uri}/advance-task"),
        json!({"expectedVersion":run["version"],"taskId":id}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}
async fn choose_generation(app: &Router, base: &str, parent: &Value, operation: &str) -> Value {
    let read = status(app, base, parent).await;
    let action = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == operation)
        .unwrap_or_else(|| panic!("missing {operation}: {read}"));
    let (code, result) = select(
        app,
        base,
        parent,
        &read,
        action["actionKey"].as_str().unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}
#[tokio::test]
async fn generation_agent_selects_plan_writer_knowledge_and_finish_without_optional_fake_reports() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("generation.sqlite3");
    let (app, base, project, root) = setup(&path).await;
    let id = root["id"].as_str().unwrap();
    complete(&app, &base, id).await;
    settle(&path, &project, id, "completed");
    let initial = workspace(&app, &base).await;
    let (code,run)=call(&app,"POST",&format!("{base}/generation-runs"),json!({"expectedProjectRevision":initial["revision"],"expectedBaseSnapshotId":initial["headSnapshotId"],"expectedSourceDigest":initial["sourceDigest"],"authorIntent":"写一章港城悬疑候选，不必提取伏笔或深审","targetChapters":1,"maxTaskAttempts":null,"maxProviderRequests":null,"maxProviderTokens":null,"deadlineAt":null}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{run}");
    let uri = format!("{base}/generation-runs/{}", run["id"].as_str().unwrap());
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    choose_generation(&app, &base, &parent, "generatePlan").await;
    advance(&app, &path, &project, &uri, &parent).await;
    let planner = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let payload = json!({"chapters":[{"chapterPlanId":"plan-1","chapterNumber":1,"goal":"找到署名","scenes":[{"id":"scene-1","goal":"保存信件","beats":[],"expectedEffects":[{"id":"effect-1","dimension":"knowledge","description":"发现署名","commitmentRefs":[]}]}]}]});
    let (code,plan)=call(&app,"POST",&format!("{base}/artifacts/plan"),json!({"taskId":planner["id"],"threadId":"parent-thread","expectedRevision":planner["inputRevision"],"expectedCandidateRevisionId":planner["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":"plan","payload":payload}),CANDIDATE).await;
    assert_eq!(code, StatusCode::CREATED, "{plan}");
    advance(&app, &path, &project, &uri, &planner).await;
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    choose_generation(&app, &base, &parent, "writeNextChapter").await;
    advance(&app, &path, &project, &uri, &parent).await;
    let before_materialization = workspace(&app, &base).await;
    let (_, run_before) = call(&app, "GET", &uri, Value::Null, AUTHOR).await;
    let fault = rusqlite::Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER reject_run_advance BEFORE UPDATE ON story_generation_runs_v1 BEGIN SELECT RAISE(ABORT,'late run write'); END;").unwrap();
    let (code, _) = call(&app, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(workspace(&app, &base).await, before_materialization);
    assert_eq!(
        call(&app, "GET", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, run_before.clone())
    );
    fault
        .execute_batch("DROP TRIGGER reject_run_advance;")
        .unwrap();
    let stored: String = fault
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [run["id"].as_str().unwrap()],
            |row| row.get(/*idx*/ 0),
        )
        .unwrap();
    fault.execute("UPDATE story_generation_runs_v1 SET state_json=json_set(state_json,'$.sourceDigest',?2) WHERE run_id=?1",rusqlite::params![run["id"].as_str().unwrap(),"0".repeat(64)]).unwrap();
    let (code, _) = call(&app, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert!(!code.is_success());
    assert_eq!(workspace(&app, &base).await, before_materialization);
    fault
        .execute(
            "UPDATE story_generation_runs_v1 SET state_json=?2 WHERE run_id=?1",
            rusqlite::params![run["id"].as_str().unwrap(), stored],
        )
        .unwrap();
    let dispatch = next(&app, &uri).await;
    assert_eq!(dispatch["action"]["operation"], "generateChapter");
    assert_eq!(next(&app, &uri).await, dispatch);
    assert_eq!(
        workspace(&app, &base).await["chapters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut forged = dispatch["action"].clone();
    forged["target"]["chapterId"] = json!("foreign-chapter");
    let invalid_step = json!({"operation":forged["operation"],"target":forged["target"],"id":forged["target"]["resumeStepId"],"inputRevision":workspace(&app,&base).await["revision"]});
    let mut request = task("generateChapter", invalid_step["target"].clone());
    request["inputRevision"] = invalid_step["inputRevision"].clone();
    let (code, _) = call(&app, "POST", &format!("{base}/tasks"), request, AUTHOR).await;
    assert!(!code.is_success());
    let writer = run_claim(&app, &base, &dispatch["action"]).await;
    let chapter = writer["target"]["chapterId"].as_str().unwrap();
    let (code,saved)=call(&app,"POST",&format!("{base}/drafts/checkpoints"),json!({"taskId":writer["id"],"threadId":"parent-thread","expectedRevision":writer["inputRevision"],"expectedCandidateRevisionId":writer["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":"draft","completeness":"chapterComplete","payload":{"chapterId":chapter,"chapterNumber":1,"title":"来信","body":"林夕拾起信件。\n\n---\n\n雨仍未停。","sceneGroups":[{"id":"group-1","sceneId":"scene-1","body":"林夕拾起信件。","locked":false},{"id":"group-2","sceneId":"scene-1","body":"雨仍未停。","locked":false}],"sourcePlanRefs":{"chapterPlanId":"plan-1","planRevisionId":plan["revisionId"],"outlineRevisionId":null,"localOnly":false,"lineageStatus":"current"}}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::CREATED, "{saved}");
    advance(&app, &path, &project, &uri, &writer).await;
    // The agent can repair one scene inside the original generation authorization.
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let read = status(&app, &base, &parent).await;
    let draft_receipt = &read["receipts"]["items"][0]["outputs"][0];
    assert_eq!(draft_receipt["chapterId"], chapter);
    assert_eq!(draft_receipt["chapterNumber"], 1);
    assert_eq!(draft_receipt["title"], "来信");
    let endpoint = format!(
        "{base}/tasks/{}/work-decision/actions",
        parent["id"].as_str().unwrap()
    );
    let proposal = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
        "proposal":{"operation":"generateChapter","target":{"chapterId":chapter,"scope":{"type":"selectedScenes","groupIds":["group-1"]}}},
        "chapterAcceptance":read["chapterAcceptance"],
        "instruction":"只补充信件署名，保留其他场景与标题","idempotencyKey":"repair-selected-scene"});
    let mut outside = proposal.clone();
    outside["proposal"]["target"]["chapterId"] = json!("another-chapter");
    assert!(
        !call(&app, "POST", &endpoint, outside, CANDIDATE)
            .await
            .0
            .is_success()
    );
    let (code, selected) = call(&app, "POST", &endpoint, proposal, CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{selected}");
    assert_eq!(
        selected["step"]["target"]["scope"]["sourceRevisionId"],
        saved["revisionId"]
    );
    advance(&app, &path, &project, &uri, &parent).await;
    let repair = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    assert_eq!(repair["target"]["scope"]["type"], "selectedScenes");
    assert_eq!(
        repair["inputManifest"]["generationAuthorIntentBinding"]["runId"],
        run["id"]
    );
    let repaired = json!({"chapterId":chapter,"chapterNumber":1,"title":"来信",
        "body":"林夕拾起信件，看清署名。\n\n---\n\n雨仍未停。",
        "sceneGroups":[{"id":"group-1","sceneId":"scene-1","body":"林夕拾起信件，看清署名。","locked":false},
            {"id":"group-2","sceneId":"scene-1","body":"雨仍未停。","locked":false}],
        "sourcePlanRefs":{"chapterPlanId":"plan-1","planRevisionId":plan["revisionId"],"outlineRevisionId":null,"localOnly":false,"lineageStatus":"current"}});
    let checkpoint = json!({"taskId":repair["id"],"threadId":"parent-thread","expectedRevision":repair["inputRevision"],
        "expectedCandidateRevisionId":repair["inputManifest"]["expectedCandidateRevisionId"],
        "idempotencyKey":"selected-draft","completeness":"chapterComplete","payload":repaired});
    for field in ["title", "unselected", "lock"] {
        let mut invalid = checkpoint.clone();
        match field {
            "title" => invalid["payload"]["title"] = json!("越权改题"),
            "unselected" => invalid["payload"]["sceneGroups"][1]["body"] = json!("越权改文"),
            _ => invalid["payload"]["sceneGroups"][0]["locked"] = json!(true),
        }
        assert!(
            !call(
                &app,
                "POST",
                &format!("{base}/drafts/checkpoints"),
                invalid,
                CANDIDATE
            )
            .await
            .0
            .is_success()
        );
    }
    let (code, repaired_saved) = call(
        &app,
        "POST",
        &format!("{base}/drafts/checkpoints"),
        checkpoint,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{repaired_saved}");
    advance(&app, &path, &project, &uri, &repair).await;
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let read = status(&app, &base, &parent).await;
    assert_eq!(read["canFinish"], false);
    assert!(
        !read["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "knowledge")
    );
    assert!(
        read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "modelKnowledge" && a["required"] == false)
    );
    assert!(
        !read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "finishChapter")
    );
    assert_eq!(
        status(&app, &base, &parent).await["finalDeliveryReviewRequired"],
        true
    );
    // An optional analysis failure stays visible without becoming a mandatory pipeline.
    choose_generation(&app, &base, &parent, "modelKnowledge").await;
    advance(&app, &path, &project, &uri, &parent).await;
    let analyzer = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let analyzer_id = analyzer["id"].as_str().unwrap();
    SqliteStoryRepository::open(&path)
        .unwrap()
        .update_task(
            &ProjectId::parse(&project).unwrap(),
            analyzer_id,
            "parent-thread",
            TaskUpdate {
                status: "failed".into(),
                error: Some("invalid_tool_arguments".into()),
                result_summary: None,
                now: 5,
            },
        )
        .unwrap();
    let (grant, _) = terminal_receipt(&path, analyzer_id, "failed");
    let (code, result) = call(
        &app,
        "POST",
        &format!("{uri}/provider-usage"),
        json!({"taskId":analyzer_id}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result} grant={grant:?}");
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let read = status(&app, &base, &parent).await;
    assert!(
        read["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "optionalAnalysis" && r["status"] == "failed")
    );
    assert!(
        !read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "finishChapter")
    );
    let retry = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "modelKnowledge")
        .unwrap();
    assert_eq!(retry["required"], false);
    assert_eq!(retry["recovery"]["failedTaskId"], analyzer_id);
    // Optional extraction is dispensable; independent frozen-standard review is not.
    choose_generation(&app, &base, &parent, "assessRevisionOutcome").await;
    advance(&app, &path, &project, &uri, &parent).await;
    let critic = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    assert_eq!(critic["operation"], "assessRevisionOutcome");
    let (code, pack) = call(&app, "GET", &format!("{base}/context?taskId={}&threadId=parent-thread&projectRevision={}&purpose=critic&byteBudget=60000", critic["id"].as_str().unwrap(), critic["inputRevision"]), Value::Null, CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{pack}");
    let author_source = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sectionPath"] == "/authorRequirementsSource")
        .unwrap();
    let mut author_cursor = String::new();
    loop {
        let (code,page)=call(&app,"GET",&format!("{base}/tasks/{}/task-source?threadId=parent-thread&expectedRevision={}&sourceId={}{author_cursor}",critic["id"].as_str().unwrap(),critic["inputRevision"],author_source["sourceId"].as_str().unwrap()),Value::Null,CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        let Some(next) = page["nextCursor"].as_str() else {
            break;
        };
        author_cursor = format!("&cursor={next}");
    }
    let source = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sectionPath"] == "/operationContext/revisionOutcomeInput")
        .unwrap();
    let mut cursor = String::new();
    loop {
        let (code,page) = call(&app,"GET",&format!("{base}/tasks/{}/task-source-content?threadId=parent-thread&expectedRevision={}&sourceId={}{cursor}",critic["id"].as_str().unwrap(),critic["inputRevision"],source["sourceId"].as_str().unwrap()),Value::Null,CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        let Some(next) = page["nextCursor"].as_str() else {
            break;
        };
        cursor = format!("&cursor={next}");
    }
    let repo = SqliteStoryRepository::open(&path).unwrap();
    let task = repo
        .task(
            &ProjectId::parse(&project).unwrap(),
            critic["id"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap();
    let input = repo.revision_outcome_input(&task).unwrap();
    let scene = &input["afterScenes"][0];
    let quote = scene["body"].as_str().unwrap();
    let report = json!({"writerTaskId":critic["target"]["writerTaskId"],"chapterId":critic["target"]["chapterId"],"originalRevisionId":critic["target"]["originalRevisionId"],"draftRevisionId":critic["target"]["draftRevisionId"],"knowledgeBatchId":critic["target"]["knowledgeBatchId"],"uncoveredRequirements":[],"outcomes":input["desiredOutcomes"].as_array().unwrap().iter().map(|criterion|json!({"id":criterion["id"],"status":"supported","issue":null,"reason":"已独立读取实际正文并核对全部冻结标准","evidence":[{"groupId":scene["groupId"],"quote":quote,"start":0,"end":quote.chars().count()}]})).collect::<Vec<_>>()});
    drop(repo);
    let (code, receipt) = call(&app,"POST",&format!("{base}/revision-outcomes/{}",critic["target"]["writerTaskId"].as_str().unwrap()),json!({"taskId":critic["id"],"threadId":"parent-thread","expectedRevision":critic["inputRevision"],"expectedCandidateRevisionId":null,"idempotencyKey":"independent-generation-report","payload":report}),CANDIDATE).await;
    assert_eq!(code, StatusCode::CREATED, "{receipt}");
    advance(&app, &path, &project, &uri, &critic).await;
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let read = status(&app, &base, &parent).await;
    assert!(
        read["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "revisionOutcome" && r["status"] == "satisfied")
    );
    choose_generation(&app, &base, &parent, "finishChapter").await;
    let (rejected, _) = call(
        &app,
        "POST",
        &format!("{base}/tasks/{}/complete", parent["id"].as_str().unwrap()),
        json!({"threadId":"parent-thread","summary":"尚未提供目标核对"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(rejected, StatusCode::BAD_REQUEST);
    let pending_run = call(&app, "GET", &uri, Value::Null, AUTHOR).await.1;
    assert_eq!(pending_run["completedChapters"], 0);
    assert_eq!(pending_run["status"], "running");
    let finished = advance(&app, &path, &project, &uri, &parent).await;
    assert_eq!(
        (
            finished["status"].clone(),
            finished["completedChapters"].clone()
        ),
        (json!("completed"), json!(1))
    );
    let delivery = &finished["chapterDeliveries"][0];
    assert_eq!(
        json!({"batch":delivery["knowledgeBatchId"],"extraction":delivery["commitmentExtractionRevisionId"],"assessment":delivery["assessmentRevisionId"]}),
        json!({"batch":null,"extraction":null,"assessment":null})
    );
    let current = workspace(&app, &base).await;
    assert_eq!(current["chapters"].as_array().unwrap().len(), 1);
    assert_eq!(current["chapters"][0]["accepted"], Value::Null);
    assert_eq!(current["artifacts"]["brief"]["candidate"], Value::Null);
    assert_eq!(current["artifacts"]["outline"]["candidate"], Value::Null);
    let db = rusqlite::Connection::open(&path).unwrap();
    let reviewed: i64 = db.query_row("SELECT COUNT(*) FROM story_work_delivery_reviews_v1 WHERE work_task_id=?1 AND decision_task_id=?2", rusqlite::params![format!("work-{}", run["id"].as_str().unwrap()), parent["id"].as_str().unwrap()], |row|row.get(0)).unwrap();
    assert_eq!(reviewed, 1);
    let scopes: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_execution_scopes_v1 WHERE work_task_id=?1 AND kind='generation' AND generation_run_id=?2 AND json_extract(scope_json,'$.bounds.targetChapters')=1",
            rusqlite::params![format!("work-{}", run["id"].as_str().unwrap()), run["id"].as_str().unwrap()],
            |r| r.get(/*idx*/ 0),
        )
        .unwrap();
    assert_eq!(scopes, 1);
}

#[tokio::test]
async fn agent_proposes_a_new_same_work_revision_without_new_authority_or_ready_action() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-proposed-revision.sqlite3");
    let (app, base, project, root) = setup(&path).await;
    let root_id = root["id"].as_str().unwrap();
    let first = action(&app, &base, root_id, "generateBrief", "initial").await;
    complete(&app, &base, root_id).await;
    settle(&path, &project, root_id, "completed");
    let worker = child(&app, &base, root_id, &first).await;
    save(&app, &base, &worker, "first-draft").await;
    let worker_id = worker["id"].as_str().unwrap();
    complete(&app, &base, worker_id).await;
    settle(&path, &project, worker_id, "completed");
    let next = decision(&app, &base, root_id).await;
    let parent = claim(&app, &base, &next["step"], "parent-thread").await;
    let read = status(&app, &base, &parent).await;
    assert_eq!(read["readyActions"], json!([]));
    let endpoint = format!(
        "{base}/tasks/{}/work-decision/actions",
        parent["id"].as_str().unwrap()
    );
    let proposal = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
        "proposal":{"operation":"generateBrief","target":{"sourceTaskId":worker_id}},
        "instruction":"只进一步修正已经授权的创作简报，保持其他内容","idempotencyKey":"new-revision"});
    for (key, value) in [
        ("operation", json!("generateOutline")),
        ("target", json!({"sourceTaskId":"foreign-task"})),
    ] {
        let mut invalid = proposal.clone();
        invalid["proposal"][key] = value;
        let (code, rejected) = call(&app, "POST", &endpoint, invalid, CANDIDATE).await;
        assert!(code.is_client_error(), "{code}: {rejected}");
    }
    let mut forged_grant = proposal.clone();
    forged_grant["proposal"]["target"]["budgetId"] = json!("new-grant");
    let (code, rejected) = call(&app, "POST", &endpoint, forged_grant, CANDIDATE).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{rejected}");
    let rejection = rejected
        .as_str()
        .expect("Axum reports malformed JSON DTOs as text");
    assert!(
        rejection.contains("unknown field `budgetId`"),
        "{rejection}"
    );
    let mut conflicting = proposal.clone();
    conflicting["actionKey"] = json!("also-selected");
    assert!(
        call(&app, "POST", &endpoint, conflicting, CANDIDATE)
            .await
            .0
            .is_client_error()
    );
    let mut stale = proposal.clone();
    stale["expectedWorkVersion"] = json!(-1);
    assert!(
        call(&app, "POST", &endpoint, stale, CANDIDATE)
            .await
            .0
            .is_client_error()
    );
    let lock_db = rusqlite::Connection::open(&path).unwrap();
    lock_db.execute("UPDATE story_object_heads_v2 SET locked=1 WHERE project_id=?1 AND object_id='artifact-brief'",[&project]).unwrap();
    assert!(
        call(&app, "POST", &endpoint, proposal.clone(), CANDIDATE)
            .await
            .0
            .is_client_error()
    );
    assert_eq!(
        lock_db
            .query_row(
                "SELECT COUNT(*) FROM story_work_decision_actions_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    lock_db.execute("UPDATE story_object_heads_v2 SET locked=0 WHERE project_id=?1 AND object_id='artifact-brief'",[&project]).unwrap();
    let (code, chosen) = call(&app, "POST", &endpoint, proposal.clone(), CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{chosen}");
    assert_eq!(
        call(&app, "POST", &endpoint, proposal.clone(), CANDIDATE).await,
        (StatusCode::OK, chosen.clone())
    );
    let mut changed = proposal;
    changed["proposal"]["target"]["sourceTaskId"] = json!(root_id);
    assert!(
        call(&app, "POST", &endpoint, changed, CANDIDATE)
            .await
            .0
            .is_client_error()
    );
    complete(&app, &base, parent["id"].as_str().unwrap()).await;
    settle(&path, &project, parent["id"].as_str().unwrap(), "completed");
    let revised = claim(&app, &base, &chosen["step"], "parent-thread").await;
    assert_eq!(
        revised["inputManifest"]["authorIntent"],
        root["inputManifest"]["authorIntent"]
    );
    assert_eq!(
        revised["inputManifest"]["workDecisionAction"]["workTaskId"],
        chosen["workTaskId"]
    );
    save(&app, &base, &revised, "agent-refined").await;
    complete(&app, &base, revised["id"].as_str().unwrap()).await;
    settle(
        &path,
        &project,
        revised["id"].as_str().unwrap(),
        "completed",
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_work_tasks_v1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_work_budget_authorizations_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_work_decision_actions_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
