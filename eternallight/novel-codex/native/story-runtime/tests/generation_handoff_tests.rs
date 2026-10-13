use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[path = "support/author_task_fixture.rs"]
mod fixture;
#[path = "support/settle_task_fixture.rs"]
mod usage;
use fixture::AuthorTaskFixture;
const AUTHOR: &str = "generation-handoff-author-1234567890";
const CANDIDATE: &str = "generation-handoff-candidate-1234567";
const CAP: &str = "generation-handoff-capability-12345";
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("x-novel-task-capability", CAP)
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
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn deliverable_goal(id: &str, title: &str) -> Value {
    json!({"id":id,"title":title,"acceptance":"登记真实行动并核对其保存回执，不把未来执行当作已交付",
        "kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"",
        "checkScope":title,"approach":"依据作者目标登记必要行动","assessment":"",
        "evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null})
}
fn continuation_goal(run: &Value) -> Value {
    let mut goal = deliverable_goal("resume", "按原范围接续原任务");
    goal["kind"] = json!("continuation");
    goal["acceptance"] = json!("为原任务登记真实交接，不新建范围或预算");
    goal["continuation"] = json!({"runId":run["id"],"proposalId":null});
    goal
}
fn assert_model_item_bound(value: &Value) {
    let result = json!({"content":[{"type":"text","text":value.to_string()}]});
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":result.to_string()});
    let tokens = eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(tokens <= 9000, "actual model item has {tokens} tokens");
}
async fn read_current_author_requirements(app: &Router, base: &str, task: &Value) {
    let id = task["id"].as_str().unwrap();
    let thread = task["threadId"].as_str().unwrap();
    let (code, pack) = call(app, "GET", &format!(
        "{base}/context?taskId={id}&threadId={thread}&projectRevision={}&purpose={}&byteBudget=60000",
        task["inputRevision"], task["purpose"].as_str().unwrap()), Value::Null, CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{pack}");
    assert_model_item_bound(&pack);
    let source = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sectionPath"] == "/authorRequirementsSource")
        .unwrap();
    assert_eq!(
        pack["authorRequirementsSource"]["sourceId"],
        source["sourceId"]
    );
    assert_eq!(pack["authorRequirementsSource"]["textDelivered"], false);
    let mut cursor = String::new();
    let mut text = String::new();
    loop {
        let (code, page) = call(app, "GET", &format!(
            "{base}/tasks/{id}/task-source?threadId={thread}&expectedRevision={}&sourceId={}{cursor}",
            task["inputRevision"], source["sourceId"].as_str().unwrap()), Value::Null, CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source["sourceId"]);
        assert_eq!(page["sectionPath"], source["sectionPath"]);
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert_eq!(page["byteRange"]["start"], text.len());
        assert_eq!(page["byteRange"]["total"], source["payloadBytes"]);
        let part = page["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        assert!(page.to_string().len() <= 6500);
        assert_model_item_bound(&page);
        text.push_str(part);
        assert_eq!(page["byteRange"]["end"], text.len());
        if let Some(next) = page["nextCursor"].as_str() {
            let next = format!("&cursor={next}");
            assert_ne!(next, cursor);
            cursor = next;
        } else {
            assert_eq!(page["byteRange"]["total"], text.len());
            break;
        }
    }
    assert_eq!(
        source["contentHash"],
        format!("{:x}", Sha256::digest(text.as_bytes()))
    );
    let required: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(required["authority"], "frozenAuthorIntent");
    assert_eq!(required["intentRevision"], 0);
    assert_eq!(
        required["authorText"],
        task["inputManifest"]["authorIntent"]
    );
    assert!(!required["authorText"].as_str().unwrap().is_empty());
}
async fn save_plan(app: &Router, base: &str, task: &Value, version: i64, goals: Value) {
    read_current_author_requirements(app, base, task).await;
    let id = task["id"].as_str().unwrap();
    let (code, result) = call(
        app,
        "POST",
        &format!("{base}/tasks/{id}/work-plan"),
        json!({"taskId":id,"threadId":"original-thread","expectedRevision":task["inputRevision"],
            "expectedPlanVersion":version,"idempotencyKey":format!("model-goals-{version}"),
            "plan":{"intentMode":"executeGoals","goals":goals}}),
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{result}");
}
async fn complete_scope(app: &Router, base: &str, task: &Value) {
    // A new scope is a proposal awaiting actual author approval, not saved chapters.
    let mut goal = deliverable_goal("scope", "提出创作范围并等待作者批准");
    goal["status"] = json!("blocked");
    goal["assessment"] = json!("范围提案已登记；作者尚未批准，正文执行不能开始");
    save_plan(app, base, task, 1, json!([goal])).await;
    let id = task["id"].as_str().unwrap();
    let (code, context) = call(
        app,
        "GET",
        &format!(
            "{base}/tasks/{id}/work-decision?threadId=original-thread&expectedRevision={}",
            task["inputRevision"]
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{context}");
    let (code, result) = call(app, "POST", &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":"original-thread","summary":"范围提案已登记，等待作者批准；尚未生成正文",
            "deliveryReview":{"expectedWorkVersion":context["version"],"expectedSourceDigest":context["sourceDigest"],
                "goalStatus":"partial","goalAssessment":"只有范围提案，实际正文尚未开始",
                "checkedScope":"提案登记与作者批准状态","unresolved":[{"issue":{"issueId":"fixture-unfinished-0","newIssue":{"summary":"正文执行等待作者批准范围"},"resolutionOf":[]},"observationId":null,"reason":"正文执行等待作者批准范围"}],
                "issueResolutions":[],"authorDecisions":["是否批准该创作范围"],"evidence":[]}}), CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
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
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"范围提案验收"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = created["project"]["id"].as_str().unwrap().to_owned();
    let base = format!("/api/projects/{project}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,task)=call(&app,"POST",&format!("{base}/tasks"),json!({"operation":"advisory","threadId":"original-thread",
        "inputRevision":0,"target":{"type":"project"},"authorIntent":"写两章中文悬疑小说，每章八百字。",
        "capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    save_plan(
        &app,
        &base,
        &task,
        0,
        json!([deliverable_goal("scope", "提出创作范围并等待作者批准")]),
    )
    .await;
    (app, base, project, task)
}
async fn propose(app: &Router, base: &str, task: &Value) -> Value {
    let id = task["id"].as_str().unwrap();
    let path = format!("{base}/tasks/{id}/generation-handoff");
    let body = json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,
        "targetChapters":2,"instruction":"从失踪的钟表匠开始，连续写两章候选。","idempotencyKey":"two-chapters"});
    let (status, value) = call(app, "POST", &path, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(
        call(app, "POST", &path, body, CANDIDATE).await,
        (StatusCode::OK, value.clone())
    );
    value["proposal"].clone()
}
fn start_request(proposal: &Value) -> Value {
    json!({"expectedProjectRevision":proposal["expectedRevision"],"expectedBaseSnapshotId":proposal["baseSnapshotId"],
        "expectedSourceDigest":proposal["sourceDigest"],"authorIntent":proposal["authorIntent"],"targetChapters":proposal["targetChapters"],
        "generationProposalId":proposal["proposalId"],"revisionPredecessor":null,"maxTaskAttempts":null,
        "maxProviderRequests":null,"maxProviderTokens":null,"deadlineAt":null})
}

#[tokio::test]
async fn proposal_is_not_permission_and_confirm_consumes_exact_source_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("handoff.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let proposal = propose(&app, &base, &task).await;
    let id = task["id"].as_str().unwrap();
    let (code, recovered) = call(&app,"GET", &format!("{base}/tasks/{id}/work-decision?threadId=original-thread&expectedRevision=0&view=conversationRuns"),Value::Null,CANDIDATE).await;
    assert_eq!(code,StatusCode::OK,"{recovered}");
    assert_eq!(recovered["generationHandoff"]["proposal"],proposal);
    assert_eq!(recovered["generationHandoff"]["idempotencyKey"],"two-chapters");
    assert_eq!(recovered["generationHandoff"]["consumedRunId"],Value::Null);
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(workspace["chapters"], json!([]));
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    complete_scope(&app, &base, &task).await;
    // Completion alone cannot bypass an unsettled provider receipt.
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    usage::settle(&path, &project, id);
    let mut wrong = start_request(&proposal);
    wrong["targetChapters"] = json!(3);
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            wrong,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, run) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs"),
        start_request(&proposal),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(
        json!({"origin":run["originTaskId"],"thread":run["rootThreadId"],"proposal":run["generationProposalId"]}),
        json!({"origin":id,"thread":"original-thread","proposal":proposal["proposalId"]})
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{base}/tasks/{id}/generation-handoff"),
            Value::Null,
            AUTHOR
        )
        .await,
        (StatusCode::OK, json!({"proposal":null}))
    );
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&proposal),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
}

#[tokio::test]
async fn changed_story_requires_a_new_proposal_even_if_author_supplies_fresh_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stale.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let proposal = propose(&app, &base, &task).await;
    let id = task["id"].as_str().unwrap();
    complete_scope(&app, &base, &task).await;
    usage::settle(&path, &project, id);
    let (status, value) = call(
        &app,
        "POST",
        &format!("{base}/artifacts/brief"),
        json!({"expectedRevision":0,"expectedCandidateRevisionId":null,
        "idempotencyKey":"new-direction","payload":{"premise":"作者修改为海上冒险"}}),
        AUTHOR,
    )
    .await;
    assert!(status.is_success(), "{value}");
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let mut request = start_request(&proposal);
    request["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    request["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
    request["expectedProjectRevision"] = workspace["revision"].clone();
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            request,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, runs) = call(
        &app,
        "GET",
        &format!("{base}/generation-runs"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!((status, runs), (StatusCode::OK, json!({"runs":[]})));
}

#[tokio::test]
async fn rejected_proposal_can_be_corrected_without_an_unknown_write_fence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proposal-recovery.sqlite3");
    let (app, base, _, task) = setup(&path).await;
    let id = task["id"].as_str().unwrap();
    for (call_id, intent, expected_status) in [
        ("invalid-intent", 1, StatusCode::BAD_REQUEST),
        ("corrected-intent", 0, StatusCode::OK),
    ] {
        let body = json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":intent,
            "targetChapters":2,"instruction":"按作者要求写两章候选","idempotencyKey":"scope-proposal"});
        let response = app
            .clone()
            .author_fixture_request(
                Request::builder()
                    .method("POST")
                    .uri(format!("{base}/tasks/{id}/generation-handoff"))
                    .header("authorization", format!("Bearer {CANDIDATE}"))
                    .header("content-type", "application/json")
                    .header("x-novel-task-capability", CAP)
                    .header("x-novel-tool-name", "request_generation_handoff")
                    .header("x-novel-root-thread", "original-thread")
                    .header("x-novel-caller-thread", "original-thread")
                    .header("x-novel-turn", "proposal-turn")
                    .header("x-novel-tool-call", call_id)
                    .header(
                        "x-novel-request-hash",
                        format!("{:x}", Sha256::digest(body.to_string())),
                    )
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected_status);
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    let rows=db.prepare("SELECT call_id,state,receipt_ref IS NOT NULL FROM story_tool_operations_v1 ORDER BY call_id").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,bool>(2)?))).unwrap()
        .collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(
        rows,
        vec![
            ("corrected-intent".into(), "committed".into(), true),
            ("invalid-intent".into(), "rejected".into(), false)
        ]
    );
}

