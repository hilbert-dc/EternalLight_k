//! Actual authenticated report transport and immutable Native recovery proofs.
#[path = "revision_outcome/fixture.rs"]
mod fixture;
#[path = "revision_outcome/flow.rs"]
mod flow;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;
#[path = "support/author_task_fixture.rs"]
mod task_fixture;

use axum::{body::Body, http::{Request, StatusCode}};
use http_body_util::BodyExt;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use fixture::{AUTHOR, CANDIDATE};
use flow::Flow;

const CAP: &str = "outcome-capability-123456789012345678";
const TURN: &str = "report-proof-turn";
const TOOL: &str = "submit_revision_outcome";

fn original_arguments(flow: &Flow, body: &Value) -> Value {
    let mut arguments = body.clone();
    arguments["projectId"] = json!(flow.fixture.project().as_str());
    arguments["writerTaskId"] = flow.writer["id"].clone();
    arguments
}

fn request_hash(flow: &Flow, body: &Value) -> String {
    format!("{:x}", Sha256::digest(json!({"tool":TOOL,"arguments":original_arguments(flow, body)}).to_string().as_bytes()))
}

async fn bound_submission(
    flow: &Flow, task: &Value, body: Value, call: &str, caller: &str, capability: &str,
) -> (StatusCode, Value) {
    bound_submission_at(flow, task, body, call, caller, capability, TOOL, &flow.fixture.root).await
}

async fn bound_submission_at(
    flow: &Flow, task: &Value, body: Value, call: &str, caller: &str, capability: &str,
    tool: &str, root: &str,
) -> (StatusCode, Value) {
    let request = Request::builder().method("POST")
        .uri(format!("{root}{}", flow.path()))
        .header("authorization", format!("Bearer {CANDIDATE}"))
        .header("content-type", "application/json")
        .header("x-novel-task-capability", capability)
        .header("x-novel-tool-name", tool)
        .header("x-novel-root-thread", task["threadId"].as_str().unwrap())
        .header("x-novel-caller-thread", caller)
        .header("x-novel-turn", TURN)
        .header("x-novel-tool-call", call)
        .header("x-novel-request-hash", request_hash(flow, &body))
        .body(Body::from(body.to_string())).unwrap();
    let response = flow.fixture.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn exact_operation(flow: &Flow, task: &Value, body: &Value, call: &str) -> Value {
    let hash = request_hash(flow, body);
    let thread = task["threadId"].as_str().unwrap();
    let path = format!("{}/tasks/recovery-operations?taskId={}&threadId={thread}&turnId={TURN}&callId={call}&toolName={TOOL}&requestHash={hash}", flow.fixture.root, task["id"].as_str().unwrap());
    let request = Request::builder().uri(path)
        .header("authorization", format!("Bearer {CANDIDATE}"))
        .header("x-novel-task-capability", CAP)
        .header("x-novel-tool-name", TOOL)
        .header("x-novel-root-thread", thread)
        .header("x-novel-caller-thread", thread)
        .header("x-novel-turn", TURN)
        .header("x-novel-tool-call", call)
        .header("x-novel-request-hash", hash)
        .body(Body::empty()).unwrap();
    let response = flow.fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn domain_state(flow: &Flow, task: &Value) -> Value {
    let db = flow.fixture.db();
    let persisted_task: (String, String, String, String, Option<String>, Option<i64>) = db.query_row(
        "SELECT status,stage,delivery_status,output_refs_json,error,capability_revoked_at FROM story_tasks_v2 WHERE task_id=?1",
        [task["id"].as_str().unwrap()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)),
    ).unwrap();
    let counts = ["story_work_issues_v1", "story_issue_observations_v1", "story_issue_resolution_refs_v1"]
        .map(|table| db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_,i64>(0)).unwrap());
    let mut budgets = db.prepare("SELECT work_task_id,ledger_json FROM story_work_budgets_v1 ORDER BY work_task_id").unwrap();
    let budgets = budgets.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap()
        .collect::<Result<Vec<_>, _>>().unwrap();
    let heads = flow.fixture.repo().object_heads(&flow.fixture.project()).unwrap().into_iter()
        .map(|h| json!({"objectId":h.object_id.as_str(),"kind":h.object_kind,"locked":h.locked,
            "candidateRevisionId":h.candidate_revision_id.as_ref().map(|r|r.as_str()),
            "acceptedRevisionId":h.accepted_revision_id.as_ref().map(|r|r.as_str())})).collect::<Vec<_>>();
    json!({"counts":flow.fixture.ledger_counts(),"issues":counts,"task":persisted_task,"budgets":budgets,"heads":heads})
}

