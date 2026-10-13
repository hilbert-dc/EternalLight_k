//! Actual Engine receipts gate all new execution, independently of finite/null limits.
use super::{AUTHOR, CANDIDATE, call, initial};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_core::TaskProviderBudget;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
#[path = "../../../../shared/novel-operation-digest.rs"]
mod operation_digest;

const CAP: &str = "work-budget-usage-capability-1234567890";

pub(super) fn snapshot(path: &std::path::Path) -> Value {
    fn database(path: &std::path::Path) -> Value {
        if !path.exists() {
            return Value::Null;
        }
        let db = rusqlite::Connection::open(path).unwrap();
        let names = db
            .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut result = serde_json::Map::new();
        for table in names {
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                .unwrap();
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get_ref(index).map(|value| format!("{value:?}")))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows.sort();
            result.insert(table, json!(rows));
        }
        Value::Object(result)
    }
    json!({"native":database(path),"engine":database(&path.with_extension("engine.sqlite3"))})
}

/// A rejected durable command adds exactly its receipt; every domain/Engine
/// row and every prior receipt must remain byte-for-byte unchanged.
pub(super) fn assert_rejected_control_only(
    path: &std::path::Path,
    before: &Value,
    response: &Value,
) {
    let mut after = snapshot(path);
    if response["error"]["code"] != "work_control_rejected" {
        assert_eq!(&after, before);
        return;
    }
    let receipt = &response["outcome"]["receipt"];
    assert_eq!(receipt["status"], "rejected");
    assert!(receipt["appliedControlRevision"].is_null());
    let command_id = receipt["commandId"].as_str().unwrap();
    let table = "story_work_control_commands_v1";
    let prior = before["native"][table].as_array().unwrap();
    let current = after["native"][table].as_array().unwrap();
    assert_eq!(current.len(), prior.len() + 1);
    for row in prior {
        assert!(current.contains(row), "a prior receipt was changed");
    }
    let db = rusqlite::Connection::open(path).unwrap();
    let stored: String = db.query_row(
        "SELECT receipt_json FROM story_work_control_commands_v1 WHERE author_identity='local-author' AND command_id=?1",
        [command_id], |row| row.get(0)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), *receipt);
    after["native"][table] = before["native"][table].clone();
    assert_eq!(&after, before);
}

