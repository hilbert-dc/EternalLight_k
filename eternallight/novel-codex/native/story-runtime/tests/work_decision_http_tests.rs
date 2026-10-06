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
    let app = router(fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        json!({"title":"同额度协调"}),
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
        task("advisory", json!({"type":"project"})),
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
        let first = status(app, base, &current).await;
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
            body["deliveryReview"] = json!({"expectedWorkVersion":first["version"],"expectedSourceDigest":first["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"本次授权范围的保存成果已核对","checkedScope":"实际保存回执；不声明文学质量认证","unresolved":[],"authorDecisions":[],"evidence":evidence});
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
    let (status,result)=call(app,"POST",&format!("{base}/tasks/{parent}/actions"),json!({"taskId":parent,"threadId":"parent-thread","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":key,"operation":operation,"target":{"type":"artifact"},"instruction":"根据实际回执重新调整方向，保留其他内容"}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    result
}
async fn child(app: &Router, base: &str, parent: &str, action: &Value) -> Value {
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
        task("generateBrief", target),
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
    assert_eq!(status, StatusCode::CREATED, "{result}");
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
    assert_eq!(context["taskSources"]["readTool"], "read_task_source");
    let item = json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":json!({"content":[{"type":"text","text":context.to_string()}]}).to_string()});
    assert!(
        eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
            .unwrap()
            .as_u64()
            .unwrap()
            <= 9000
    );
    let (status, result) = call(
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
    call(app,"POST",&format!("{base}/tasks/{id}/work-decision/actions"),json!({"taskId":id,"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":key,"instruction":"根据真实失败回执修正参数，保持原作者范围","idempotencyKey":"selected-action"}),CANDIDATE).await
}
#[tokio::test]
async fn saved_child_receipt_parent_and_finish_preserve_original_work_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("success.sqlite3");
    let (app, base, project, root) = setup(&path).await;
    let id = root["id"].as_str().unwrap();
    let first = action(&app, &base, id, "generateBrief", "first").await;
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
    let next = decision(&app, &base, id).await;
    assert_eq!(decision(&app, &base, id).await, next);
    let parent = claim(&app, &base, &next["step"], "parent-thread").await;
    assert_eq!(
        parent["inputManifest"]["authorIntent"],
        root["inputManifest"]["authorIntent"]
    );
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
    for retry_succeeds in [false, true] {
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
                    error: Some("invalid_tool_arguments; private prose".into()),
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
        assert!(!read.to_string().contains("private prose"));
        assert!(
            !select(&app, &base, &parent, &read, "forged-key")
                .await
                .0
                .is_success()
        );
        let key = read["readyActions"][0]["actionKey"].as_str().unwrap();
        assert_eq!(read["readyActions"][0]["operation"], "generateBrief");
        let (code, chosen) = select(&app, &base, &parent, &read, key).await;
        assert_eq!(code, StatusCode::OK, "{chosen}");
        assert_eq!(
            select(&app, &base, &parent, &read, key).await,
            (StatusCode::OK, chosen.clone())
        );
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
            assert_eq!(read["canFinish"], false);
            assert_eq!(read["canReportPartial"], true);
            let retry = &read["readyActions"][0];
            assert_eq!(retry["recovery"]["failedTaskId"], retry_id);
            assert_eq!(retry["recovery"]["sameConditionFailures"], 2);
            assert_eq!(retry["recovery"]["planRequired"], true);
            let key = retry["actionKey"].as_str().unwrap();
            assert!(!select(&app, &base, &parent, &read, key).await.0.is_success());
            let plan = json!({"failedTaskId":retry_id,"cause":"工具参数与保存契约不符","change":"先读取保存结构，再只提交简报所需字段","verification":"核对回执与简报内容"});
            let request = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":key,"instruction":"修正参数后保存创作简报","idempotencyKey":"changed-strategy","recoveryPlan":plan});
            let endpoint = format!("{base}/tasks/{}/work-decision/actions",parent["id"].as_str().unwrap());
            let (code, selected) = call(&app,"POST",&endpoint,request.clone(),CANDIDATE).await;
            assert_eq!(code,StatusCode::OK,"{selected}");
            assert_eq!(selected["step"]["target"]["recoveryPlan"],plan);
            assert_eq!(call(&app,"POST",&endpoint,request.clone(),CANDIDATE).await,(StatusCode::OK,selected.clone()));
            let mut changed_replay=request; changed_replay["recoveryPlan"]["change"]=json!("另一种策略");
            assert!(!call(&app,"POST",&endpoint,changed_replay,CANDIDATE).await.0.is_success());
            complete(&app,&base,parent["id"].as_str().unwrap()).await;
            settle(&path,&project,parent["id"].as_str().unwrap(),"completed");
            let worker=claim(&app,&base,&selected["step"],"parent-thread").await;
            let recovery_id=worker["id"].as_str().unwrap();
            SqliteStoryRepository::open(&path).unwrap().update_task(&ProjectId::parse(&project).unwrap(),recovery_id,"parent-thread",TaskUpdate{status:"failed".into(),error:Some("TaskOutputMissing".into()),result_summary:None,now:3}).unwrap();
            settle(&path,&project,recovery_id,"failed");
            let next=decision(&app,&base,id).await;
            let parent=claim(&app,&base,&next["step"],"parent-thread").await;
            let read=status(&app,&base,&parent).await;
            let mut repeated=plan; repeated["failedTaskId"]=json!(recovery_id);
            let request=json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":read["readyActions"][0]["actionKey"],"instruction":"再次尝试同一方法","idempotencyKey":"repeat-strategy","recoveryPlan":repeated});
            let endpoint=format!("{base}/tasks/{}/work-decision/actions",parent["id"].as_str().unwrap());
            let (code,rejection)=call(&app,"POST",&endpoint,request,CANDIDATE).await;
            assert!(!code.is_success(),"{rejection}");
            assert!(rejection.to_string().contains("already failed"),"{rejection}");
            let new_plan=json!({"failedTaskId":recovery_id,"cause":"未能完成保存","change":"先重新读取简报目标，再提交最小完整简报并立刻完成","verification":"核对完整简报回执"});
            let request=json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":read["readyActions"][0]["actionKey"],"instruction":"换一种保存步骤","idempotencyKey":"different-strategy","recoveryPlan":new_plan});
            let (code,chosen)=call(&app,"POST",&endpoint,request,CANDIDATE).await;
            assert_eq!(code,StatusCode::OK,"{chosen}");
            complete(&app,&base,parent["id"].as_str().unwrap()).await;
            settle(&path,&project,parent["id"].as_str().unwrap(),"completed");
            let worker=claim(&app,&base,&chosen["step"],"parent-thread").await;
            let last_id=worker["id"].as_str().unwrap();
            SqliteStoryRepository::open(&path).unwrap().update_task(&ProjectId::parse(&project).unwrap(),last_id,"parent-thread",TaskUpdate{status:"failed".into(),error:Some("TaskOutputMissing".into()),result_summary:None,now:4}).unwrap();
            settle(&path,&project,last_id,"failed");
            let next=decision(&app,&base,id).await;
            let parent=claim(&app,&base,&next["step"],"parent-thread").await;
            let read=status(&app,&base,&parent).await;
            assert_eq!(read["readyActions"][0]["recovery"]["sameConditionFailures"],4);
            assert_eq!(read["readyActions"][0]["recovery"]["exhausted"],true);
            assert_eq!(read["canReportPartial"],true);
            let request=json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":read["readyActions"][0]["actionKey"],"instruction":"再次换一种表述","idempotencyKey":"over-limit","recoveryPlan":{"failedTaskId":last_id,"cause":"未保存","change":"第三种不同的说法","verification":"核对回执"}});
            let endpoint=format!("{base}/tasks/{}/work-decision/actions",parent["id"].as_str().unwrap());
            let (code,rejection)=call(&app,"POST",&endpoint,request,CANDIDATE).await;
            assert!(!code.is_success(),"{rejection}");
            assert!(rejection.to_string().contains("allowance exhausted"),"{rejection}");
            let proposed=json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"proposal":{"operation":"generateBrief","target":{"sourceTaskId":last_id}},"instruction":"改名为新任务也不能重置失败额度","idempotencyKey":"proposal-over-limit"});
            let (code,rejection)=call(&app,"POST",&endpoint,proposed,CANDIDATE).await;
            assert!(!code.is_success(),"{rejection}");
            assert!(rejection.to_string().contains("allowance exhausted"),"{rejection}");
            let mut review=json!({"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"简报未能保存","checkedScope":"保存回执核对","unresolved":[],"authorDecisions":[],"evidence":[]});
            let endpoint=format!("{base}/tasks/{}/complete",parent["id"].as_str().unwrap());
            let body=json!({"threadId":"parent-thread","summary":"停止重复尝试，保留未完成事项","deliveryReview":review});
            assert!(!call(&app,"POST",&endpoint,body,CANDIDATE).await.0.is_success());
            review["goalStatus"]=json!("partial"); review["unresolved"]=json!(["简报保存仍未完成，已停止重复尝试"]);
            let (code,result)=call(&app,"POST",&endpoint,json!({"threadId":"parent-thread","summary":"已停止重复尝试","deliveryReview":review}),CANDIDATE).await;
            assert_eq!(code,StatusCode::OK,"{result}");
            settle(&path,&project,parent["id"].as_str().unwrap(),"completed");
            assert_eq!(decision(&app,&base,id).await["stopReason"],"completed");
            let db=rusqlite::Connection::open(&path).unwrap();
            let grants:i64=db.query_row("SELECT COUNT(*) FROM story_work_budget_authorizations_v1",[],|r|r.get(0)).unwrap();
            assert_eq!(grants,1);
            continue;
        }
        save(&app, &base, &worker, "corrected").await;
        complete(&app, &base, worker["id"].as_str().unwrap()).await;
        settle(&path, &project, worker["id"].as_str().unwrap(), "completed");
        let next = decision(&app, &base, id).await;
        let parent = claim(&app, &base, &next["step"], "parent-thread").await;
        assert_eq!(status(&app, &base, &parent).await["canFinish"], true);
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
async fn next(app: &Router, uri: &str) -> Value {
    let (code, result) = call(app, "POST", &format!("{uri}/next"), json!({}), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}
async fn run_claim(app: &Router, base: &str, action: &Value) -> Value {
    let step = json!({"operation":action["operation"],"target":action["target"],"id":action["target"]["resumeStepId"],"inputRevision":workspace(app,base).await["revision"]});
    claim(app, base, &step, "parent-thread").await
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
    let endpoint = format!("{base}/tasks/{}/work-decision/actions",parent["id"].as_str().unwrap());
    let proposal = json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
        "proposal":{"operation":"generateChapter","target":{"chapterId":chapter,"scope":{"type":"selectedScenes","groupIds":["group-1"]}}},
        "instruction":"只补充信件署名，保留其他场景与标题","idempotencyKey":"repair-selected-scene"});
    let mut outside = proposal.clone();
    outside["proposal"]["target"]["chapterId"] = json!("another-chapter");
    assert!(!call(&app,"POST",&endpoint,outside,CANDIDATE).await.0.is_success());
    let (code,selected) = call(&app,"POST",&endpoint,proposal,CANDIDATE).await;
    assert_eq!(code,StatusCode::OK,"{selected}");
    assert_eq!(selected["step"]["target"]["scope"]["sourceRevisionId"],saved["revisionId"]);
    advance(&app, &path, &project, &uri, &parent).await;
    let repair = run_claim(&app,&base,&next(&app,&uri).await["action"]).await;
    assert_eq!(repair["target"]["scope"]["type"],"selectedScenes");
    assert_eq!(repair["inputManifest"]["generationAuthorIntentBinding"]["runId"],run["id"]);
    let repaired = json!({"chapterId":chapter,"chapterNumber":1,"title":"来信",
        "body":"林夕拾起信件，看清署名。\n\n---\n\n雨仍未停。",
        "sceneGroups":[{"id":"group-1","sceneId":"scene-1","body":"林夕拾起信件，看清署名。","locked":false},
            {"id":"group-2","sceneId":"scene-1","body":"雨仍未停。","locked":false}],
        "sourcePlanRefs":{"chapterPlanId":"plan-1","planRevisionId":plan["revisionId"],"outlineRevisionId":null,"localOnly":false,"lineageStatus":"current"}});
    let checkpoint = json!({"taskId":repair["id"],"threadId":"parent-thread","expectedRevision":repair["inputRevision"],
        "expectedCandidateRevisionId":repair["inputManifest"]["expectedCandidateRevisionId"],
        "idempotencyKey":"selected-draft","completeness":"chapterComplete","payload":repaired});
    for field in ["title","unselected","lock"] {
        let mut invalid = checkpoint.clone();
        match field {
            "title" => invalid["payload"]["title"] = json!("越权改题"),
            "unselected" => invalid["payload"]["sceneGroups"][1]["body"] = json!("越权改文"),
            _ => invalid["payload"]["sceneGroups"][0]["locked"] = json!(true),
        }
        assert!(!call(&app,"POST",&format!("{base}/drafts/checkpoints"),invalid,CANDIDATE).await.0.is_success());
    }
    let (code,repaired_saved) = call(&app,"POST",&format!("{base}/drafts/checkpoints"),checkpoint,CANDIDATE).await;
    assert_eq!(code,StatusCode::CREATED,"{repaired_saved}");
    advance(&app,&path,&project,&uri,&repair).await;
    let parent = run_claim(&app, &base, &next(&app, &uri).await["action"]).await;
    let read = status(&app, &base, &parent).await;
    assert_eq!(read["canFinish"], false);
    assert!(!read["requirements"].as_array().unwrap().iter().any(|r| r["kind"] == "knowledge"));
    assert!(read["readyActions"].as_array().unwrap().iter().any(|a| a["operation"] == "modelKnowledge" && a["required"] == false));
    assert!(read["readyActions"].as_array().unwrap().iter().any(|a| a["operation"] == "finishChapter"));
    assert_eq!(
        status(&app, &base, &parent).await["finalDeliveryReviewRequired"],
        true
    );
    // An optional analysis failure stays visible without becoming a mandatory pipeline.
    choose_generation(&app, &base, &parent, "modelKnowledge").await;
    advance(&app, &path, &project, &uri, &parent).await;
    let analyzer=run_claim(&app,&base,&next(&app,&uri).await["action"]).await;
    let analyzer_id=analyzer["id"].as_str().unwrap();
    SqliteStoryRepository::open(&path).unwrap().update_task(&ProjectId::parse(&project).unwrap(),analyzer_id,"parent-thread",TaskUpdate{status:"failed".into(),error:Some("invalid_tool_arguments".into()),result_summary:None,now:5}).unwrap();
    let (grant,_)=terminal_receipt(&path,analyzer_id,"failed");
    let (code,result)=call(&app,"POST",&format!("{uri}/provider-usage"),json!({"taskId":analyzer_id}),AUTHOR).await;
    assert_eq!(code,StatusCode::OK,"{result} grant={grant:?}");
    let parent=run_claim(&app,&base,&next(&app,&uri).await["action"]).await;
    let read=status(&app,&base,&parent).await;
    assert!(read["requirements"].as_array().unwrap().iter().any(|r|r["kind"]=="optionalAnalysis"&&r["status"]=="failed"));
    assert!(read["readyActions"].as_array().unwrap().iter().any(|a|a["operation"]=="finishChapter"));
    let retry=read["readyActions"].as_array().unwrap().iter().find(|a|a["operation"]=="modelKnowledge").unwrap();
    assert_eq!(retry["required"],false);
    assert_eq!(retry["recovery"]["failedTaskId"],analyzer_id);
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
    let temp=tempfile::tempdir().unwrap();
    let path=temp.path().join("agent-proposed-revision.sqlite3");
    let (app,base,project,root)=setup(&path).await;
    let root_id=root["id"].as_str().unwrap();
    let first=action(&app,&base,root_id,"generateBrief","initial").await;
    complete(&app,&base,root_id).await;
    settle(&path,&project,root_id,"completed");
    let worker=child(&app,&base,root_id,&first).await;
    save(&app,&base,&worker,"first-draft").await;
    let worker_id=worker["id"].as_str().unwrap();
    complete(&app,&base,worker_id).await;
    settle(&path,&project,worker_id,"completed");
    let next=decision(&app,&base,root_id).await;
    let parent=claim(&app,&base,&next["step"],"parent-thread").await;
    let read=status(&app,&base,&parent).await;
    assert_eq!(read["readyActions"],json!([]));
    let endpoint=format!("{base}/tasks/{}/work-decision/actions",parent["id"].as_str().unwrap());
    let proposal=json!({"taskId":parent["id"],"threadId":"parent-thread","expectedRevision":parent["inputRevision"],
        "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
        "proposal":{"operation":"generateBrief","target":{"sourceTaskId":worker_id}},
        "instruction":"只进一步修正已经授权的创作简报，保持其他内容","idempotencyKey":"new-revision"});
    for (key,value) in [("operation",json!("generateOutline")),("target",json!({"sourceTaskId":"foreign-task"}))] {
        let mut invalid=proposal.clone(); invalid["proposal"][key]=value;
        let (code,rejected)=call(&app,"POST",&endpoint,invalid,CANDIDATE).await;
        assert!(code.is_client_error(),"{code}: {rejected}");
    }
    let mut forged_grant=proposal.clone();
    forged_grant["proposal"]["target"]["budgetId"]=json!("new-grant");
    let (code,rejected)=call(&app,"POST",&endpoint,forged_grant,CANDIDATE).await;
    assert_eq!(code,StatusCode::UNPROCESSABLE_ENTITY,"{rejected}");
    let rejection=rejected.as_str().expect("Axum reports malformed JSON DTOs as text");
    assert!(rejection.contains("unknown field `budgetId`"),"{rejection}");
    let mut conflicting=proposal.clone(); conflicting["actionKey"]=json!("also-selected");
    assert!(call(&app,"POST",&endpoint,conflicting,CANDIDATE).await.0.is_client_error());
    let mut stale=proposal.clone(); stale["expectedWorkVersion"]=json!(-1);
    assert!(call(&app,"POST",&endpoint,stale,CANDIDATE).await.0.is_client_error());
    let lock_db=rusqlite::Connection::open(&path).unwrap();
    lock_db.execute("UPDATE story_object_heads_v2 SET locked=1 WHERE project_id=?1 AND object_id='artifact-brief'",[&project]).unwrap();
    assert!(call(&app,"POST",&endpoint,proposal.clone(),CANDIDATE).await.0.is_client_error());
    assert_eq!(lock_db.query_row("SELECT COUNT(*) FROM story_work_decision_actions_v1",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    lock_db.execute("UPDATE story_object_heads_v2 SET locked=0 WHERE project_id=?1 AND object_id='artifact-brief'",[&project]).unwrap();
    let (code,chosen)=call(&app,"POST",&endpoint,proposal.clone(),CANDIDATE).await;
    assert_eq!(code,StatusCode::OK,"{chosen}");
    assert_eq!(call(&app,"POST",&endpoint,proposal.clone(),CANDIDATE).await,(StatusCode::OK,chosen.clone()));
    let mut changed=proposal; changed["proposal"]["target"]["sourceTaskId"]=json!(root_id);
    assert!(call(&app,"POST",&endpoint,changed,CANDIDATE).await.0.is_client_error());
    complete(&app,&base,parent["id"].as_str().unwrap()).await;
    settle(&path,&project,parent["id"].as_str().unwrap(),"completed");
    let revised=claim(&app,&base,&chosen["step"],"parent-thread").await;
    assert_eq!(revised["inputManifest"]["authorIntent"],root["inputManifest"]["authorIntent"]);
    assert_eq!(revised["inputManifest"]["workDecisionAction"]["workTaskId"],chosen["workTaskId"]);
    save(&app,&base,&revised,"agent-refined").await;
    complete(&app,&base,revised["id"].as_str().unwrap()).await;
    settle(&path,&project,revised["id"].as_str().unwrap(),"completed");
    let db=rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_work_tasks_v1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_work_budget_authorizations_v1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_work_decision_actions_v1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