async fn prepared() -> (Flow, Value) {
    let flow = Flow::unmodeled().await;
    let task = flow.assessment().await;
    flow.fixture.repo().bind_task_turn(&flow.fixture.project(), task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap(), TURN).unwrap();
    (flow, task)
}

async fn prepared_author_source_report() -> (Flow, Value, Value, Value, Value) {
    let flow = Flow::modeled(flow::Goals::Absent).await;
    let (status, completed) = flow.fixture.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    flow.fixture.settle(&flow.model);
    flow.fixture.choose(&flow.model, "assessRevisionOutcome").await;
    let task = flow.assessment().await;
    flow.fixture.repo().bind_task_turn(&flow.fixture.project(), task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap(), TURN).unwrap();
    let (status, context) = flow.fixture.request("GET", &format!(
        "/context?taskId={}&threadId={}&projectRevision={}&purpose=critic",
        task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap(), task["inputRevision"]),
        CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let source = |path: &str| context["taskSources"]["entries"].as_array().unwrap().iter()
        .find(|entry| entry["sectionPath"] == path).unwrap().clone();
    let author = source("/authorRequirementsSource");
    let interpretation = source("/operationContext/revisionOutcomeInput");
    let mut report = flow.report();
    // All frozen whole-chapter criteria are supported, measured length is
    // satisfied, and no selected-scene criteria are invented or removed.
    report["outcomes"] = json!([report["outcomes"][3].clone()]);
    (flow, task, report, author, interpretation)
}

fn original_reconciliation_request(flow: &Flow, task: &Value, body: &Value, call: &str) -> Value {
    json!({"taskId":task["id"],"threadId":task["threadId"],"turnId":TURN,"callId":call,
        "toolName":TOOL,"requestHash":request_hash(flow, body),"arguments":original_arguments(flow, body)})
}

#[tokio::test]
async fn terminal_report_reconciliation_proves_each_original_bad_cas_without_domain_changes() {
    use novel_codex_story_core::{RecoveryOperationBinding, TaskUpdate};

    let (flow, task, report, _, _) = prepared_author_source_report().await;
    let f = &flow.fixture;
    let mut originals = Vec::new();
    for (index, wrong) in [json!("null"), json!("other-valid-candidate-id")].into_iter().enumerate() {
        let mut body = flow.submission(&task, report.clone(), &format!("original-report-{index}"));
        body["expectedCandidateRevisionId"] = wrong;
        let call = format!("original-report-call-{index}");
        let mut repo = f.repo();
        repo.bind_recovery_operation(RecoveryOperationBinding {
            project_id:f.project(),task_id:task["id"].as_str().unwrap().into(),thread_id:task["threadId"].as_str().unwrap().into(),
            turn_id:TURN.into(),call_id:call.clone(),tool_name:TOOL.into(),request_hash:request_hash(&flow, &body),
            idempotency_ref:Some(format!("sha256:{:x}", Sha256::digest(body["idempotencyKey"].to_string().as_bytes()))),now:1,
        }).unwrap();
        // Real Native admission and report preparation succeeded before a
        // simulated process interruption; no candidate save was observed.
        let persisted = repo.task(&f.project(), task["id"].as_str().unwrap()).unwrap().unwrap();
        assert!(repo.prepare_revision_outcome_payload_with_recovery(&persisted, &report).is_ok());
        assert_eq!(exact_operation(&flow, &task, &body, &call).await["operation"]["state"], "accepted");
        originals.push((body, call));
    }
    f.repo().update_task(&f.project(), task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap(), TaskUpdate {
        status:"failed".into(),error:Some("interrupted report delivery".into()),result_summary:None,now:2,
    }).unwrap();
    let before = domain_state(&flow, &task);
    let path = "/tasks/recovery-operations/reconcile";
    let request = original_reconciliation_request(&flow, &task, &originals[0].0, &originals[0].1);
    let (status, _) = f.request("POST", path, CANDIDATE, request.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(domain_state(&flow, &task), before);

    // A failure persisting the proof cannot be turned into a known rejection.
    f.db().execute_batch("CREATE TRIGGER fail_reconcile_proof BEFORE UPDATE ON story_tool_operations_v1 WHEN NEW.state='rejected' BEGIN SELECT RAISE(ABORT,'synthetic reconciliation persistence failure'); END;").unwrap();
    let (status, error) = f.request("POST", path, AUTHOR, request.clone()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    assert_eq!(exact_operation(&flow, &task, &originals[0].0, &originals[0].1).await["operation"]["state"], "accepted");
    assert_eq!(domain_state(&flow, &task), before);
    f.db().execute_batch("DROP TRIGGER fail_reconcile_proof;").unwrap();

    for (body, call) in originals {
        let request = original_reconciliation_request(&flow, &task, &body, &call);
        let (status, proof) = f.request("POST", path, AUTHOR, request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{proof}");
        assert_eq!(proof["operation"]["state"], "rejected");
        assert_eq!(proof["operation"]["tool_call_id"], call);
        assert_eq!(proof["operation"]["request_hash"], request_hash(&flow, &body));
        assert!(proof["operation"]["receipt_ref"].is_null());
        assert!(!proof.to_string().contains(fixture::INTENT));
        assert!(!proof.to_string().contains(fixture::NEW_BODY));
        assert_eq!(exact_operation(&flow, &task, &body, &call).await, proof);
        assert_eq!(f.request("POST", path, AUTHOR, request).await, (StatusCode::OK, proof));
        assert_eq!(domain_state(&flow, &task), before);
    }
    assert!(f.repo().revision_outcome(&f.project(), flow.writer["id"].as_str().unwrap()).unwrap().is_none());
    assert!(f.repo().authorize_task_capability(&f.project(), task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap(), 0,
        &format!("{:x}", Sha256::digest(CAP.as_bytes())), 3).is_err(), "Factual reconciliation grants no execution authority");
}

#[tokio::test]
async fn author_source_rejection_is_private_and_same_task_can_correct_without_passing_omissions() {
    let (flow, task, mut report, author, interpretation) = prepared_author_source_report().await;
    let f = &flow.fixture;
    // Another real Author Work in the same project/database, created through
    // public authorization. Its admitted source must not become report authority.
    let (status, workspace) = f.request("GET", "/creative-workspace", fixture::AUTHOR, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let private_foreign_author = "PRIVATE_OTHER_WORK_AUTHOR_INPUT";
    let (status, foreign_task) = f.request("POST", "/tasks", fixture::AUTHOR, json!({
        "operation":"advisory","target":{"type":"project"},"inputRevision":0,
        "threadId":"foreign-author-work","authorIntent":private_foreign_author,
        "capabilityHash":format!("{:x}", Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+900,
        "initialBudget":{"authorizationId":"foreign-author-budget","authorMessageId":"foreign-author-message",
            "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
            "maxRequests":16,"maxTokens":200_000,"deadlineAt":now+900}})).await;
    assert_eq!(status, StatusCode::CREATED, "{foreign_task}");
    f.read_author_requirements(&foreign_task).await;
    let (status, foreign_context) = f.request("GET", &format!(
        "/context?taskId={}&threadId={}&projectRevision=0&purpose=planner",
        foreign_task["id"].as_str().unwrap(), foreign_task["threadId"].as_str().unwrap()), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{foreign_context}");
    let foreign_source = foreign_context["taskSources"]["entries"].as_array().unwrap().iter()
        .find(|entry| entry["sectionPath"] == "/authorRequirementsSource").unwrap().clone();
    report["uncoveredRequirements"] = json!([{
        "issue":{"issueId":"author-requirement-uncovered","newIssue":{"summary":"作者原要求仍需补齐"},"resolutionOf":[]},
        "requirement":"核对尚未落实的作者原要求。","reason":"独立报告记录遗漏，不宣布完成。",
        "authorSourceChecks":[{"sourceId":author["sourceId"],"contentHash":author["contentHash"],"quote":fixture::INTENT}]
    }]);
    let before = domain_state(&flow, &task);
    let thread = task["threadId"].as_str().unwrap();
    let mut previous_error = None;
    let mut rejections = Vec::new();
    for (index, source) in [interpretation, json!({"sourceId":"unknown-source","contentHash":"a".repeat(64)}), foreign_source.clone()]
        .into_iter().enumerate() {
        let mut invalid = report.clone();
        invalid["uncoveredRequirements"][0]["authorSourceChecks"][0]["sourceId"] = source["sourceId"].clone();
        invalid["uncoveredRequirements"][0]["authorSourceChecks"][0]["contentHash"] = source["contentHash"].clone();
        let body = flow.submission(&task, invalid, &format!("bad-author-source-{index}"));
        let call = format!("bad-author-source-call-{index}");
        let (status, error) = bound_submission(&flow, &task, body.clone(), &call, thread, CAP).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["error"]["code"], "invalid_output_payload");
        let detail = &error["error"]["details"];
        assert_eq!(detail["reason"], "author_requirement_source_not_admitted");
        assert_eq!(detail["instancePath"], "/uncoveredRequirements");
        assert_eq!(detail["relativeInstancePath"], "/sourceId");
        assert_eq!(detail["requirementIndex"], 0);
        assert_eq!(detail["authorSourceCheckIndex"], 0);
        assert_eq!(detail["expectedSectionPath"], "/authorRequirementsSource");
        assert!(error.to_string().len() < 4096);
        assert!(!error.to_string().contains(private_foreign_author));
        assert!(!error.to_string().contains(foreign_source["sourceId"].as_str().unwrap()));
        assert!(!error.to_string().contains(fixture::INTENT));
        if let Some(previous) = &previous_error { assert_eq!(&error, previous, "Source existence or other Work ownership must not be disclosed"); }
        previous_error = Some(error);
        assert_eq!(domain_state(&flow, &task), before, "The report, issues, task, manuscript and budgets remain unchanged");
        let proof = exact_operation(&flow, &task, &body, &call).await;
        assert_eq!(proof["operation"]["state"], "rejected");
        assert_eq!(proof["operation"]["run_id"], task["id"]);
        assert_eq!(proof["operation"]["tool_call_id"], call);
        assert_eq!(proof["operation"]["request_hash"], request_hash(&flow, &body));
        assert!(proof["operation"]["receipt_ref"].is_null());
        rejections.push((body, call, proof));
    }
    let corrected = flow.submission(&task, report.clone(), "correct-author-source");
    let (status, saved) = bound_submission(&flow, &task, corrected.clone(), "correct-author-source-call", thread, CAP).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let proof = exact_operation(&flow, &task, &corrected, "correct-author-source-call").await;
    assert_eq!(proof["operation"]["state"], "committed");
    assert_eq!(proof["operation"]["receipt_ref"], format!("runtime:candidate:{}", saved["revisionId"].as_str().unwrap()));
    for (body, call, proof) in rejections { assert_eq!(exact_operation(&flow, &task, &body, &call).await, proof); }
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["id"], task["id"]);
    let stored = f.repo().revision_outcome(&f.project(), flow.writer["id"].as_str().unwrap()).unwrap().unwrap();
    assert_eq!(stored.content, report);
    f.settle(&task);
    let (_, decision) = f.decision(&task).await;
    let assessment = &decision["requirements"].as_array().unwrap().iter()
        .find(|requirement| requirement["kind"] == "revisionOutcome").unwrap()["assessment"];
    assert_eq!(assessment["length"]["satisfied"], true);
    assert_eq!(assessment["checkCounts"], json!({"total":1,"supported":1,"unmet":0,"insufficientEvidence":0}));
    assert_eq!(assessment["uncoveredRequirementCount"], 1);
    assert_eq!(assessment["verdict"], "notMet");
    assert_eq!(decision["canFinish"], false);
    assert_ne!(f.work().await["status"], "completed");
}

#[tokio::test]
async fn empty_author_omissions_keep_the_supported_report_passing() {
    let (flow, task, report, _, _) = prepared_author_source_report().await;
    assert_eq!(report["uncoveredRequirements"], json!([]));
    let body = flow.submission(&task, report.clone(), "no-author-omissions");
    let (status, saved) = bound_submission(&flow, &task, body.clone(), "no-author-omissions-call", task["threadId"].as_str().unwrap(), CAP).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(exact_operation(&flow, &task, &body, "no-author-omissions-call").await["operation"]["state"], "committed");
    let (status, completed) = flow.fixture.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let stored = flow.fixture.repo().revision_outcome(&flow.fixture.project(), flow.writer["id"].as_str().unwrap()).unwrap().unwrap();
    assert_eq!(stored.content["uncoveredRequirements"], json!([]));
    flow.fixture.settle(&task);
    let (_, decision) = flow.fixture.decision(&task).await;
    let assessment = &decision["requirements"].as_array().unwrap().iter()
        .find(|requirement| requirement["kind"] == "revisionOutcome").unwrap()["assessment"];
    assert_eq!(assessment["length"]["satisfied"], true);
    assert_eq!(assessment["checkCounts"], json!({"total":1,"supported":1,"unmet":0,"insufficientEvidence":0}));
    assert!(assessment.get("uncoveredRequirementCount").is_none());
    assert_eq!(assessment["verdict"], "passed");
}

#[tokio::test]
async fn candidate_expectation_rejection_is_exact_and_same_report_task_can_correct() {
    let (flow, task, report, _, _) = prepared_author_source_report().await;
    let thread = task["threadId"].as_str().unwrap();
    let object = task["inputManifest"]["outputObjectId"].clone();
    assert!(task["inputManifest"]["expectedCandidateRevisionId"].is_null());
    let before = domain_state(&flow, &task);
    let mut rejections = Vec::new();
    let mut previous_error = None;
    // A string spelling null, an arbitrary valid ID, and the real input draft
    // revision are all wrong CAS values for this separate report object.
    for (index, expected) in [json!("null"), json!("unrelated-candidate-revision"), flow.draft["revisionId"].clone()]
        .into_iter().enumerate() {
        let mut body = flow.submission(&task, report.clone(), &format!("wrong-report-cas-{index}"));
        body["expectedCandidateRevisionId"] = expected;
        let call = format!("wrong-report-cas-call-{index}");
        let (status, error) = bound_submission(&flow, &task, body.clone(), &call, thread, CAP).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["error"]["code"], "invalid_output_payload");
        let details = &error["error"]["details"];
        assert_eq!(details["reason"], "candidate_expectation_mismatch");
        assert_eq!(details["instancePath"], "/expectedCandidateRevisionId");
        assert_eq!(details["outputObjectId"], object);
        assert!(details["expectedCandidateRevisionId"].is_null());
        assert!(error.to_string().len() < 4096);
        assert!(!error.to_string().contains(fixture::INTENT));
        assert!(!error.to_string().contains(fixture::NEW_BODY));
        if let Some(previous) = &previous_error { assert_eq!(&error, previous); }
        previous_error = Some(error);
        assert_eq!(domain_state(&flow, &task), before);
        let proof = exact_operation(&flow, &task, &body, &call).await;
        assert_eq!(proof["operation"]["state"], "rejected");
        assert_eq!(proof["operation"]["run_id"], task["id"]);
        assert_eq!(proof["operation"]["tool_call_id"], call);
        assert_eq!(proof["operation"]["request_hash"], request_hash(&flow, &body));
        assert!(proof["operation"]["receipt_ref"].is_null());
        rejections.push((body, call, proof));
    }
    let corrected = flow.submission(&task, report.clone(), "correct-report-cas");
    let (status, saved) = bound_submission(&flow, &task, corrected.clone(), "correct-report-cas-call", thread, CAP).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(exact_operation(&flow, &task, &corrected, "correct-report-cas-call").await["operation"]["state"], "committed");

    // The same task can revise its own report, but absence is no longer the
    // expected head. Only this output's last successful receipt supplies CAS.
    let before = domain_state(&flow, &task);
    let mut next = flow.submission(&task, report.clone(), "missing-existing-report-cas");
    let (status, error) = bound_submission(&flow, &task, next.clone(), "missing-existing-report-cas-call", thread, CAP).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["error"]["details"]["reason"], "candidate_expectation_mismatch");
    assert_eq!(error["error"]["details"]["expectedCandidateRevisionId"], saved["revisionId"]);
    assert_eq!(domain_state(&flow, &task), before);
    let proof = exact_operation(&flow, &task, &next, "missing-existing-report-cas-call").await;
    assert_eq!(proof["operation"]["state"], "rejected");
    rejections.push((next.clone(), "missing-existing-report-cas-call".into(), proof));
    next["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    next["idempotencyKey"] = json!("correct-existing-report-cas");
    let (status, revised) = bound_submission(&flow, &task, next.clone(), "correct-existing-report-cas-call", thread, CAP).await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let proof = exact_operation(&flow, &task, &next, "correct-existing-report-cas-call").await;
    assert_eq!(proof["operation"]["state"], "committed");
    assert_eq!(proof["operation"]["receipt_ref"], format!("runtime:candidate:{}", revised["revisionId"].as_str().unwrap()));
    for (body, call, proof) in rejections { assert_eq!(exact_operation(&flow, &task, &body, &call).await, proof); }
    assert_eq!(flow.fixture.repo().revision_outcome(&flow.fixture.project(), flow.writer["id"].as_str().unwrap()).unwrap().unwrap().content, report);

    let before = domain_state(&flow, &task);
    for (name, caller, tool, root, foreign_task) in [
        ("foreign-thread", "foreign-root", TOOL, flow.fixture.root.as_str(), false),
        ("wrong-tool", thread, "save_brief", flow.fixture.root.as_str(), false),
        ("wrong-task", thread, TOOL, flow.fixture.root.as_str(), true),
        ("foreign-project", thread, TOOL, "/api/projects/other-project", false),
    ] {
        let mut body = flow.submission(&task, report.clone(), name);
        if foreign_task { body["taskId"] = json!("unknown-report-task"); }
        let (status, error) = bound_submission_at(&flow, &task, body.clone(), name, caller, CAP, tool, root).await;
        assert!(status.is_client_error(), "{name}: {error}");
        assert!(!error.to_string().contains(object.as_str().unwrap()));
        assert!(!error.to_string().contains(revised["revisionId"].as_str().unwrap()));
        assert_ne!(error["error"]["details"]["reason"], "candidate_expectation_mismatch");
        assert!(exact_operation(&flow, &task, &body, name).await["operation"].is_null());
        assert_eq!(domain_state(&flow, &task), before);
    }

    // A real source change is still stale input, even if the caller also sends
    // an incorrect expected head. Field guidance cannot grant fresh inputs.
    let mut changed = flow.fixture.original.clone();
    changed["sceneGroups"][1]["body"] = json!("她把密函交给守卫。");
    fixture::join_body(&mut changed);
    let (status, edited) = flow.fixture.checkpoint(None, &flow.draft, changed, "invalidate-report-source").await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    let before = domain_state(&flow, &task);
    let stale = flow.submission(&task, report, "stale-report-cas");
    let (status, error) = bound_submission(&flow, &task, stale, "stale-report-cas-call", thread, CAP).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(error["error"]["message"], "TaskInputStale");
    assert_ne!(error["error"]["details"]["reason"], "candidate_expectation_mismatch");
    assert_eq!(domain_state(&flow, &task), before);
}

#[tokio::test]
async fn criterion_rejection_has_exact_native_proof_and_same_task_can_correct() {
    let (flow, task) = prepared().await;
    let before = domain_state(&flow, &task);
    let mut report = flow.report();
    let mut extra = report["outcomes"][0].clone();
    extra["id"] = json!("model-added-length-criterion");
    report["outcomes"].as_array_mut().unwrap().push(extra);
    let body = flow.submission(&task, report, "rejected-report");
    let thread = task["threadId"].as_str().unwrap();
    let (status, rejected) = bound_submission(&flow, &task, body.clone(), "invalid-report-call", thread, CAP).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(rejected["error"]["code"], "invalid_output_payload");
    assert_eq!(rejected["error"]["details"]["reason"], "criterion_set_mismatch");
    assert_eq!(domain_state(&flow, &task), before, "Only the recovery record may change");
    let proof = exact_operation(&flow, &task, &body, "invalid-report-call").await;
    let operation = &proof["operation"];
    assert_eq!(operation["state"], "rejected");
    assert!(operation["receipt_ref"].is_null());
    assert!(operation["operation_id"].as_str().unwrap().starts_with("operation-"));
    assert_eq!(operation["run_id"], task["id"]);
    assert_eq!(operation["thread_id"], task["threadId"]);
    assert_eq!(operation["turn_id"], TURN);
    assert_eq!(operation["tool_call_id"], "invalid-report-call");
    assert_eq!(operation["tool_name"], TOOL);
    assert_eq!(operation["request_hash"], request_hash(&flow, &body));
    assert!(!proof.to_string().contains(fixture::NEW_BODY));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    assert!(flow.fixture.repo().authorize_task_capability(&flow.fixture.project(), task["id"].as_str().unwrap(), thread, 0,
        &format!("{:x}",Sha256::digest(CAP.as_bytes())), now).is_ok());
    let (status, replay) = bound_submission(&flow, &task, body.clone(), "invalid-report-call", thread, CAP).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{replay}");
    assert_eq!(domain_state(&flow, &task), before);
    assert_eq!(exact_operation(&flow, &task, &body, "invalid-report-call").await, proof);

    let corrected = flow.submission(&task, flow.report(), "corrected-report");
    let (status, saved) = bound_submission(&flow, &task, corrected.clone(), "correct-report-call", thread, CAP).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let committed = exact_operation(&flow, &task, &corrected, "correct-report-call").await;
    assert_eq!(committed["operation"]["state"], "committed");
    assert_eq!(committed["operation"]["receipt_ref"], format!("runtime:candidate:{}", saved["revisionId"].as_str().unwrap()));
    assert_eq!(exact_operation(&flow, &task, &body, "invalid-report-call").await, proof);
    assert_eq!(flow.fixture.repo().task(&flow.fixture.project(), task["id"].as_str().unwrap()).unwrap().unwrap().status, "running");
}

#[tokio::test]
async fn identity_capability_and_stale_input_reject_before_operation_binding() {
    let (flow, task) = prepared().await;
    let before = domain_state(&flow, &task);
    let body = flow.submission(&task, flow.report(), "guarded-report");
    let thread = task["threadId"].as_str().unwrap();
    let (status, rejected) = bound_submission(&flow, &task, body.clone(), "foreign-call", "foreign-root", CAP).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(rejected["error"]["message"], "TaskBindingInvalid");
    assert!(exact_operation(&flow, &task, &body, "foreign-call").await["operation"].is_null());
    let (status, _) = bound_submission(&flow, &task, body.clone(), "wrong-capability", thread, "wrong-task-capability-123456789012345678").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(exact_operation(&flow, &task, &body, "wrong-capability").await["operation"].is_null());
    let mut stale = body.clone();
    stale["expectedRevision"] = json!(1);
    let (status, rejected) = bound_submission(&flow, &task, stale.clone(), "stale-call", thread, CAP).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{rejected}");
    assert!(exact_operation(&flow, &task, &stale, "stale-call").await["operation"].is_null());
    let mut wrong_writer = body.clone();
    wrong_writer["payload"]["writerTaskId"] = json!("foreign-writer");
    let (status, rejected) = bound_submission(&flow, &task, wrong_writer.clone(), "wrong-writer-call", thread, CAP).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(rejected["error"]["code"], "invalid_writer_task_id");
    assert!(exact_operation(&flow, &task, &wrong_writer, "wrong-writer-call").await["operation"].is_null());
    assert_eq!(domain_state(&flow, &task), before);
}

#[tokio::test]
async fn uncertain_storage_failure_cannot_be_certified_as_prewrite_rejection() {
    let (flow, task) = prepared().await;
    let before = domain_state(&flow, &task);
    flow.fixture.db().execute_batch("CREATE TRIGGER fail_report_save BEFORE INSERT ON story_task_outputs_v2 WHEN NEW.output_kind='revisionOutcome' BEGIN SELECT RAISE(ABORT,'synthetic report output persistence failure'); END;").unwrap();
    let body = flow.submission(&task, flow.report(), "storage-failure");
    let (status, failure) = bound_submission(&flow, &task, body.clone(), "storage-call", task["threadId"].as_str().unwrap(), CAP).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{failure}");
    assert_eq!(domain_state(&flow, &task), before);
    let operation = exact_operation(&flow, &task, &body, "storage-call").await;
    assert_eq!(operation["operation"]["state"], "accepted");
    assert!(operation["operation"]["receipt_ref"].is_null());
    assert_eq!(flow.fixture.request("GET", &flow.path(), AUTHOR, Value::Null).await, (StatusCode::OK, Value::Null));
}