pub(super) async fn candidate(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .unwrap()
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    )
    .oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {CANDIDATE}"))
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
    let code = response.status();
    (
        code,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

fn task_request(operation: &str, mut target: Value, step: Option<&Value>) -> Value {
    if let Some(step) = step {
        target["resumeStepId"] = step["id"].clone();
    }
    json!({"operation":operation,"target":target,"inputRevision":0,"threadId":"budget-thread",
        "authorIntent":"包装说明不得替换原始作者输入","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),
        "capabilityExpiresAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600})
}

pub(super) async fn provider_grant(
    path: &std::path::Path,
    root: &str,
    task: &Value,
) -> TaskProviderBudget {
    let (code, result) = call(
        path,
        "GET",
        &format!(
            "{root}/tasks/{}/provider-budget",
            task["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    serde_json::from_value(result).unwrap()
}

pub(super) async fn complete(path: &std::path::Path, root: &str, task: &Value) {
    let (code, result) = candidate(
        path,
        "POST",
        &format!("{root}/tasks/{}/complete", task["id"].as_str().unwrap()),
        json!({"threadId":"budget-thread","summary":"本步骤真实结果已保存或已登记交接"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
}

pub(super) async fn settle(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    grant: &TaskProviderBudget,
    usage: Option<Value>,
) -> Value {
    let mut engine=PersistentBudget::new(&path.with_extension("engine.sqlite3"),&grant.task_id,
        json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,
            "startedAt":1000},"now":1000})).unwrap();
    if let Some(usage) = usage {
        let reservation = engine
            .apply(
                "budget.reserve",
                json!({"parameters":{"estimatedInputTokens":20,"maxOutputTokens":50},"now":1001}),
            )
            .unwrap();
        engine
            .apply(
                "budget.settle",
                json!({"id":reservation,"usage":usage,"now":1002}),
            )
            .unwrap();
    }
    engine.complete("completed").unwrap();
    let receipt =
        PersistentBudget::receipt(&path.with_extension("engine.sqlite3"), &grant.task_id).unwrap();
    let (code, charged) = call(
        path,
        "POST",
        &format!("{root}/work-tasks/{}/provider-usage", grant.work_task_id),
        json!({"taskId":task["id"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{charged}");
    let charge = charged["charges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|charge| charge["taskId"] == task["id"])
        .unwrap();
    assert_eq!(charge["budgetId"], receipt["budgetId"]);
    assert_eq!(charge["status"], receipt["status"]);
    let receipt_usage = operation_digest::host_json(&receipt["usage"]).unwrap();
    assert_eq!(charge["requests"], receipt_usage["requests"]);
    assert_eq!(charge["unknownTokens"], receipt_usage["unknownTokens"]);
    charged
}

async fn work(path: &std::path::Path, root: &str, id: &str) -> Value {
    let (code, result) = call(
        path,
        "GET",
        &format!("{root}/work-tasks/{id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}

async fn decision(path: &std::path::Path, root: &str, id: &str) -> Value {
    let current = work(path, root, id).await;
    let (code, workspace) = call(
        path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{workspace}");
    let (code, result) = call(
        path,
        "POST",
        &format!("{root}/work-tasks/{id}/decision"),
        json!({
        "expectedWorkVersion":current["version"],"expectedRevision":workspace["revision"],
        "expectedSourceDigest":workspace["sourceDigest"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}

async fn read_decision(path: &std::path::Path, root: &str, task: &Value) -> Value {
    let id = task["id"].as_str().unwrap();
    let (code, context) = candidate(
        path,
        "GET",
        &format!(
            "{root}/context?taskId={id}&threadId=budget-thread&projectRevision={}&purpose=planner",
            task["inputRevision"]
        ),
        Value::Null,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{context}");
    for source in context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|source| source["sectionPath"] == "/authorRequirementsSource")
    {
        let mut cursor = String::new();
        loop {
            let (code, page) = candidate(path, "GET", &format!(
                "{root}/tasks/{id}/task-source?threadId=budget-thread&expectedRevision={}&sourceId={}{cursor}",
                task["inputRevision"], source["sourceId"].as_str().unwrap()), Value::Null).await;
            assert_eq!(code, StatusCode::OK, "{page}");
            let Some(next) = page["nextCursor"].as_str() else {
                break;
            };
            cursor = format!("&cursor={next}");
        }
    }
    let uri = format!(
        "{root}/tasks/{id}/work-decision?threadId=budget-thread&expectedRevision={}",
        task["inputRevision"]
    );
    let (code, mut result) = candidate(path, "GET", &uri, Value::Null).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    let mut cursor = result["pagination"]["nextCursor"].clone();
    while let Some(next) = cursor.as_str() {
        let (code, page) =
            candidate(path, "GET", &format!("{uri}&cursor={next}"), Value::Null).await;
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

async fn worker_before_charge(
    path: &std::path::Path,
    metered: bool,
    request_limit: Option<u64>,
) -> (String, String, Value, Value, TaskProviderBudget, Value) {
    let (root, mut request) = initial::request(path).await;
    request["capabilityHash"] = json!(format!("{:x}", Sha256::digest(CAP.as_bytes())));
    request["authorIntent"] = json!("生成完整创作简报，再根据简报组织后续创作。");
    if metered {
        for field in ["maxRequests", "maxTokens", "deadlineAt"] {
            request["initialBudget"][field] = Value::Null;
        }
    }
    if let Some(limit) = request_limit {
        request["initialBudget"]["maxRequests"] = json!(limit);
    }
    let (code, parent) = call(path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{parent}");
    let parent_grant = provider_grant(path, &root, &parent).await;
    let parent_id = parent["id"].as_str().unwrap();
    let (code,plan)=candidate(path,"POST",&format!("{root}/tasks/{parent_id}/work-plan"),json!({
        "taskId":parent_id,"threadId":"budget-thread","expectedRevision":0,"expectedPlanVersion":0,
        "idempotencyKey":"real-budget-plan","plan":{"intentMode":"executeGoals","goals":[{
            "id":"brief-goal","title":"保存简报并继续组织创作","acceptance":"有真实完整简报且后续创作已安排",
            "kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"简报是否完整",
            "checkScope":"作者要求","approach":"保存简报后根据结果安排下一行动","assessment":"",
            "evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null}]}})).await;
    assert_eq!(code, StatusCode::OK, "{plan}");
    let (code,action)=candidate(path,"POST",&format!("{root}/tasks/{parent_id}/actions"),json!({
        "taskId":parent_id,"threadId":"budget-thread","expectedRevision":0,"expectedIntentRevision":0,
        "idempotencyKey":"actual-brief-action","operation":"generateBrief","target":{"type":"artifact"},
        "instruction":"保存完整创作简报"})).await;
    assert_eq!(code, StatusCode::CREATED, "{action}");
    complete(path, &root, &parent).await;
    settle(path, &root, &parent, &parent_grant, None).await;
    let (code, approved) = call(
        path,
        "POST",
        &format!(
            "{root}/tasks/{parent_id}/actions/{}/approve",
            action["id"].as_str().unwrap()
        ),
        json!({"expectedIntentRevision":0}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{approved}");
    let (code, permission) = call(
        path,
        "GET",
        &format!(
            "{root}/tasks/{parent_id}/actions/{}/permission",
            action["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{permission}");
    assert_eq!(permission["permission"]["kind"], "exactAction");
    assert_eq!(permission["permission"]["actionId"], action["id"]);
    assert_eq!(
        permission["permission"]["workTaskId"],
        parent_grant.work_task_id
    );
    assert_eq!(
        permission["permission"]["evidence"]["sourceTaskId"],
        parent["id"]
    );
    let mut target = action["target"].clone();
    target["requestedByTaskId"] = parent["id"].clone();
    target["requestedActionId"] = action["id"].clone();
    let (code, worker) = call(
        path,
        "POST",
        &format!("{root}/tasks"),
        task_request("generateBrief", target, None),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{worker}");
    let worker_grant = provider_grant(path, &root, &worker).await;
    let (code,saved)=candidate(path,"POST",&format!("{root}/artifacts/brief"),json!({
        "taskId":worker["id"],"threadId":"budget-thread","expectedRevision":0,
        "expectedCandidateRevisionId":worker["inputManifest"]["expectedCandidateRevisionId"],
        "idempotencyKey":"real-complete-brief","payload":{"premise":"真实保存的简报，未知费用不否定已有成果"}})).await;
    assert_eq!(code, StatusCode::CREATED, "{saved}");
    complete(path, &root, &worker).await;
    (
        root,
        parent_grant.work_task_id,
        parent,
        worker,
        worker_grant,
        json!({"saved":saved}),
    )
}

/// Fault injection only: the repaired public flow cannot enqueue this planner
/// while the writer's usage is unsettled. Reproduce a queued step left by the
/// former gate, without inserting Tasks, grants, author inputs or fee receipts.
async fn legacy_queued_decision(
    path: &std::path::Path,
    root: &str,
    work_id: &str,
    origin: &Value,
    source: &Value,
) -> Value {
    let before = snapshot(path);
    let (code, workspace) = call(
        path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{workspace}");
    let target = json!({"workDecision":{"workTaskId":work_id,"originTaskId":origin["id"],
        "sourceTaskId":source["id"],"fingerprint":format!("{:x}",Sha256::digest(format!("{}:{}",
            workspace["revision"].as_i64().unwrap(),workspace["sourceDigest"].as_str().unwrap())))}});
    let db = rusqlite::Connection::open(path).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    db.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,status)
        VALUES('decision-legacy-fixture',?1,(SELECT MAX(ordinal)+1 FROM story_work_steps_v1 WHERE work_task_id=?1),
        'advisory',?2,?3,'queued')",rusqlite::params![work_id,target.to_string(),workspace["revision"].as_i64().unwrap()]).unwrap();
    drop(db);
    let mut before_facts = before;
    let mut after_facts = snapshot(path);
    before_facts["native"]
        .as_object_mut()
        .unwrap()
        .remove("story_work_steps_v1");
    after_facts["native"]
        .as_object_mut()
        .unwrap()
        .remove("story_work_steps_v1");
    assert_eq!(after_facts, before_facts);
    let current = work(path, root, work_id).await;
    current["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["id"] == "decision-legacy-fixture")
        .unwrap()
        .clone()
}

#[tokio::test]
async fn settled_real_unknown_receipt_blocks_manual_queued_decision_and_event_for_finite_limits() {
    for metered in [false] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unknown-admission.sqlite3");
        let (root, work_id, parent, worker, grant, refs) =
            worker_before_charge(&path, metered, None).await;
        let charged = settle(&path, &root, &worker, &grant, Some(json!({}))).await;
        assert_eq!(charged["usedRequests"], 1);
        assert_eq!(charged["usedTokens"], 0);
        assert_eq!(charged["unknownTokens"], 70);
        assert_eq!(charged["overrun"], false);
        let before = snapshot(&path);
        let stopped = decision(&path, &root, &work_id).await;
        assert_eq!(stopped["stopReason"], "unknownUsage");
        assert!(stopped["step"].is_null());
        assert_eq!(snapshot(&path), before);
        // Separate legacy-state injection from the all-public unknown receipt
        // chain above. An existing queued planner must not bypass the same gate.
        let early = legacy_queued_decision(&path, &root, &work_id, &parent, &worker).await;
        let before = snapshot(&path);
        let stopped = decision(&path, &root, &work_id).await;
        assert_eq!(stopped["stopReason"], "unknownUsage");
        assert!(stopped["step"].is_null());
        assert_eq!(snapshot(&path), before);
        let request = task_request("advisory", early["target"].clone(), Some(&early));
        let (code, rejected) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
        assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
        assert_eq!(rejected["error"]["code"], "work_usage_unknown");
        assert_eq!(snapshot(&path), before);
        let current = work(&path, &root, &work_id).await;
        assert_eq!(current["resumeBlockReason"], "unknownUsage");
        let (code, workspace) = call(
            &path,
            "GET",
            &format!("{root}/creative-workspace"),
            Value::Null,
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{workspace}");
        assert_eq!(
            workspace["artifacts"]["brief"]["candidate"]["id"],
            refs["saved"]["revisionId"]
        );
        let (code, events) = call(
            &path,
            "GET",
            &format!("{root}/work-execution-events"),
            Value::Null,
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{events}");
        let event = events["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["sourceAttemptId"] == worker["id"])
            .unwrap();
        let (code,claim)=call(&path,"POST",&format!("{root}/work-execution-events/{}/claim",event["eventId"].as_str().unwrap()),
            json!({"hostSessionId":"unknown-budget-host","expectedWorkVersion":current["version"],
                "expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}),AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{claim}");
        assert_eq!(claim["admission"], "blocked");
        assert_eq!(claim["blockedReason"], "unknownUsage");
        assert_eq!(snapshot(&path), before);
        assert_eq!(
            call(
                &path,
                "POST",
                &format!("{root}/work-tasks/{work_id}/provider-usage"),
                json!({"taskId":worker["id"]}),
                AUTHOR
            )
            .await,
            (StatusCode::OK, charged)
        );
        assert_eq!(snapshot(&path), before);
        let (code, original) = call(
            &path,
            "GET",
            &format!("{root}/tasks/{}", parent["id"].as_str().unwrap()),
            Value::Null,
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{original}");
        assert_eq!(original["status"], "completed");
    }
}

#[tokio::test]
async fn real_known_usage_allows_the_same_queued_decision_under_original_finite_and_null_authority()
{
    for metered in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("known-admission.sqlite3");
        let (root, work_id, _, worker, grant, _) = worker_before_charge(&path, metered, None).await;
        let charged = settle(
            &path,
            &root,
            &worker,
            &grant,
            Some(json!({"total_tokens":125})),
        )
        .await;
        assert_eq!(charged["unknownTokens"], 0);
        let resumed = decision(&path, &root, &work_id).await;
        assert!(resumed["step"].is_object(), "{resumed}");
        let early = resumed["step"].clone();
        let repeated = decision(&path, &root, &work_id).await;
        assert_eq!(repeated["step"]["id"], early["id"]);
        let request = task_request("advisory", early["target"].clone(), Some(&early));
        let (code, next) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
        assert_eq!(code, StatusCode::CREATED, "{next}");
        let next_grant = provider_grant(&path, &root, &next).await;
        assert_eq!(next_grant.work_task_id, work_id);
        assert_eq!(
            next_grant.max_requests,
            if metered { None } else { Some(2) }
        );
        assert_eq!(
            next_grant.max_tokens,
            if metered { None } else { Some(575) }
        );
        assert!(work(&path, &root, &work_id).await["resumeBlockReason"].is_null());
    }
}

#[tokio::test]
async fn exact_bound_action_rejection_settles_only_its_operation_in_existing_unknown_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unknown-bound-operation.sqlite3");
    let (root, work_id, _, worker, grant, _) = worker_before_charge(&path, false, None).await;
    settle(
        &path,
        &root,
        &worker,
        &grant,
        Some(json!({"total_tokens":125})),
    )
    .await;
    let queued = decision(&path, &root, &work_id).await;
    assert!(queued["step"].is_object(), "{queued}");
    let (code, parent) = call(
        &path,
        "POST",
        &format!("{root}/tasks"),
        task_request(
            "advisory",
            queued["step"]["target"].clone(),
            Some(&queued["step"]),
        ),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{parent}");
    let active_grant = provider_grant(&path, &root, &parent).await;
    let read = read_decision(&path, &root, &parent).await;
    let request = json!({"taskId":parent["id"],"threadId":"budget-thread",
        "expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],
        "expectedSourceDigest":read["sourceDigest"],
        "proposal":{"operation":"generateBrief","target":{"sourceTaskId":worker["id"]}},
        "instruction":"根据原作者要求继续完善已有简报","idempotencyKey":"blocked-by-unknown-history"});

    // Fault injection models a history admitted by the former missing gate.
    // The active Task/grant and historical charge were created through public
    // APIs. Only that existing charge and its aggregate are made unknown; no
    // successful record is inserted, no allowance reopened, and FK stays on.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    let encoded: String = db
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [&work_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut ledger: Value = serde_json::from_str(&encoded).unwrap();
    let charge = ledger["charges"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|charge| charge["taskId"] == worker["id"])
        .unwrap();
    charge["unknownTokens"] = json!(70);
    ledger["unknownTokens"] = json!(70);
    db.execute(
        "UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id=?2",
        rusqlite::params![ledger.to_string(), work_id],
    )
    .unwrap();
    drop(db);
    let before = snapshot(&path);
    let mut arguments = request.clone();
    arguments["projectId"] = json!(root.strip_prefix("/api/projects/").unwrap());
    let digest = operation_digest::operation_digest("request_work_action", &arguments).unwrap();
    let response = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .unwrap()
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    )
    .oneshot(
        Request::builder()
            .method("POST")
            .uri(format!(
                "{root}/tasks/{}/work-decision/actions",
                parent["id"].as_str().unwrap()
            ))
            .header("authorization", format!("Bearer {CANDIDATE}"))
            .header("x-novel-task-capability", CAP)
            .header("content-type", "application/json")
            .header("x-novel-tool-name", "request_work_action")
            .header("x-novel-root-thread", "budget-thread")
            .header("x-novel-caller-thread", "budget-thread")
            .header("x-novel-turn", "actual-unknown-history-turn")
            .header("x-novel-tool-call", "actual-unknown-history-action")
            .header("x-novel-request-hash", &digest)
            .body(Body::from(request.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();
    let code = response.status();
    let rejected: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["code"], "work_usage_unknown");
    let db = rusqlite::Connection::open(&path).unwrap();
    let operation: (String,String,Option<String>) = db.query_row(
        "SELECT state,request_hash,receipt_ref FROM story_tool_operations_v1 WHERE task_id=?1 AND call_id='actual-unknown-history-action'",
        [parent["id"].as_str().unwrap()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(operation, ("rejected".into(), digest, None));
    drop(db);
    let after = snapshot(&path);
    let mut before_facts = before.clone();
    let mut after_facts = after.clone();
    before_facts["native"]
        .as_object_mut()
        .unwrap()
        .remove("story_tool_operations_v1");
    after_facts["native"]
        .as_object_mut()
        .unwrap()
        .remove("story_tool_operations_v1");
    assert_eq!(after_facts, before_facts);
    let (code, rejected) = call(
        &path,
        "GET",
        &format!(
            "{root}/tasks/{}/provider-budget",
            parent["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["code"], "work_usage_unknown");
    assert_eq!(snapshot(&path), after);
    assert_eq!(active_grant.work_task_id, work_id);
}

#[tokio::test]
async fn actual_completed_work_can_close_its_event_without_resolving_unknown_usage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unknown-completed-work.sqlite3");
    let (root, mut request) = initial::request(&path).await;
    request["capabilityHash"] = json!(format!("{:x}", Sha256::digest(CAP.as_bytes())));
    let (code, task) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let grant = provider_grant(&path, &root, &task).await;
    complete(&path, &root, &task).await;
    let charged = settle(&path, &root, &task, &grant, Some(json!({}))).await;
    let before = snapshot(&path);
    let stopped = decision(&path, &root, &grant.work_task_id).await;
    assert_eq!(stopped["stopReason"], "completed");
    assert!(stopped["step"].is_null());
    assert_eq!(snapshot(&path), before);
    let current = work(&path, &root, &grant.work_task_id).await;
    assert_eq!(current["status"], "completed");
    let (code, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    let (code, events) = call(
        &path,
        "GET",
        &format!("{root}/work-execution-events"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    let event = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["sourceAttemptId"] == task["id"])
        .unwrap();
    let uri = format!(
        "{root}/work-execution-events/{}/claim",
        event["eventId"].as_str().unwrap()
    );
    let claim_request = json!({"hostSessionId":"unknown-completed-host", "expectedWorkVersion":current["version"],
        "expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]});
    let (code, claim) = call(&path, "POST", &uri, claim_request.clone(), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{claim}");
    assert_eq!(claim["admission"], "settled");
    assert_eq!(claim["event"]["state"], "consumed");
    assert!(claim["lease"].is_null());
    let mut before_facts = before;
    let mut after_facts = snapshot(&path);
    before_facts["native"]
        .as_object_mut()
        .unwrap()
        .remove("story_work_execution_events_v1");
    after_facts["native"]
        .as_object_mut()
        .unwrap()
        .remove("story_work_execution_events_v1");
    assert_eq!(after_facts, before_facts);
    let after = snapshot(&path);
    assert_eq!(
        call(&path, "POST", &uri, claim_request, AUTHOR).await,
        (StatusCode::OK, claim)
    );
    assert_eq!(snapshot(&path), after);
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{root}/work-tasks/{}/provider-usage", grant.work_task_id),
            json!({"taskId":task["id"]}),
            AUTHOR
        )
        .await,
        (StatusCode::OK, charged)
    );
    assert_eq!(snapshot(&path), after);
}

#[tokio::test]
async fn trusted_late_usage_reaches_original_work_via_task_id_only() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("late-usage.sqlite3");
    let (root, mut request) = initial::request(&path).await;
    request["capabilityHash"] = json!(format!("{:x}", Sha256::digest(CAP.as_bytes())));
    let (code, task) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let grant = provider_grant(&path, &root, &task).await;
    complete(&path, &root, &task).await;
    let engine_path = path.with_extension("engine.sqlite3");
    let mut engine=PersistentBudget::new(&engine_path,&grant.task_id,json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"startedAt":1000},"now":1000})).unwrap();
    let id = engine
        .apply(
            "budget.reserve",
            json!({"parameters":{"estimatedInputTokens":20,"maxOutputTokens":50},"now":1001}),
        )
        .unwrap();
    engine
        .apply("budget.settle", json!({"id":id,"usage":null}))
        .unwrap();
    engine.complete("completed").unwrap();
    drop(engine);
    let uri = format!("{root}/work-tasks/{}/provider-usage", grant.work_task_id);
    let (code, first) = call(&path, "POST", &uri, json!({"taskId":task["id"]}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{first}");
    assert_eq!(first["unknownTokens"], 70);
    let (code, pending) = call(&path, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(pending["pending"]["taskId"], task["id"]);
    PersistentBudget::reconcile_usage(
        &engine_path,
        &grant.task_id,
        json!({"id":id,"usage":{"total_tokens":17},"receiptRef":"verified-provider-request"}),
    )
    .unwrap();
    let (code, settled) = call(&path, "POST", &uri, json!({"taskId":task["id"]}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{settled}");
    assert_eq!(settled["unknownTokens"], 0);
    assert_eq!(settled["usedTokens"], 17);
    assert_eq!(settled["usedRequests"], 1);
    let (code, replayed) = call(&path, "POST", &uri, json!({"taskId":task["id"]}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(replayed, settled);
    let (code, _) = call(
        &path,
        "POST",
        &uri,
        json!({"taskId":task["id"],"usage":{"total_tokens":0}}),
        AUTHOR,
    )
    .await;
    assert!(code.is_client_error());
}

#[tokio::test]
async fn metering_only_work_continues_with_original_unknown_charge_unchanged() {
    for request_limit in [None, Some(6)] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metering-only-recovery.sqlite3");
        let (root, work_id, _, worker, grant, _) =
            worker_before_charge(&path, true, request_limit).await;
        let charged = settle(&path, &root, &worker, &grant, Some(json!({}))).await;
        assert_eq!(charged["unknownTokens"], 70);
        let resumed = decision(&path, &root, &work_id).await;
        assert!(resumed["step"].is_object(), "{resumed}");
        let step = &resumed["step"];
        let (code, next) = call(
            &path,
            "POST",
            &format!("{root}/tasks"),
            task_request("advisory", step["target"].clone(), Some(step)),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{next}");
        let next_grant = provider_grant(&path, &root, &next).await;
        assert_eq!(next_grant.work_task_id, work_id);
        assert_eq!(
            (
                next_grant.max_requests,
                next_grant.max_tokens,
                next_grant.deadline_at
            ),
            (request_limit.map(|limit| limit - 1), None, None)
        );
        assert_ne!(next_grant.task_id, grant.task_id);
        // Re-reading the original receipt cannot clear or double-charge it.
        let (code, replayed) = call(
            &path,
            "POST",
            &format!("{root}/work-tasks/{work_id}/provider-usage"),
            json!({"taskId":worker["id"]}),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{replayed}");
        assert_eq!(replayed, charged);
    }
}