#[tokio::test]
async fn hundred_chapter_scope_is_author_bound_and_not_a_three_chapter_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long-scope.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let id = task["id"].as_str().unwrap();
    let uri = format!("{base}/tasks/{id}/generation-handoff");
    let mut body = json!({"taskId":id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,
        "targetChapters":101,"instruction":"规划长期创作范围，等待作者批准。","idempotencyKey":"long-scope"});
    assert!(
        !call(&app, "POST", &uri, body.clone(), CANDIDATE)
            .await
            .0
            .is_success()
    );
    body["targetChapters"] = json!(100);
    let (status, result) = call(&app, "POST", &uri, body, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let proposal = &result["proposal"];
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(proposal),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    complete_scope(&app, &base, &task).await;
    usage::settle(&path, &project, id);
    let mut changed = start_request(proposal);
    changed["targetChapters"] = json!(99);
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            changed,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    let (status, run) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs"),
        start_request(proposal),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["targetChapters"], 100);
    assert_eq!(run["completedChapters"], 0);
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        workspace["chapters"],
        json!([]),
        "approving scope must not create placeholder chapters"
    );
}

#[tokio::test]
async fn semantic_resume_preserves_original_scope_and_rejects_foreign_or_stale_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("semantic-resume.sqlite3");
    let (app, base, project, task) = setup(&path).await;
    let proposal = propose(&app, &base, &task).await;
    let id = task["id"].as_str().unwrap();
    complete_scope(&app, &base, &task).await;
    usage::settle(&path, &project, id);
    let (status, run) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs"),
        start_request(&proposal),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_id = run["id"].as_str().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status,decision)=call(&app,"POST",&format!("{base}/tasks"),json!({"operation":"advisory","threadId":"original-thread","inputRevision":0,"target":{"type":"project"},"authorIntent":"照此前安排接着写，不另开任务。","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{decision}");
    let decision_id = decision["id"].as_str().unwrap();
    let status_route = format!("{base}/tasks/{decision_id}/work-decision?threadId=original-thread&expectedRevision=0");
    let (_, running_context) = call(&app, "GET", &status_route, Value::Null, CANDIDATE).await;
    assert_eq!(running_context["conversationRuns"][0]["resumeSelection"]["status"], "notNeeded");
    let (code, rejected) = call(&app, "POST", &format!("{base}/tasks/{decision_id}/generation-handoff"),
        json!({"taskId":decision_id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,
            "targetChapters":2,"instruction":"按原范围续写","idempotencyKey":"already-running",
            "resumeRun":{"id":run_id,"expectedVersion":run["version"]}}), CANDIDATE).await;
    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["details"], json!({"origin":"storyRuntime","phase":"domainValidation",
        "reason":"generation_resume_not_needed","instancePath":"/resumeRun","nextAction":"read_work_status",
        "runId":run_id,"runStatus":"running","workStatus":"running","resumeSelection":{"status":"notNeeded"}}));
    let (_, unchanged) = call(&app, "GET", &status_route, Value::Null, CANDIDATE).await;
    assert_eq!(unchanged, running_context, "rejected resume must not register a handoff or mutate state");
    let (status, paused) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs/{run_id}/control"),
        json!({"expectedVersion":run["version"],"command":"pause"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    save_plan(
        &app,
        &base,
        &decision,
        0,
        json!([continuation_goal(&paused)]),
    )
    .await;
    let (status, context) = call(
        &app,
        "GET",
        &format!(
            "{base}/tasks/{decision_id}/work-decision?threadId=original-thread&expectedRevision=0"
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["conversationRuns"][0]["id"], run_id);
    assert_eq!(context["conversationRuns"][0]["resumeSelection"]["status"], "candidate");
    let route = format!("{base}/tasks/{decision_id}/generation-handoff");
    let body = json!({"taskId":decision_id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,"targetChapters":2,"instruction":"按原范围续写","idempotencyKey":"resume","resumeRun":{"id":run_id,"expectedVersion":paused["version"]}});
    let mut stale = body.clone();
    stale["resumeRun"]["expectedVersion"] = json!(999);
    assert!(
        !call(&app, "POST", &route, stale, CANDIDATE)
            .await
            .0
            .is_success()
    );
    let mut expanded = body.clone();
    expanded["targetChapters"] = json!(3);
    assert!(
        !call(&app, "POST", &route, expanded, CANDIDATE)
            .await
            .0
            .is_success()
    );
    let mut foreign = body.clone();
    foreign["resumeRun"]["id"] = json!("generation-foreign");
    assert!(
        !call(&app, "POST", &route, foreign, CANDIDATE)
            .await
            .0
            .is_success()
    );
    let (status, selected) = call(&app, "POST", &route, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    assert_eq!(
        call(&app, "POST", &route, body, CANDIDATE).await,
        (status, selected.clone())
    );
    let (_, runs) = call(
        &app,
        "GET",
        &format!("{base}/generation-runs"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(runs["runs"].as_array().unwrap().len(), 1);
    assert_eq!(runs["runs"][0]["providerBudget"], paused["providerBudget"]);
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/generation-runs"),
            start_request(&selected["proposal"]),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
}

#[tokio::test]
async fn post_edit_decision_can_select_original_run_only_after_prerequisites_settle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("post-edit-resume.sqlite3");
    let (app, base, project, initial) = setup(&path).await;
    let proposal = propose(&app, &base, &initial).await;
    let initial_id = initial["id"].as_str().unwrap();
    complete_scope(&app, &base, &initial).await;
    usage::settle(&path, &project, initial_id);
    let (_, run) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs"),
        start_request(&proposal),
        AUTHOR,
    )
    .await;
    let run_id = run["id"].as_str().unwrap();
    let (_, paused) = call(
        &app,
        "POST",
        &format!("{base}/generation-runs/{run_id}/control"),
        json!({"expectedVersion":run["version"],"command":"pause"}),
        AUTHOR,
    )
    .await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let task_body = |operation: &str, target: Value, revision: Value| json!({"operation":operation,"threadId":"original-thread","inputRevision":revision,"target":target,"authorIntent":"先校正简报，再恢复原两章任务，不新增范围或预算。","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600});
    let (code, root) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        task_body("advisory", json!({"type":"project"}), json!(0)),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{root}");
    let root_id = root["id"].as_str().unwrap();
    let brief_goal = deliverable_goal("brief", "先校正并保存创作简报");
    let mut future_resume = continuation_goal(&paused);
    future_resume["status"] = json!("pending");
    future_resume["dependsOn"] = json!(["brief"]);
    save_plan(&app, &base, &root, 0, json!([brief_goal, future_resume])).await;
    let (code,action)=call(&app,"POST",&format!("{base}/tasks/{root_id}/actions"),json!({"taskId":root_id,"threadId":"original-thread","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"brief-first","operation":"generateBrief","target":{"type":"artifact"},"instruction":"校正故事定位，保存候选简报"}),CANDIDATE).await;
    assert_eq!(code, StatusCode::CREATED, "{action}");
    let resume = |id: &str, revision: Value, key: &str| json!({"taskId":id,"threadId":"original-thread","expectedRevision":revision,"expectedIntentRevision":0,"targetChapters":2,"instruction":"校正保存后按原范围继续","idempotencyKey":key,"resumeRun":{"id":run_id,"expectedVersion":paused["version"]}});
    assert!(
        !call(
            &app,
            "POST",
            &format!("{base}/tasks/{root_id}/generation-handoff"),
            resume(root_id, json!(0), "premature"),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/tasks/{root_id}/complete"),
            json!({"threadId":"original-thread","summary":"已登记简报校正，待保存验收"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    usage::settle(&path, &project, root_id);
    let action_id = action["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/tasks/{root_id}/actions/{action_id}/approve"),
            json!({"expectedIntentRevision":0}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (code, child) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        task_body(
            "generateBrief",
            json!({"type":"artifact","requestedActionId":action_id,"requestedByTaskId":root_id}),
            json!(0),
        ),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{child}");
    let child_id = child["id"].as_str().unwrap();
    let (code,saved)=call(&app,"POST",&format!("{base}/artifacts/brief"),json!({"taskId":child_id,"threadId":"original-thread","expectedRevision":0,"expectedCandidateRevisionId":child["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":"corrected","payload":{"premise":"东方幻想悬疑，先揭开本城案件，不提前解决终局"}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::CREATED, "{saved}");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/tasks/{child_id}/complete"),
            json!({"threadId":"original-thread","summary":"简报校正已保存"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    usage::settle(&path, &project, child_id);
    let (_, work) = call(
        &app,
        "GET",
        &format!("{base}/tasks/{root_id}/work-task"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (code,decision)=call(&app,"POST",&format!("{base}/work-tasks/{}/decision",work["id"].as_str().unwrap()),json!({"expectedWorkVersion":work["version"],"expectedRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}),AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{decision}");
    let mut target = decision["step"]["target"].clone();
    target["resumeStepId"] = decision["step"]["id"].clone();
    let (code, post) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        task_body("advisory", target, workspace["revision"].clone()),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{post}");
    let post_id = post["id"].as_str().unwrap();
    let mut reviewed_brief = deliverable_goal("brief", "先校正并保存创作简报");
    reviewed_brief["status"] = json!("satisfied");
    reviewed_brief["assessment"] = json!("已读回本 Work 的真实候选简报保存回执");
    reviewed_brief["evidence"] = json!([{"taskId":child_id,"revisionId":saved["revisionId"]}]);
    let mut ready_resume = continuation_goal(&paused);
    ready_resume["dependsOn"] = json!(["brief"]);
    save_plan(&app, &base, &post, 1, json!([reviewed_brief, ready_resume])).await;
    let status_uri = format!(
        "{base}/tasks/{post_id}/work-decision?threadId=original-thread&expectedRevision={}&view=conversationRuns",
        workspace["revision"]
    );
    let (code, choices) = call(&app, "GET", &status_uri, Value::Null, CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{choices}");
    assert_eq!(choices["conversationRuns"][0]["id"], run_id);
    assert!(
        !call(
            &app,
            "GET",
            &format!("{status_uri}&cursor=invalid"),
            Value::Null,
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_work_steps_v1 SET status='queued' WHERE current_attempt_id=?1",
        [child_id],
    )
    .unwrap();
    let uri = format!("{base}/tasks/{post_id}/generation-handoff");
    assert!(
        !call(
            &app,
            "POST",
            &uri,
            resume(post_id, workspace["revision"].clone(), "unsettled"),
            CANDIDATE
        )
        .await
        .0
        .is_success()
    );
    db.execute(
        "UPDATE story_work_steps_v1 SET status='completed' WHERE current_attempt_id=?1",
        [child_id],
    )
    .unwrap();
    let (code, selected) = call(
        &app,
        "POST",
        &uri,
        resume(post_id, workspace["revision"].clone(), "settled"),
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{selected}");
    let (_, runs) = call(
        &app,
        "GET",
        &format!("{base}/generation-runs"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(runs["runs"].as_array().unwrap().len(), 1);
    assert_eq!(runs["runs"][0]["providerBudget"], paused["providerBudget"]);
}

async fn paused_original(
    app: &Router,
    base: &str,
    project: &str,
    path: &std::path::Path,
    initial: &Value,
) -> Value {
    let selected = propose(app, base, initial).await;
    let id = initial["id"].as_str().unwrap();
    complete_scope(app, base, initial).await;
    usage::settle(path, project, id);
    let (code, run) = call(
        app,
        "POST",
        &format!("{base}/generation-runs"),
        start_request(&selected),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{run}");
    let (code, paused) = call(
        app,
        "POST",
        &format!(
            "{base}/generation-runs/{}/control",
            run["id"].as_str().unwrap()
        ),
        json!({"expectedVersion":run["version"],"command":"pause"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{paused}");
    paused
}
async fn resume_decision(app: &Router, base: &str, run: &Value, revision: i64) -> (Value, Value) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (code,task)=call(app,"POST",&format!("{base}/tasks"),json!({"operation":"advisory","threadId":"original-thread","inputRevision":revision,"target":{"type":"project"},"authorIntent":"前置工作已核对，接回原范围，保持预算和作者意图。","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+3600}),AUTHOR).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let goal = continuation_goal(run);
    save_plan(app, base, &task, 0, json!([goal])).await;
    let (code,selected)=call(app,"POST",&format!("{base}/tasks/{id}/generation-handoff"),json!({"taskId":id,"threadId":"original-thread","expectedRevision":revision,"expectedIntentRevision":0,"targetChapters":run["targetChapters"],"instruction":"接回原执行范围","idempotencyKey":"durable-resume","resumeRun":{"id":run["id"],"expectedVersion":run["version"]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{selected}");
    let mut fulfilled_goal = continuation_goal(run);
    fulfilled_goal["status"] = json!("satisfied");
    fulfilled_goal["assessment"] = json!("原范围交接已实际登记；尚未执行未来章节");
    fulfilled_goal["continuation"]["proposalId"] = selected["proposal"]["proposalId"].clone();
    save_plan(app, base, &task, 1, json!([fulfilled_goal])).await;
    (task, selected["proposal"].clone())
}
async fn complete_resume(app: &Router, base: &str, task: &Value, status: &str) {
    let id = task["id"].as_str().unwrap();
    let revision = task["inputRevision"].as_i64().unwrap();
    if status == "partial" {
        let (code, plan) = call(app, "GET", &format!(
            "{base}/tasks/{id}/work-plan?threadId=original-thread&expectedRevision={revision}&goalId=resume"
        ), Value::Null, CANDIDATE).await;
        assert_eq!(code, StatusCode::OK, "{plan}");
        let mut blocked_check = deliverable_goal("prerequisite-check", "核验前置正文");
        blocked_check["kind"] = json!("check");
        blocked_check["status"] = json!("blocked");
        blocked_check["assessment"] =
            json!("前置正文的完整核验尚未完成，不能声明通过或自动恢复原任务");
        save_plan(
            app,
            base,
            task,
            plan["version"].as_i64().unwrap(),
            json!([plan["goal"], blocked_check]),
        )
        .await;
    }
    let (code, context) = call(
        app,
        "GET",
        &format!(
            "{base}/tasks/{id}/work-decision?threadId=original-thread&expectedRevision={revision}"
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{context}");
    let (code,result)=call(app,"POST",&format!("{base}/tasks/{id}/complete"),json!({"threadId":"original-thread","summary":"前置工作核对及交接登记结束，长篇正文尚待继续执行。","deliveryReview":{"expectedWorkVersion":context["version"],"expectedSourceDigest":context["sourceDigest"],"goalStatus":status,"goalAssessment":"本次检查及接续决定已完成，不代表长篇范围已完成。","checkedScope":"检查当前前置状态和原任务身份","unresolved":if status=="partial"{json!([{"issue":{"issueId":"unpassed-prerequisite","newIssue":{"summary":"前置检查尚未通过"},"resolutionOf":[]},"observationId":null,"reason":"前置检查尚未通过"}])}else{json!([])},"issueResolutions":[],"authorDecisions":if status=="needsAuthorDecision"{json!(["请作者决定检查范围"])}else{json!([])},"evidence":[]}}),CANDIDATE).await;
    assert_eq!(code, StatusCode::OK, "{result}");
}

#[tokio::test]
async fn durable_resume_is_atomic_recoverable_and_does_not_override_later_author_pause() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("durable-resume.sqlite3");
    let (app, base, project, initial) = setup(&path).await;
    let original = paused_original(&app, &base, &project, &path, &initial).await;
    let (task, proposal) = resume_decision(&app, &base, &original, 0).await;
    let id = task["id"].as_str().unwrap();
    let endpoint = format!("{base}/tasks/{id}/generation-handoff/execute");
    let body = json!({"proposalId":proposal["proposalId"]});
    complete_resume(&app, &base, &task, "fulfilled").await;
    // A completed model turn is insufficient until its provider receipt closes.
    assert!(
        !call(&app, "POST", &endpoint, body.clone(), AUTHOR)
            .await
            .0
            .is_success()
    );
    usage::settle(&path, &project, id);
    let (code, first) = call(&app, "POST", &endpoint, body.clone(), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{first}");
    assert_eq!(first["shouldExecute"], true);
    assert_eq!(
        first["run"]["version"],
        original["version"].as_i64().unwrap() + 1
    );
    assert_eq!(first["run"]["providerBudget"], original["providerBudget"]);
    assert_eq!(first["run"]["intentRevisions"], original["intentRevisions"]);
    assert_eq!(first["run"]["targetChapters"], original["targetChapters"]);
    let work_id = format!("work-{id}");
    let (_, work) = call(
        &app,
        "GET",
        &format!("{base}/work-tasks/{work_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["status"], "completed");
    assert_eq!(work["pendingContinuation"], true);
    // Rebuild the runtime after the Native commit but before any Host dispatch.
    let restarted = router(fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (code, saved) = call(
        &restarted,
        "GET",
        &format!("{base}/work-tasks/{work_id}/generation-handoff"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{saved}");
    assert_eq!(saved["handoff"]["proposal"], proposal);
    let (code, repeated) = call(&restarted, "POST", &endpoint, body.clone(), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{repeated}");
    assert_eq!(repeated, first);
    assert!(
        !call(&restarted, "POST", &endpoint, body.clone(), CANDIDATE)
            .await
            .0
            .is_success()
    );
    let (code, paused) = call(
        &restarted,
        "POST",
        &format!(
            "{base}/generation-runs/{}/control",
            original["id"].as_str().unwrap()
        ),
        json!({"expectedVersion":first["run"]["version"],"command":"pause"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{paused}");
    let (code, later) = call(&restarted, "POST", &endpoint, body, AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{later}");
    assert_eq!(later["shouldExecute"], false);
    assert_eq!(later["run"], paused);
    let (_, work) = call(
        &restarted,
        "GET",
        &format!("{base}/work-tasks/{work_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["pendingContinuation"], false);
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM story_generation_handoff_executions_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn partial_or_author_pending_delivery_cannot_automatically_resume() {
    for status in ["partial", "needsAuthorDecision"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.sqlite3");
        let (app, base, project, initial) = setup(&path).await;
        let original = paused_original(&app, &base, &project, &path, &initial).await;
        let (task, proposal) = resume_decision(&app, &base, &original, 0).await;
        complete_resume(&app, &base, &task, status).await;
        let id = task["id"].as_str().unwrap();
        usage::settle(&path, &project, id);
        let (code, rejected) = call(
            &app,
            "POST",
            &format!("{base}/tasks/{id}/generation-handoff/execute"),
            json!({"proposalId":proposal["proposalId"]}),
            AUTHOR,
        )
        .await;
        assert!(!code.is_success());
        assert_eq!(
            rejected["error"]["code"],
            "generation_handoff_prerequisites_incomplete"
        );
        let (_, work) = call(
            &app,
            "GET",
            &format!("{base}/work-tasks/work-{id}"),
            Value::Null,
            AUTHOR,
        )
        .await;
        assert_eq!(work["pendingContinuation"], false);
        let (_, current) = call(
            &app,
            "GET",
            &format!(
                "{base}/generation-runs/{}",
                original["id"].as_str().unwrap()
            ),
            Value::Null,
            AUTHOR,
        )
        .await;
        assert_eq!(current, original);
    }
}

#[tokio::test]
async fn atomic_rebind_receipt_survives_exit_without_reopening_scope_or_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rebind-resume.sqlite3");
    let (app, base, project, initial) = setup(&path).await;
    let original = paused_original(&app, &base, &project, &path, &initial).await;
    let (code,changed)=call(&app,"POST",&format!("{base}/artifacts/brief"),json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"prerequisite-edit","payload":{"premise":"修订后的前置资料，仍按原两章范围继续"}}),AUTHOR).await;
    assert!(code.is_success(), "{changed}");
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (task, proposal) = resume_decision(
        &app,
        &base,
        &original,
        workspace["revision"].as_i64().unwrap(),
    )
    .await;
    complete_resume(&app, &base, &task, "fulfilled").await;
    let id = task["id"].as_str().unwrap();
    usage::settle(&path, &project, id);
    let endpoint = format!("{base}/tasks/{id}/generation-handoff/execute");
    let body = json!({"proposalId":proposal["proposalId"]});
    let (code, first) = call(&app, "POST", &endpoint, body.clone(), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{first}");
    assert_eq!(first["run"]["sourceDigest"], workspace["sourceDigest"]);
    assert_eq!(
        first["run"]["intentGeneration"],
        original["intentGeneration"]
    );
    assert_eq!(first["run"]["intentRevisions"], original["intentRevisions"]);
    assert_eq!(first["run"]["providerBudget"], original["providerBudget"]);
    assert_eq!(first["run"]["replans"].as_array().unwrap().len(), 1);
    let restarted = router(fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    assert_eq!(
        call(&restarted, "POST", &endpoint, body, AUTHOR).await,
        (StatusCode::OK, first)
    );
}

#[tokio::test]
async fn first_consumption_requires_exact_run_version_and_current_story_source() {
    for changed_run in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cas.sqlite3");
        let (app, base, project, initial) = setup(&path).await;
        let original = paused_original(&app, &base, &project, &path, &initial).await;
        let (task, proposal) = resume_decision(&app, &base, &original, 0).await;
        complete_resume(&app, &base, &task, "fulfilled").await;
        let id = task["id"].as_str().unwrap();
        usage::settle(&path, &project, id);
        if changed_run {
            let (_, running) = call(
                &app,
                "POST",
                &format!(
                    "{base}/generation-runs/{}/control",
                    original["id"].as_str().unwrap()
                ),
                json!({"expectedVersion":original["version"],"command":"resume"}),
                AUTHOR,
            )
            .await;
            let (code, value) = call(
                &app,
                "POST",
                &format!(
                    "{base}/generation-runs/{}/control",
                    original["id"].as_str().unwrap()
                ),
                json!({"expectedVersion":running["version"],"command":"pause"}),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{value}");
        } else {
            let (code,value)=call(&app,"POST",&format!("{base}/artifacts/brief"),json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"new-source","payload":{"premise":"交接之后作者改动资料，需要重新检查"}}),AUTHOR).await;
            assert!(code.is_success(), "{value}");
        }
        let (code, rejected) = call(
            &app,
            "POST",
            &format!("{base}/tasks/{id}/generation-handoff/execute"),
            json!({"proposalId":proposal["proposalId"]}),
            AUTHOR,
        )
        .await;
        assert!(!code.is_success());
        assert_eq!(
            rejected["error"]["code"],
            if changed_run {
                "generation_handoff_run_changed"
            } else {
                "generation_handoff_source_changed"
            }
        );
        let db = rusqlite::Connection::open(path).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_generation_handoff_executions_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn a_consumption_receipt_does_not_authorize_changed_author_intent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("changed-intent.sqlite3");
    let (app, base, project, initial) = setup(&path).await;
    let original = paused_original(&app, &base, &project, &path, &initial).await;
    let (task, proposal) = resume_decision(&app, &base, &original, 0).await;
    complete_resume(&app, &base, &task, "fulfilled").await;
    let id = task["id"].as_str().unwrap();
    usage::settle(&path, &project, id);
    let endpoint = format!("{base}/tasks/{id}/generation-handoff/execute");
    let body = json!({"proposalId":proposal["proposalId"]});
    let (code, first) = call(&app, "POST", &endpoint, body.clone(), AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{first}");
    let (code,changed)=call(&app,"POST",&format!("{base}/generation-runs/{}/future-intents",original["id"].as_str().unwrap()),json!({"expectedVersion":first["run"]["version"],"instruction":"作者改变下一章目标，应按新指令重新选择接续。"}),AUTHOR).await;
    assert_eq!(code, StatusCode::OK, "{changed}");
    assert!(
        !call(&app, "POST", &endpoint, body, AUTHOR)
            .await
            .0
            .is_success()
    );
    let (_, work) = call(
        &app,
        "GET",
        &format!("{base}/work-tasks/work-{id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(work["pendingContinuation"], false);
}
