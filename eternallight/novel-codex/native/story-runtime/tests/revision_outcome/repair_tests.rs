use super::{decision_tests::*, fixture::*, flow::Flow};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const REPAIR_INTENT: &str = "保留藏信，让她向守卫坦白，并补上守卫沉默的动机。";
const REPAIRED_BODY: &str = "她将密函藏进衣襟。她向守卫坦白密函的存在。";

fn source(flow: &Flow, receipt: &Value, decision: &Value) -> Value {
    json!({"writerTaskId":flow.writer["id"],"assessmentRevisionId":receipt["revisionId"],
        "decisionId":decision["id"]})
}

async fn repair_decision(flow: &Flow, receipt: &Value) -> Value {
    let (status, decision) = decide(
        flow,
        decision_request(receipt, "repair", Value::Null, "repairDraft"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{decision}");
    decision
}

async fn root_request(f: &Fixture, source: Value, key: &str) -> Value {
    let (status, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    let mut request = f.task_request(
        "advisory",
        json!({"type":"project","revisionRepairSource":source}),
    );
    request["authorIntent"] = json!(REPAIR_INTENT);
    request["inputRevision"] = workspace["revision"].clone();
    request["initialBudget"] = json!({"authorizationId":format!("repair-budget-{key}"),
        "authorMessageId":format!("repair-author-{key}"),
        "expectedBaseSnapshotId":workspace["headSnapshotId"],
        "expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":16,"maxTokens":200_000,"deadlineAt":request["capabilityExpiresAt"]});
    request
}

async fn root(flow: &Flow, receipt: &Value, decision: &Value) -> Value {
    let request = root_request(&flow.fixture, source(flow, receipt, decision), "root").await;
    let (status, task) = flow
        .fixture
        .request("POST", "/tasks", AUTHOR, request)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    task
}

fn repair_target(flow: &Flow) -> Value {
    let mut target = flow.fixture.target.clone();
    target["scope"]["sourceRevisionId"] = flow.draft["revisionId"].clone();
    target
}

async fn propose(f: &Fixture, root: &Value, target: Value, key: &str) -> (StatusCode, Value) {
    f.request(
        "POST",
        &format!("/tasks/{}/actions", root["id"].as_str().unwrap()),
        CANDIDATE,
        json!({"taskId":root["id"],"threadId":root["threadId"],"expectedRevision":0,
            "expectedIntentRevision":0,"idempotencyKey":key,"operation":"generateChapter",
            "target":target,"instruction":"按照当前作者目标修复已选场景"}),
    )
    .await
}

fn action_path(root: &Value, action: &Value) -> String {
    format!(
        "/tasks/{}/actions/{}",
        root["id"].as_str().unwrap(),
        action["id"].as_str().unwrap()
    )
}

async fn approve(f: &Fixture, root: &Value, action: &Value) -> (StatusCode, Value) {
    f.request(
        "POST",
        &format!("{}/approve", action_path(root, action)),
        AUTHOR,
        json!({"expectedIntentRevision":0}),
    )
    .await
}

fn claim_target(root: &Value, action: &Value) -> Value {
    let mut target = action["target"].clone();
    target["requestedByTaskId"] = root["id"].clone();
    target["requestedActionId"] = action["id"].clone();
    target
}

async fn context(f: &Fixture, task: &Value, byte_budget: usize) -> (StatusCode, Value) {
    let position = if task["purpose"] == "writer" {
        format!(
            "&readingChapter={}&sceneOrder={}",
            task["target"]["readingChapter"], task["target"]["sceneOrder"]
        )
    } else {
        String::new()
    };
    let path = format!(
        "/context?taskId={}&threadId={}&projectRevision=0&purpose={}&budget=24&byteBudget={byte_budget}{position}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["purpose"].as_str().unwrap()
    );
    let (status, context) = f.request("GET", &path, CANDIDATE, Value::Null).await;
    if status == StatusCode::OK {
        assert_eq!(
            context["budgetsUsed"]["byteSize"],
            context.to_string().len()
        );
        assert!(context.to_string().len() <= byte_budget);
        let mut content = context.clone();
        content.as_object_mut().unwrap().remove("id");
        content.as_object_mut().unwrap().remove("contentHash");
        let hash = format!("{:x}", Sha256::digest(content.to_string().as_bytes()));
        assert_eq!(
            (context["id"].clone(), context["contentHash"].clone()),
            (json!(format!("context-{hash}")), json!(hash))
        );
    }
    (status, context)
}

#[tokio::test]
async fn repair_root_freezes_analysis_once_without_changing_authority_or_author_intent() {
    let (flow, receipt) = completed_outcome().await;
    let decision = repair_decision(&flow, &receipt).await;
    let f = &flow.fixture;
    let work = f.work().await;
    let watermarks = f.repo().project_source_watermarks(&f.project()).unwrap();
    let task = root(&flow, &receipt, &decision).await;
    let frozen = task["inputManifest"]["revisionRepairInput"].clone();
    let assessment = flow.report()["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| json!({"id":item["id"],"status":item["status"],"reason":item["reason"]}))
        .collect::<Vec<_>>();
    assert_eq!(
        frozen,
        json!({"authority":"candidateAnalysis",
        "source":source(&flow, &receipt, &decision),"authorDecision":decision,
        "chapterId":"chapter-one","chapterNumber":1,"selectedGroupIds":["letter","witness"],
        "desiredOutcomes":desired_outcomes(),"assessment":assessment})
    );
    let repository = f.repo();
    let stored = repository
        .task(&f.project(), task["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        repository.revision_repair_input(&stored).unwrap(),
        Some(frozen.clone())
    );
    enum Forgery {
        Target,
        Manifest,
        Thread,
        Project,
    }
    for field in [
        Forgery::Target,
        Forgery::Manifest,
        Forgery::Thread,
        Forgery::Project,
    ] {
        let mut forged = stored.clone();
        match field {
            Forgery::Target => forged.target_json = json!({"type":"project"}).to_string(),
            Forgery::Manifest => {
                forged.input_manifest_json = json!({"authorIntent":"fake"}).to_string()
            }
            Forgery::Thread => forged.thread_id = "forged-thread".into(),
            Forgery::Project => {
                forged.project_id =
                    novel_codex_story_core::ProjectId::parse("other-project").unwrap()
            }
        }
        assert_eq!(
            repository.revision_repair_input(&forged),
            Err(novel_codex_story_core::RepositoryError::TaskBindingInvalid)
        );
    }
    assert!(frozen.to_string().len() <= 8_000);
    assert!(!frozen.to_string().contains("\"quote\""));
    assert_eq!(task["inputManifest"]["authorIntent"], REPAIR_INTENT);
    assert_eq!(f.work().await, work);
    assert_eq!(
        f.repo().project_source_watermarks(&f.project()).unwrap(),
        watermarks
    );
    assert_ne!(
        f.grant(&task).await["workTaskId"],
        flow.grants[0]["workTaskId"]
    );
    let ledger_before = f.ledger_counts();
    let (status, pack) = context(f, &task, 60_000).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(
        read_context_source(f, &task, &pack, "/operationContext/revisionRepairInput").await,
        frozen
    );
    assert_eq!(
        pack["activeIntentRefs"],
        json!([{"source":"authorTask","taskId":task["id"],"text":REPAIR_INTENT}])
    );
    assert!(pack.get("inputManifest").is_none());
    assert_eq!(
        pack.to_string().matches("\"revisionRepairInput\"").count(),
        1
    );
    // Initial projection registers frozen sources; subsequent reads are pure.
    assert_eq!(f.ledger_counts(), ledger_before);
    assert_eq!(
        f.repo().project_source_watermarks(&f.project()).unwrap(),
        watermarks
    );
    let before = database_snapshot(f);
    let (status, tight) = context(f, &task, 4_000).await;
    if status == StatusCode::OK {
        assert_eq!(
            read_context_source(f, &task, &tight, "/operationContext/revisionRepairInput").await,
            frozen
        );
    } else {
        assert_eq!(
            (status, tight["error"]["code"].clone()),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!("context_budget_exceeded")
            )
        );
    }
    assert_eq!(database_snapshot(f), before);
}

#[tokio::test]
async fn invalid_repair_roots_roll_back_author_messages_work_and_root_budgets() {
    let (flow, receipt) = completed_outcome().await;
    let decision = repair_decision(&flow, &receipt).await;
    let f = &flow.fixture;
    let valid = root_request(f, source(&flow, &receipt, &decision), "invalid").await;
    let before = database_snapshot(f);
    for (pointer, value) in [
        (
            "/target/revisionRepairSource/writerTaskId",
            json!("missing-writer"),
        ),
        (
            "/target/revisionRepairSource/assessmentRevisionId",
            flow.draft["revisionId"].clone(),
        ),
        (
            "/target/revisionRepairSource/decisionId",
            json!("missing-decision"),
        ),
        (
            "/target/revisionRepairSource",
            json!({"writerTaskId":flow.writer["id"],
            "assessmentRevisionId":receipt["revisionId"],"decisionId":decision["id"],"unknown":true}),
        ),
        ("/target/type", json!("chapter")),
        ("/operation", json!("generateBrief")),
    ] {
        let mut request = valid.clone();
        *request.pointer_mut(pointer).unwrap() = value;
        let (status, error) = f.request("POST", "/tasks", AUTHOR, request).await;
        assert!(status.is_client_error(), "{pointer}: {error}");
        assert_eq!(database_snapshot(f), before);
    }
    let mut spoof = valid.clone();
    spoof["revisionRepairInput"] =
        json!({"authority":"author","source":valid["target"]["revisionRepairSource"]});
    assert!(
        f.request("POST", "/tasks", AUTHOR, spoof)
            .await
            .0
            .is_client_error()
    );
    assert_eq!(database_snapshot(f), before);
    let mut spoof = valid.clone();
    spoof["target"]["revisionRepairInput"] = json!({"authority":"author"});
    assert!(
        f.request("POST", "/tasks", AUTHOR, spoof)
            .await
            .0
            .is_client_error()
    );
    assert_eq!(database_snapshot(f), before);
    let mut unfunded = valid.clone();
    unfunded.as_object_mut().unwrap().remove("initialBudget");
    assert!(
        f.request("POST", "/tasks", AUTHOR, unfunded)
            .await
            .0
            .is_client_error()
    );
    assert_eq!(database_snapshot(f), before);
    assert!(
        f.request("POST", "/tasks", CANDIDATE, valid.clone())
            .await
            .0
            .is_client_error()
    );
    assert_eq!(database_snapshot(f), before);
    let (status, changed) = decide(
        &flow,
        decision_request(
            &receipt,
            "accepted-instead",
            decision["id"].clone(),
            "acceptRevision",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{changed}");
    let before = database_snapshot(f);
    for decision_id in [decision["id"].clone(), changed["id"].clone()] {
        let mut request = valid.clone();
        request["target"]["revisionRepairSource"]["decisionId"] = decision_id;
        let (status, error) = f.request("POST", "/tasks", AUTHOR, request).await;
        assert!(status.is_client_error(), "{error}");
        assert_eq!(database_snapshot(f), before);
    }
}

#[tokio::test]
async fn repair_input_is_complete_under_context_pressure_and_oversize_creation_rolls_back() {
    for reason_chars in [500, 1000] {
        let flow = Flow::queued().await;
        let f = &flow.fixture;
        let task = flow.assessment().await;
        let mut report = flow.report();
        for item in report["outcomes"].as_array_mut().unwrap() {
            item["reason"] = json!("月".repeat(reason_chars));
        }
        let (status, receipt) = f
            .request(
                "POST",
                &flow.path(),
                CANDIDATE,
                flow.submission(&task, report, "bounded-repair"),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{receipt}");
        let (status, completed) = f.complete(&task).await;
        assert_eq!(status, StatusCode::OK, "{completed}");
        f.settle(&task);
        let decision = repair_decision(&flow, &receipt).await;
        let request = root_request(f, source(&flow, &receipt, &decision), "bounded").await;
        let before = database_snapshot(f);
        let (status, root) = f.request("POST", "/tasks", AUTHOR, request).await;
        if reason_chars == 1000 {
            assert!(status.is_client_error(), "{root}");
            assert_eq!(database_snapshot(f), before);
            continue;
        }
        assert_eq!(status, StatusCode::CREATED, "{root}");
        let frozen = root["inputManifest"]["revisionRepairInput"].clone();
        assert!(frozen.to_string().len() <= 8_000);
        assert_eq!(
            frozen["assessment"],
            json!([
                {"id":"hide-letter","status":"supported","reason":"月".repeat(500)},
                {"id":"confess","status":"unmet","reason":"月".repeat(500)},
                {"id":"guard-motive","status":"insufficientEvidence","reason":"月".repeat(500)}
            ])
        );
        let (status, roomy) = context(f, &root, 60_000).await;
        assert_eq!(status, StatusCode::OK, "{roomy}");
        let initial =
            read_context_source(f, &root, &roomy, "/operationContext/revisionRepairInput").await;
        assert_eq!(initial, frozen);
        // The full Unicode analysis exceeds one page; it remains complete even
        // when the initial context is limited to references and author intent.
        assert!(frozen.to_string().len() > 4000);
        let cached = database_snapshot(f);
        let (status, insufficient) = context(f, &root, 4_000).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{insufficient}");
        assert_eq!(insufficient["error"]["code"], "context_budget_exceeded");
        assert_eq!(database_snapshot(f), cached);
        let reference_budget = roomy.to_string().len() + 64;
        assert!(reference_budget < roomy.to_string().len() + frozen.to_string().len());
        let (status, tight) = context(f, &root, reference_budget).await;
        assert_eq!(status, StatusCode::OK, "{tight}");
        assert_eq!(
            tight["operationContext"]["revisionRepairInput"],
            roomy["operationContext"]["revisionRepairInput"]
        );
        assert_eq!(
            read_context_source(f, &root, &tight, "/operationContext/revisionRepairInput").await,
            frozen
        );
        assert_eq!(tight["coverage"]["complete"], false);
    }
}

#[tokio::test]
async fn stale_decision_or_draft_rejects_root_proposal_approval_and_writer_claim() {
    for change in ["decision", "draft", "knowledge"] {
        let (flow, receipt) = completed_outcome().await;
        let decision = repair_decision(&flow, &receipt).await;
        let f = &flow.fixture;
        let task = root(&flow, &receipt, &decision).await;
        let target = repair_target(&flow);
        let (status, action) = propose(f, &task, target.clone(), "before-change").await;
        assert_eq!(status, StatusCode::CREATED, "{action}");
        let (status, queued) = propose(f, &task, target.clone(), "queued-before-change").await;
        assert_eq!(status, StatusCode::CREATED, "{queued}");
        let (status, approved) = approve(f, &task, &queued).await;
        assert_eq!(status, StatusCode::OK, "{approved}");
        if change == "decision" {
            let (status, changed) = decide(
                &flow,
                decision_request(
                    &receipt,
                    "new-repair",
                    decision["id"].clone(),
                    "repairDraft",
                ),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{changed}");
        } else if change == "draft" {
            let mut next = f.original.clone();
            next["sceneGroups"][1]["body"] = json!("她将密函交给守卫。");
            join_body(&mut next);
            let (status, changed) = f.checkpoint(None, &flow.draft, next, "stale-repair").await;
            assert_eq!(status, StatusCode::CREATED, "{changed}");
        } else {
            remodel(&flow).await;
        }
        let request = root_request(f, source(&flow, &receipt, &decision), "stale").await;
        let before = database_snapshot(f);
        let (status, error) = f.request("POST", "/tasks", AUTHOR, request).await;
        assert!(status.is_client_error(), "{change}: {error}");
        assert_eq!(database_snapshot(f), before);
        let (status, error) = propose(f, &task, target, "after-change").await;
        assert!(status.is_client_error(), "{change}: {error}");
        assert_eq!(database_snapshot(f), before);
        let (status, error) = approve(f, &task, &action).await;
        assert!(status.is_client_error(), "{change}: {error}");
        assert_eq!(database_snapshot(f), before);
        let (status, error) = f
            .create_task("generateChapter", claim_target(&task, &queued))
            .await;
        assert!(status.is_client_error(), "{change}: {error}");
        assert_eq!(database_snapshot(f), before);
        let (status, error) = context(f, &task, 60_000).await;
        assert!(status.is_client_error(), "{change}: {error}");
        assert_eq!(database_snapshot(f), before);
    }
}

#[tokio::test]
async fn approved_repair_writer_models_and_assesses_with_frozen_history_in_one_new_work() {
    let (flow, receipt) = completed_outcome().await;
    let decision = repair_decision(&flow, &receipt).await;
    let f = &flow.fixture;
    let task = root(&flow, &receipt, &decision).await;
    let frozen = task["inputManifest"]["revisionRepairInput"].clone();
    let root_grant = f.grant(&task).await;
    let target = repair_target(&flow);
    let before = database_snapshot(f);
    for (pointer, value) in [
        ("/chapterId", json!("other-chapter")),
        ("/scope/sourceRevisionId", f.original["revisionId"].clone()),
        ("/scope/groupIds", json!(["opening"])),
        ("/desiredOutcomes", json!([])),
    ] {
        let mut invalid = target.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        let (status, error) = propose(f, &task, invalid, "invalid-target").await;
        assert!(status.is_client_error(), "{pointer}: {error}");
        assert_eq!(database_snapshot(f), before);
    }
    let (status, action) = propose(f, &task, target.clone(), "repair-writer").await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let path = action_path(&task, &action);
    assert_eq!(
        f.request("GET", &format!("{path}/permission"), AUTHOR, Value::Null)
            .await,
        (StatusCode::OK, json!({"permission":null}))
    );
    let before = database_snapshot(f);
    let (status, error) = f
        .create_task("generateChapter", claim_target(&task, &action))
        .await;
    assert!(status.is_client_error(), "Unapproved repair: {error}");
    assert_eq!(database_snapshot(f), before);
    assert!(
        f.request(
            "POST",
            &format!("{path}/approve"),
            CANDIDATE,
            json!({"expectedIntentRevision":0})
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(database_snapshot(f), before);
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&task);
    let (status, approved) = approve(f, &task, &action).await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let (status, permission) = f
        .request("GET", &format!("{path}/permission"), AUTHOR, Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK, "{permission}");
    assert_eq!(permission["permission"]["kind"], "exactAction");
    assert_eq!(permission["permission"]["evidence"]["target"], target);
    let (status, writer) = f
        .create_task("generateChapter", claim_target(&task, &action))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{writer}");
    assert_eq!(
        f.grant(&writer).await["workTaskId"],
        root_grant["workTaskId"]
    );
    assert_eq!(writer["inputManifest"]["authorIntent"], REPAIR_INTENT);
    assert_eq!(
        writer["inputManifest"]["authorIntentBinding"]["sourceTaskId"],
        task["id"]
    );
    let (status, pack) = context(f, &writer, 60_000).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(
        read_context_source(f, &writer, &pack, "/operationContext/revisionRepairInput").await,
        frozen
    );
    let mut output = f.original.clone();
    output["sceneGroups"][1]["body"] = json!(REPAIRED_BODY);
    output["sceneGroups"][2]["body"] = json!("守卫为了保护林夕而沉默。");
    join_body(&mut output);
    let (status, draft) = f
        .checkpoint(Some(&writer), &flow.draft, output, "repaired-draft")
        .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, pack) = context(f, &writer, 60_000).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Own draft must not invalidate repair: {pack}"
    );
    assert_eq!(
        read_context_source(f, &writer, &pack, "/operationContext/revisionRepairInput").await,
        frozen
    );
    let (status, completed) = f.complete(&writer).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&writer);
    // The old report is now historical. Only the registered decision in this
    // repair Work may continue; copying its shape or crossing Works grants nothing.
    let (code, work) = f
        .request(
            "GET",
            &format!("/tasks/{}/work-task", writer["id"].as_str().unwrap()),
            AUTHOR,
            Value::Null,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{work}");
    let (code, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    assert_eq!(code, StatusCode::OK, "{workspace}");
    let (code, continuation) = f.request("POST", &format!("/work-tasks/{}/decision", work["id"].as_str().unwrap()), AUTHOR,
        json!({"expectedWorkVersion":work["version"],"expectedRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]})).await;
    assert_eq!(code, StatusCode::OK, "{continuation}");
    let queued = &continuation["step"];
    let mut decision_target = queued["target"].clone();
    decision_target["resumeStepId"] = queued["id"].clone();
    let before_claim = database_snapshot(f);
    for (pointer, value) in [
        (
            "/workDecision/workTaskId",
            flow.grants[0]["workTaskId"].clone(),
        ),
        ("/workDecision/originTaskId", f.discussion["id"].clone()),
        ("/resumeStepId", json!("unregistered-step")),
    ] {
        let mut forged = decision_target.clone();
        *forged.pointer_mut(pointer).unwrap() = value;
        let mut request = f.task_request("advisory", forged);
        request["threadId"] = work["threadId"].clone();
        request["inputRevision"] = queued["inputRevision"].clone();
        let (code, error) = f.request("POST", "/tasks", AUTHOR, request).await;
        assert!(code.is_client_error(), "{pointer}: {error}");
        assert_eq!(database_snapshot(f), before_claim);
    }
    let selected = f.choose(&writer, "modelKnowledge").await;
    let mut target = selected["target"].clone();
    target["resumeStepId"] = selected["id"].clone();
    let (status, model) = f.create_task("modelKnowledge", target).await;
    assert_eq!(status, StatusCode::CREATED, "{model}");
    let batch = f
        .repo()
        .modeling_batch(&f.project(), model["target"]["batchId"].as_str().unwrap())
        .unwrap()
        .as_json();
    assert_eq!(
        f.grant(&model).await["workTaskId"],
        root_grant["workTaskId"]
    );
    let (status, pack) = context(f, &model, 60_000).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(
        read_context_source(f, &model, &pack, "/operationContext/revisionRepairInput").await,
        frozen
    );
    let (status, published) = f
        .request(
            "POST",
            &format!(
                "/modeling-batches/{}/publish",
                batch["id"].as_str().unwrap()
            ),
            CANDIDATE,
            json!({"taskId":model["id"],"threadId":model["threadId"],"expectedRevision":0,
            "entities":[],"assertions":[]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (status, completed) = f.complete(&model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&model);
    f.choose(&model, "assessRevisionOutcome").await;
    let (status, work) = f
        .request(
            "GET",
            &format!("/tasks/{}/work-task", task["id"].as_str().unwrap()),
            AUTHOR,
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{work}");
    let queued = step(&work, "assessRevisionOutcome");
    let mut target = queued["target"].clone();
    target["resumeStepId"] = queued["id"].clone();
    let (status, assessment) = f.create_task("assessRevisionOutcome", target).await;
    assert_eq!(status, StatusCode::CREATED, "{assessment}");
    assert_eq!(
        f.grant(&assessment).await["workTaskId"],
        root_grant["workTaskId"]
    );
    let (status, pack) = context(f, &assessment, 60_000).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(
        read_context_source(
            f,
            &assessment,
            &pack,
            "/operationContext/revisionRepairInput"
        )
        .await,
        frozen
    );
    let outcome_input = read_context_source(
        f,
        &assessment,
        &pack,
        "/operationContext/revisionOutcomeInput",
    )
    .await;
    assert_eq!(outcome_input["desiredOutcomes"], desired_outcomes());
    assert_eq!(outcome_input["beforeScenes"][0]["body"], NEW_BODY);
    let mut report = flow.report();
    report["writerTaskId"] = writer["id"].clone();
    report["originalRevisionId"] = flow.draft["revisionId"].clone();
    report["draftRevisionId"] = draft["revisionId"].clone();
    report["knowledgeBatchId"] = batch["id"].clone();
    report["outcomes"] = json!([
        {"id":"hide-letter","status":"insufficientEvidence","reason":"沿用已支持的藏信，没有新增藏信证据。","evidence":[]},
        {"id":"confess","status":"supported","reason":"本次明确坦白。","evidence":[super::flow::evidence("letter", REPAIRED_BODY, "她向守卫坦白密函的存在。")]},
        {"id":"guard-motive","status":"supported","reason":"本次交代保护林夕的动机。","evidence":[super::flow::evidence("witness", "守卫为了保护林夕而沉默。", "守卫为了保护林夕而沉默。")]}
    ]);
    let (status, outcome) = f
        .request(
            "POST",
            &format!("/revision-outcomes/{}", writer["id"].as_str().unwrap()),
            CANDIDATE,
            flow.submission(&assessment, report.clone(), "repair-outcome"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{outcome}");
    let (status, completed) = f.complete(&assessment).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&assessment);
    let (_, current) = f
        .request(
            "GET",
            &format!("/revision-outcomes/{}", writer["id"].as_str().unwrap()),
            AUTHOR,
            Value::Null,
        )
        .await;
    assert_eq!(
        current,
        json!({"revisionId":outcome["revisionId"],"authority":"candidateAnalysis",
        "state":"current","assessment":report,"decisions":[]})
    );
    let (_, history) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(
        history,
        json!({"revisionId":receipt["revisionId"],"authority":"candidateAnalysis",
        "state":"stale","assessment":flow.report(),"decisions":[decision]})
    );
}

#[tokio::test]
async fn ordinary_context_has_no_repair_input() {
    let f = Fixture::new().await;
    let (status, pack) = context(&f, &f.discussion, 60_000).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert!(
        pack["operationContext"]
            .get("revisionRepairInput")
            .is_none()
    );
    assert!(
        f.discussion["inputManifest"]
            .get("revisionRepairInput")
            .is_none()
    );
}

#[tokio::test]
async fn paused_repair_resumes_the_frozen_source_and_account_but_cannot_reactivate_stale_authority()
{
    for source_changed in [false, true] {
        let (flow, receipt) = completed_outcome().await;
        let decision = repair_decision(&flow, &receipt).await;
        let f = &flow.fixture;
        let task = root(&flow, &receipt, &decision).await;
        let grant = f.grant(&task).await;
        let work_id = grant["workTaskId"].as_str().unwrap();
        let mut repository = f.repo();
        let work = repository.work_task(&f.project(), work_id).unwrap();
        let paused = repository
            .control_work_task(&f.project(), work_id, "pause", work.version, /*now*/ 1)
            .unwrap();
        repository
            .update_task(
                &f.project(),
                task["id"].as_str().unwrap(),
                task["threadId"].as_str().unwrap(),
                novel_codex_story_core::TaskUpdate {
                    status: "failed".into(),
                    error: Some("paused by author".into()),
                    result_summary: None,
                    now: 2,
                },
            )
            .unwrap();
        f.settle(&task);
        let latest = repository.work_task(&f.project(), &paused.id).unwrap();
        assert_eq!(latest.status, "paused");
        if source_changed {
            let (status, changed) = decide(
                &flow,
                decision_request(
                    &receipt,
                    "defer-after-pause",
                    decision["id"].clone(),
                    "defer",
                ),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{changed}");
        }
        let before = database_snapshot(f);
        let resumed = repository.control_work_task(
            &f.project(),
            work_id,
            "resume",
            latest.version,
            /*now*/ 3,
        );
        if source_changed {
            assert_eq!(
                resumed,
                Err(novel_codex_story_core::RepositoryError::TaskInputStale)
            );
            assert_eq!(database_snapshot(f), before);
            continue;
        }
        let resumed = resumed.unwrap();
        let queued = resumed
            .steps
            .iter()
            .find(|step| step.operation == "advisory")
            .unwrap();
        let mut target = queued.target.clone();
        target["resumeStepId"] = json!(queued.id);
        let (status, next) = f.create_task("advisory", target).await;
        assert_eq!(status, StatusCode::CREATED, "{next}");
        assert_eq!(next["inputManifest"]["authorIntent"], REPAIR_INTENT);
        assert_eq!(
            next["inputManifest"]["revisionRepairInput"],
            task["inputManifest"]["revisionRepairInput"]
        );
        assert_eq!(
            next["inputManifest"]["authorIntentBinding"]["sourceTaskId"],
            task["id"]
        );
        assert_eq!(f.grant(&next).await["workTaskId"], grant["workTaskId"]);
        assert_eq!(f.grant(&next).await["deadlineAt"], grant["deadlineAt"]);
    }
}
