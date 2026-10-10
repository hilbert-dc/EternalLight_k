//! A recovery's failed assessor is not the producer of the frozen draft.
//! All successful tasks, outputs and charges are created by current public protocols.
use super::{decision_tests::database_snapshot, fixture::*, flow::Flow};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_core::TaskProviderBudget;
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn foreign_authority(f: &Fixture) -> (String, String) {
    async fn request(f: &Fixture, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        let response = f
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {AUTHOR}"))
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
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap(),
        )
    }
    let (code, created) = request(
        f,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel",
        "idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"独立权限范围"}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap().to_owned();
    let root = format!("/api/projects/{project}");
    let (code, workspace) =
        request(f, "GET", &format!("{root}/creative-workspace"), Value::Null).await;
    assert_eq!(code, StatusCode::OK, "{workspace}");
    let mut task_request = f.task_request("advisory", json!({"type":"project"}));
    task_request["threadId"] = json!("foreign-authority");
    task_request["initialBudget"] = json!({"authorizationId":uuid::Uuid::new_v4().to_string(),
        "authorMessageId":uuid::Uuid::new_v4().to_string(),"expectedBaseSnapshotId":workspace["headSnapshotId"],
        "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":16,"maxTokens":200_000,
        "deadlineAt":task_request["capabilityExpiresAt"]});
    let (code, task) = request(f, "POST", &format!("{root}/tasks"), task_request).await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    (project, format!("work-{}", task["id"].as_str().unwrap()))
}

async fn settle_http(f: &Fixture, task: &Value, grant: TaskProviderBudget, requests: u64) -> Value {
    let mut engine = PersistentBudget::new(
        &f.path().with_extension("engine.sqlite3"),
        &grant.task_id,
        json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,
            "maxTokens":grant.max_tokens,"startedAt":1000},"now":1000}),
    )
    .unwrap();
    for index in 0..requests {
        let reservation = engine
            .apply(
                "budget.reserve",
                json!({"parameters":{
            "estimatedInputTokens":20,"maxOutputTokens":50},"now":1001+index*2}),
            )
            .unwrap();
        engine
            .apply(
                "budget.settle",
                json!({"id":reservation,
            "usage":{"total_tokens":125},"now":1002+index*2}),
            )
            .unwrap();
    }
    engine.complete("completed").unwrap();
    let (code, ledger) = f
        .request(
            "POST",
            &format!("/work-tasks/{}/provider-usage", grant.work_task_id),
            AUTHOR,
            json!({"taskId":task["id"]}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{ledger}");
    let charge = ledger["charges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|charge| charge["taskId"] == task["id"])
        .unwrap();
    assert_eq!(charge["budgetId"], grant.budget_id);
    assert_eq!(charge["status"], "completed");
    assert_eq!(charge["requests"], requests);
    assert_eq!(charge["tokens"], requests * 125);
    assert_eq!(charge["unknownTokens"], 0);
    assert_eq!(charge["overrun"], false);
    ledger
}

async fn fail_without_output(flow: &Flow, task: &Value) -> Value {
    let f = &flow.fixture;
    // Capture the real grant while the capability is live; terminal tasks
    // cannot request a new provider-budget grant.
    let grant = serde_json::from_value(f.grant(task).await).unwrap();
    let path = format!("/tasks/{}", task["id"].as_str().unwrap());
    let (code, running) = f
        .request(
            "PATCH",
            &path,
            AUTHOR,
            json!({"threadId":task["threadId"],"status":"running","error":null}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{running}");
    let before = database_snapshot(f);
    let (code, rejected) = f.complete(task).await;
    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["message"], "TaskOutputMissing");
    assert_eq!(database_snapshot(f), before);
    let (code, failed) = f
        .request(
            "PATCH",
            &path,
            AUTHOR,
            json!({"threadId":task["threadId"],"status":"failed","error":"TaskOutputMissing"}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{failed}");
    assert_eq!(failed["outputRefs"], json!([]));
    assert!(
        f.repo()
            .task(&f.project(), task["id"].as_str().unwrap())
            .unwrap()
            .unwrap()
            .capability_revoked_at
            .is_some()
    );
    settle_http(f, task, grant, 1).await;
    failed
}

async fn queue_reassessment(
    flow: &Flow,
    source: &Value,
    plan: Option<Value>,
    expected_failures: u64,
) -> Value {
    let f = &flow.fixture;
    let (parent, status) = f.decision(source).await;
    let action = status["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["operation"] == "assessRevisionOutcome")
        .unwrap();
    assert_eq!(action["recovery"]["failedTaskId"], source["id"]);
    assert_eq!(
        action["recovery"]["sameConditionFailures"],
        expected_failures
    );
    assert_eq!(action["recovery"]["planRequired"], expected_failures >= 2);
    assert_eq!(action["recovery"]["exhausted"], false);
    let path = format!(
        "/tasks/{}/work-decision/actions",
        parent["id"].as_str().unwrap()
    );
    let mut request = json!({"taskId":parent["id"],"threadId":parent["threadId"],
        "expectedRevision":parent["inputRevision"],"expectedWorkVersion":status["version"],
        "expectedSourceDigest":status["sourceDigest"],"actionKey":action["actionKey"],
        "instruction":"重新读取冻结的正文和准则，保存完整评审报告后交还。",
        "idempotencyKey":"recover-assessment"});
    if expected_failures >= 2 {
        for invalid_plan in [
            None,
            Some(json!({"failedTaskId":flow.writer["id"],
            "cause":"报告没有保存","change":"先整理完整报告再提交","verification":"读回报告"})),
        ] {
            let mut invalid = request.clone();
            if let Some(value) = invalid_plan {
                invalid["recoveryPlan"] = value;
            }
            let before = database_snapshot(f);
            let (code, rejected) = f.request("POST", &path, CANDIDATE, invalid).await;
            assert!(code.is_client_error(), "{rejected}");
            assert_eq!(database_snapshot(f), before);
        }
    }
    if let Some(plan) = plan {
        request["recoveryPlan"] = plan;
    }
    let (code, selected) = f.request("POST", &path, CANDIDATE, request).await;
    assert_eq!(code, StatusCode::OK, "{selected}");
    let step = selected["step"].clone();
    let db = f.db();
    let binding: (String, String, String) = db.query_row(
        "SELECT source_task_id,source_revision_id,decision_task_id FROM story_work_decision_actions_v1 WHERE step_id=?1",
        [step["id"].as_str().unwrap()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(binding.0, source["id"].as_str().unwrap());
    assert_eq!(binding.1, flow.draft["revisionId"].as_str().unwrap());
    assert_eq!(binding.2, parent["id"].as_str().unwrap());
    assert_ne!(source["id"], step["target"]["writerTaskId"]);
    assert_eq!(step["target"]["writerTaskId"], flow.writer["id"]);
    assert_eq!(step["target"]["draftRevisionId"], flow.draft["revisionId"]);
    let grant = serde_json::from_value(f.grant(&parent).await).unwrap();
    let (code, completed) = f.complete(&parent).await;
    assert_eq!(code, StatusCode::OK, "{completed}");
    settle_http(f, &parent, grant, 0).await;
    step
}

fn claim_request(f: &Fixture, step: &Value) -> Value {
    let mut target = step["target"].clone();
    target["resumeStepId"] = step["id"].clone();
    let mut request = f.task_request("assessRevisionOutcome", target);
    request["inputRevision"] = step["inputRevision"].clone();
    request
}

async fn claim(f: &Fixture, step: &Value) -> Value {
    let (code, task) = f
        .request("POST", "/tasks", AUTHOR, claim_request(f, step))
        .await;
    assert_eq!(code, StatusCode::CREATED, "{task}");
    f.read_author_requirements(&task).await;
    task
}

fn immutable_authority(f: &Fixture) -> Value {
    let snapshot = database_snapshot(f);
    json!(
        snapshot
            .into_iter()
            .filter(|(table, _)| matches!(
                table.as_str(),
                "story_author_messages_v1" | "story_work_budget_authorizations_v1"
            ))
            .collect::<Vec<_>>()
    )
}

async fn save_read_complete(flow: &Flow, task: &Value, key: &str) -> Value {
    let f = &flow.fixture;
    let (code, pack) = f
        .request(
            "GET",
            &format!(
                "/context?taskId={}&threadId={}&projectRevision=0&purpose=critic",
                task["id"].as_str().unwrap(),
                task["threadId"].as_str().unwrap()
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{pack}");
    let input = read_context_source(f, task, &pack, "/operationContext/revisionOutcomeInput").await;
    assert_eq!(
        input["criterionContract"]["frozenRefs"]["writerTaskId"],
        flow.writer["id"]
    );
    assert_eq!(
        input["criterionContract"]["frozenRefs"]["draftRevisionId"],
        flow.draft["revisionId"]
    );
    let (code, saved) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(task, flow.report(), key),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{saved}");
    let grant = serde_json::from_value(f.grant(task).await).unwrap();
    let (code, completed) = f.complete(task).await;
    assert_eq!(code, StatusCode::OK, "{completed}");
    assert_eq!(completed["status"], "completed");
    let ledger = settle_http(f, task, grant, 1).await;
    assert!(
        ledger["charges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|charge| charge["taskId"] == task["id"] && charge["requests"] == 1)
    );
    let (code, stored) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(code, StatusCode::OK, "{stored}");
    assert_eq!(stored["revisionId"], saved["revisionId"]);
    assert_eq!(stored["assessment"], flow.report());
    assert_eq!(stored["state"], "current");
    saved
}

#[tokio::test]
async fn failed_critic_can_be_selection_source_without_becoming_the_draft_producer() {
    let flow = Flow::unmodeled().await;
    let f = &flow.fixture;
    let work_before = f.work().await;
    let authority = immutable_authority(f);
    let first = flow.assessment().await;
    assert_eq!(
        first["inputManifest"]["workDecisionAction"]["sourceTaskId"],
        flow.writer["id"]
    );
    let failed = fail_without_output(&flow, &first).await;
    let ledger_encoded: String = f
        .db()
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [work_before["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let ledger_before: Value = serde_json::from_str(&ledger_encoded).unwrap();
    let step = queue_reassessment(&flow, &failed, None, 1).await;
    let repaired = claim(f, &step).await;
    assert_eq!(
        repaired["inputManifest"]["workDecisionAction"]["sourceTaskId"],
        first["id"]
    );
    assert_eq!(repaired["target"]["writerTaskId"], flow.writer["id"]);
    assert_eq!(repaired["inputManifest"]["authorIntent"], INTENT);
    let saved = save_read_complete(&flow, &repaired, "recovered-report").await;
    let work_after = f.work().await;
    assert_eq!(work_after["id"], work_before["id"]);
    assert_eq!(immutable_authority(f), authority);
    let ledger_encoded: String = f
        .db()
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [work_before["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let ledger: Value = serde_json::from_str(&ledger_encoded).unwrap();
    assert_eq!(ledger["maxRequests"], 16);
    assert_eq!(ledger["maxTokens"], 200_000);
    assert_eq!(ledger["usedRequests"], 2);
    assert_eq!(ledger["usedTokens"], 250);
    assert_eq!(
        ledger["charges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|charge| charge["taskId"] == first["id"])
            .unwrap()["status"],
        "completed"
    );
    assert!(
        ledger["charges"]
            .as_array()
            .unwrap()
            .starts_with(ledger_before["charges"].as_array().unwrap())
    );
    let mut next = f.original.clone();
    next["sceneGroups"][1]["body"] = json!("她将密函交给守卫。");
    join_body(&mut next);
    let (code, changed) = f.checkpoint(None, &flow.draft, next, "later-head").await;
    assert_eq!(code, StatusCode::CREATED, "{changed}");
    let before = database_snapshot(f);
    let (code, historical) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(code, StatusCode::OK, "{historical}");
    assert_eq!(historical["state"], "stale");
    assert_eq!(historical["revisionId"], saved["revisionId"]);
    assert_eq!(historical["assessment"], flow.report());
    assert_eq!(database_snapshot(f), before);
}

#[tokio::test]
async fn second_failed_assessor_requires_bound_plan_and_recovery_keeps_same_work_allowance() {
    let flow = Flow::unmodeled().await;
    let f = &flow.fixture;
    let authority = immutable_authority(f);
    let first = flow.assessment().await;
    let failed = fail_without_output(&flow, &first).await;
    let step = queue_reassessment(&flow, &failed, None, 1).await;
    let second = claim(f, &step).await;
    let second_failed = fail_without_output(&flow, &second).await;
    let plan = json!({"failedTaskId":second["id"],"cause":"执行结束而完整报告没有保存",
        "change":"先读取冻结的准则和正文，构造完整报告，再保存并读回真实回执",
        "verification":"核对报告绑定的写作者、正文版本以及完成回执"});
    let step = queue_reassessment(&flow, &second_failed, Some(plan.clone()), 2).await;
    let recovered = claim(f, &step).await;
    assert_eq!(
        recovered["inputManifest"]["workDecisionAction"]["sourceTaskId"],
        second["id"]
    );
    assert_eq!(recovered["inputManifest"]["recoveryPlan"], plan);
    assert_eq!(recovered["target"]["writerTaskId"], flow.writer["id"]);
    let grant = f.grant(&recovered).await;
    assert_eq!(grant["maxRequests"], 14);
    save_read_complete(&flow, &recovered, "second-recovered-report").await;
    assert_eq!(immutable_authority(f), authority);
    let work = f.work().await;
    let encoded: String = f
        .db()
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [work["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let ledger: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(ledger["usedRequests"], 3);
    assert_eq!(ledger["usedTokens"], 375);
    assert_eq!(ledger["maxRequests"], 16);
    for failed in [&first, &second] {
        assert_eq!(
            f.db()
                .query_row(
                    "SELECT status FROM story_tasks_v2 WHERE task_id=?1",
                    [failed["id"].as_str().unwrap()],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "failed"
        );
        assert!(
            ledger["charges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|charge| charge["taskId"] == failed["id"] && charge["requests"] == 1)
        );
    }
}

#[tokio::test]
async fn recovery_claim_preserves_source_target_parent_output_and_known_zero_fences() {
    for fence in [
        "sourceOperation",
        "foreignSourceProject",
        "foreignSourceWork",
        "sourceNotEarlier",
        "sourceUnsupportedError",
        "sourceRunning",
        "sourceCancelled",
        "sourceNotRevoked",
        "sourceOutputRefs",
        "sourceOutput",
        "sourceAcceptedOperation",
        "sourceUnsettled",
        "sourceUnknownUsage",
        "sourceFrozenTarget",
        "wrongSelectionWriter",
        "wrongSelectionRevision",
        "stepTarget",
        "actualTarget",
        "wrongParentTask",
        "wrongParentAttempt",
        "parentNotCompleted",
        "producerNotWriter",
        "producerForeignProject",
        "producerOutput",
        "wrongRecoveryPlan",
    ] {
        let flow = Flow::unmodeled().await;
        let f = &flow.fixture;
        let first = flow.assessment().await;
        let failed = fail_without_output(&flow, &first).await;
        let step = queue_reassessment(&flow, &failed, None, 1).await;
        let mut request = claim_request(f, &step);
        let foreign = if matches!(
            fence,
            "foreignSourceProject" | "foreignSourceWork" | "producerForeignProject"
        ) {
            foreign_authority(f).await
        } else {
            (String::new(), String::new())
        };
        let db = f.db();
        let source = first["id"].as_str().unwrap();
        let producer = flow.writer["id"].as_str().unwrap();
        let step_id = step["id"].as_str().unwrap();
        let work = step["target"]["workDecisionAction"]["workTaskId"]
            .as_str()
            .unwrap();
        let parent = step["target"]["workDecisionAction"]["decisionTaskId"]
            .as_str()
            .unwrap();
        let parent_step = step["target"]["workDecisionAction"]["decisionStepId"]
            .as_str()
            .unwrap();
        match fence {
            "sourceOperation" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET operation='advisory' WHERE task_id=?1",
                    [source],
                )
                .unwrap();
            }
            "foreignSourceProject" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET project_id=?2 WHERE task_id=?1",
                    params![source, foreign.0],
                )
                .unwrap();
            }
            "foreignSourceWork" => {
                db.execute(
                    "UPDATE story_work_steps_v1 SET work_task_id=?2 WHERE current_attempt_id=?1",
                    params![source, foreign.1],
                )
                .unwrap();
            }
            "sourceNotEarlier" => {
                db.execute("UPDATE story_work_attempts_v1 SET sequence=(SELECT MAX(sequence)+1 FROM story_work_attempts_v1) WHERE attempt_id=?1",[source]).unwrap();
            }
            "sourceUnsupportedError" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET error='provider_unknown_failure' WHERE task_id=?1",
                    [source],
                )
                .unwrap();
            }
            "sourceRunning" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET status='running' WHERE task_id=?1",
                    [source],
                )
                .unwrap();
            }
            "sourceCancelled" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET status='cancelled' WHERE task_id=?1",
                    [source],
                )
                .unwrap();
            }
            "sourceNotRevoked" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET capability_revoked_at=NULL WHERE task_id=?1",
                    [source],
                )
                .unwrap();
            }
            "sourceOutputRefs" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET output_refs_json=?2 WHERE task_id=?1",
                    params![source, json!([flow.draft["revisionId"]]).to_string()],
                )
                .unwrap();
            }
            "sourceOutput" => {
                db.execute("INSERT INTO story_task_outputs_v2 SELECT ?1,ordinal,output_kind,revision_id,working_base_revision_id,completeness FROM story_task_outputs_v2 WHERE task_id=?2",params![source,producer]).unwrap();
            }
            "sourceAcceptedOperation" => {
                db.execute("INSERT INTO story_tool_operations_v1 VALUES('unknown-assessment-save',?1,?2,?3,'turn','call','save_revision_outcome','hash',NULL,'accepted',NULL,1,1)",params![f.project().as_str(),source,first["threadId"].as_str()]).unwrap();
            }
            "sourceUnsettled" => {
                db.execute(
                    "UPDATE story_task_provider_budgets_v1 SET settled=0 WHERE task_id=?1",
                    [source],
                )
                .unwrap();
            }
            "sourceUnknownUsage" => {
                let encoded: String = db
                    .query_row(
                        "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
                        [work],
                        |row| row.get(0),
                    )
                    .unwrap();
                let mut ledger: Value = serde_json::from_str(&encoded).unwrap();
                ledger["unknownTokens"] = json!(1);
                for charge in ledger["charges"].as_array_mut().unwrap() {
                    if charge["taskId"] == source {
                        charge["unknownTokens"] = json!(1);
                    }
                }
                db.execute(
                    "UPDATE story_work_budgets_v1 SET ledger_json=?2 WHERE work_task_id=?1",
                    params![work, ledger.to_string()],
                )
                .unwrap();
            }
            "sourceFrozenTarget" => {
                db.execute("UPDATE story_tasks_v2 SET target_json=json_set(target_json,'$.draftRevisionId','foreign-draft') WHERE task_id=?1",[source]).unwrap();
            }
            "wrongSelectionWriter" => {
                db.execute("UPDATE story_work_decision_actions_v1 SET target_json=json_set(target_json,'$.writerTaskId',?2) WHERE step_id=?1",params![step_id,source]).unwrap();
            }
            "wrongSelectionRevision" => {
                db.execute("UPDATE story_work_decision_actions_v1 SET source_revision_id=?2 WHERE step_id=?1",params![step_id,f.original["revisionId"].as_str()]).unwrap();
            }
            "stepTarget" => {
                db.execute("UPDATE story_work_steps_v1 SET target_json=json_set(target_json,'$.originalRevisionId','foreign-original') WHERE id=?1",[step_id]).unwrap();
            }
            "actualTarget" => {
                request["target"]["draftRevisionId"] = json!("foreign-draft");
            }
            "wrongParentTask" => {
                db.execute("UPDATE story_work_decision_actions_v1 SET decision_task_id=?2 WHERE step_id=?1",params![step_id,f.discussion["id"].as_str()]).unwrap();
            }
            "wrongParentAttempt" => {
                db.execute(
                    "UPDATE story_work_steps_v1 SET current_attempt_id=?2 WHERE id=?1",
                    params![parent_step, f.discussion["id"].as_str()],
                )
                .unwrap();
            }
            "parentNotCompleted" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET status='running' WHERE task_id=?1",
                    [parent],
                )
                .unwrap();
            }
            "producerNotWriter" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET operation='advisory' WHERE task_id=?1",
                    [producer],
                )
                .unwrap();
            }
            "producerForeignProject" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET project_id=?2 WHERE task_id=?1",
                    params![producer, foreign.0],
                )
                .unwrap();
            }
            "producerOutput" => {
                db.execute(
                    "UPDATE story_task_outputs_v2 SET completeness='scenePartial' WHERE task_id=?1",
                    [producer],
                )
                .unwrap();
            }
            "wrongRecoveryPlan" => {
                let plan = json!({"failedTaskId":producer,"cause":"报告没有保存","change":"提交完整报告","verification":"读回报告"});
                db.execute("UPDATE story_work_steps_v1 SET target_json=json_set(target_json,'$.recoveryPlan',json(?2)) WHERE id=?1",params![step_id,plan.to_string()]).unwrap();
                db.execute("UPDATE story_work_decision_actions_v1 SET target_json=json_set(target_json,'$.recoveryPlan',json(?2)) WHERE step_id=?1",params![step_id,plan.to_string()]).unwrap();
                request["target"]["recoveryPlan"] = plan;
            }
            _ => unreachable!(),
        }
        let before = database_snapshot(f);
        let (code, rejected) = f.request("POST", "/tasks", AUTHOR, request).await;
        assert!(
            code.is_client_error() || code.is_server_error(),
            "{fence}: {rejected}"
        );
        assert_eq!(database_snapshot(f), before, "{fence}");
    }
}

#[tokio::test]
async fn recovered_assessment_claim_still_requires_author_permission_and_current_uncancelled_work()
{
    for fence in ["permission", "cancel", "head"] {
        let flow = Flow::unmodeled().await;
        let f = &flow.fixture;
        let first = flow.assessment().await;
        let failed = fail_without_output(&flow, &first).await;
        let step = queue_reassessment(&flow, &failed, None, 1).await;
        let actor = if fence == "permission" {
            CANDIDATE
        } else {
            AUTHOR
        };
        if fence == "cancel" {
            let work = f.work().await;
            let (code, cancelled) = f
                .request(
                    "POST",
                    &format!("/work-tasks/{}/control", work["id"].as_str().unwrap()),
                    AUTHOR,
                    json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":work["threadId"],"command":"cancel","expectedControlRevision":work["controlRevision"]}),
                )
                .await;
            assert_eq!(code, StatusCode::OK, "{cancelled}");
        }
        if fence == "head" {
            let mut next = f.original.clone();
            next["sceneGroups"][1]["body"] = json!("她把密函交给守卫。");
            join_body(&mut next);
            let (code, changed) = f
                .checkpoint(None, &flow.draft, next, "before-recovery-claim")
                .await;
            assert_eq!(code, StatusCode::CREATED, "{changed}");
        }
        let before = database_snapshot(f);
        let (code, rejected) = f
            .request("POST", "/tasks", actor, claim_request(f, &step))
            .await;
        assert!(code.is_client_error(), "{fence}: {rejected}");
        assert_eq!(database_snapshot(f), before, "{fence}");
    }
}
