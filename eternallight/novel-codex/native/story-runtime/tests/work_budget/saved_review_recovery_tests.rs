//! Pure recovery aggregates real frozen reports; it never settles unknown usage.
use super::unknown_usage_admission::{candidate, complete, provider_grant, settle, snapshot};
use super::{AUTHOR, CANDIDATE, call, initial};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

#[path = "../../../../shared/novel-operation-digest.rs"]
mod operation_digest;
const CAP: &str = "work-budget-usage-capability-1234567890";

struct Fixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
    root: String,
    work_id: String,
    run: Value,
    source: Option<Value>,
    tasks: Vec<Value>,
}

impl Fixture {
    fn work_uri(&self) -> String {
        format!("{}/work-tasks/{}", self.root, self.work_id)
    }
    fn run_uri(&self) -> String {
        format!(
            "{}/planning-reviews/{}",
            self.root,
            self.run["id"].as_str().unwrap()
        )
    }
    async fn get(&self, uri: &str) -> Value {
        let (code, value) = call(&self.path, "GET", uri, Value::Null, AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{value}");
        value
    }
    async fn control(&self, command: &str, version: &Value) -> (StatusCode, Value) {
        call(
            &self.path,
            "POST",
            &format!("{}/control", self.work_uri()),
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":(self.get(&self.work_uri()).await)["threadId"],"command":command,"expectedControlRevision":version}),
            AUTHOR,
        )
        .await
    }
    async fn pause(&self) -> Value {
        let current = self.get(&self.work_uri()).await;
        let (code, value) = self.control("pause", &current["controlRevision"]).await;
        assert_eq!(code, StatusCode::OK, "{value}");
        assert_eq!(value["status"], "paused");
        value
    }
}

async fn register(
    path: &std::path::Path,
    root: &str,
    source: &Value,
    outline: &Value,
    roles: &Value,
) -> Value {
    let body = json!({"threadId":"budget-thread","expectedRevision":0,"expectedIntentRevision":0,
        "artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,
        "roles":roles,"focus":"按作者要求检查现有结构与人物，不改大纲","idempotencyKey":"actual-saved-review"});
    let mut arguments = body.clone();
    arguments["projectId"] = json!(root.strip_prefix("/api/projects/").unwrap());
    arguments["taskId"] = source["id"].clone();
    let response = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .unwrap()
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    )
    .oneshot(
        Request::builder()
            .method("POST")
            .uri(format!(
                "{root}/tasks/{}/planning-reviews",
                source["id"].as_str().unwrap()
            ))
            .header("authorization", format!("Bearer {CANDIDATE}"))
            .header("content-type", "application/json")
            .header("x-novel-task-capability", CAP)
            .header("x-novel-tool-name", "request_planning_review")
            .header("x-novel-root-thread", "budget-thread")
            .header("x-novel-caller-thread", "budget-thread")
            .header("x-novel-turn", "actual-review-registration-turn")
            .header("x-novel-tool-call", "actual-review-registration-call")
            .header(
                "x-novel-request-hash",
                operation_digest::operation_digest("request_planning_review", &arguments).unwrap(),
            )
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();
    let code = response.status();
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(code, StatusCode::OK, "{result}");
    result
}

async fn fixture(conversation: bool, metered: bool, partial: bool, queued: bool) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("saved-review.sqlite3");
    let (root, mut request) = initial::request(&path).await;
    let (code, outline) = call(
        &path,
        "POST",
        &format!("{root}/artifacts/outline"),
        json!({
        "payload":{"nodes":[{"id":"book","kind":"book","position":1,"title":"港城","purpose":"记忆",
        "summary":"城市消失","dependencyKeys":[],"certainty":"tentative","protection":"free"}]},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"actual-outline"}),
        AUTHOR,
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{outline}");
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let expires = request["capabilityExpiresAt"].clone();
    let roles = if queued {
        json!(["structure", "character", "experience"])
    } else {
        json!(["structure", "character"])
    };
    let (run, source) = if conversation {
        request["capabilityHash"] = json!(format!("{:x}", Sha256::digest(CAP.as_bytes())));
        request["authorIntent"] =
            json!("从结构和人物视角检查现有大纲，保存报告，不修改或采用规划。");
        request["initialBudget"]["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
        request["initialBudget"]["expectedSourceDigest"] = workspace["sourceDigest"].clone();
        request["initialBudget"]["maxRequests"] = if metered { Value::Null } else { json!(4) };
        request["initialBudget"]["maxTokens"] = if metered { Value::Null } else { json!(2000) };
        if metered {
            request["initialBudget"]["deadlineAt"] = Value::Null;
        }
        let (code, source) = call(&path, "POST", &format!("{root}/tasks"), request, AUTHOR).await;
        assert_eq!(code, StatusCode::CREATED, "{source}");
        let grant = provider_grant(&path, &root, &source).await;
        let run = register(&path, &root, &source, &outline, &roles).await;
        complete(&path, &root, &source).await;
        settle(&path, &root, &source, &grant, None).await;
        (run, Some(source))
    } else {
        let (code, run) = call(
            &path,
            "POST",
            &format!("{root}/planning-reviews"),
            json!({
            "artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,
            "authorIntent":"从结构和人物视角检查现有大纲，不修改规划","roles":roles}),
            AUTHOR,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{run}");
        let uri = format!("{root}/work-tasks/{}", run["workTaskId"].as_str().unwrap());
        let (_, work) = call(&path, "GET", &uri, Value::Null, AUTHOR).await;
        let (code,budget)=call(&path,"POST",&format!("{uri}/budget-authorization"),json!({
            "expectedVersion":work["version"],"expectedProjectRevision":workspace["revision"],
            "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":if metered{Value::Null}else{json!(4)},
            "maxTokens":if metered{Value::Null}else{json!(2000)},"deadlineAt":if metered{Value::Null}else{expires.clone()}}),AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{budget}");
        (run, None)
    };
    let mut f = Fixture {
        _directory: directory,
        path,
        root,
        work_id: run["workTaskId"].as_str().unwrap().into(),
        run,
        source,
        tasks: Vec::new(),
    };
    f.run = f.get(&f.run_uri()).await;
    for (index, role) in ["structure", "character"].into_iter().enumerate() {
        let (code,task)=call(&f.path,"POST",&format!("{}/seats/{role}/tasks",f.run_uri()),json!({
            "threadId":"budget-thread","model":"fixture","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),
            "capabilityExpiresAt":expires}),AUTHOR).await;
        assert_eq!(code, StatusCode::OK, "{task}");
        let grant = provider_grant(&f.path, &f.root, &task).await;
        let coverage = if partial && index == 1 {
            json!([])
        } else {
            Value::Array(
                f.run["checksByRole"][role]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| json!({"checkId":id,"status":"checked"}))
                    .collect(),
            )
        };
        let (code,saved)=candidate(&f.path,"POST",&format!("{}/reports",f.run_uri()),json!({
            "taskId":task["id"],"threadId":"budget-thread","expectedRevision":0,
            "report":{"summary":format!("真实保存的 {role} 视角报告"),"strengths":[],"findings":[],"coverage":coverage}})).await;
        assert_eq!(code, StatusCode::OK, "{saved}");
        complete(&f.path, &f.root, &task).await;
        let charge = settle(
            &f.path,
            &f.root,
            &task,
            &grant,
            Some(if index == 1 {
                json!({})
            } else {
                json!({"total_tokens":125})
            }),
        )
        .await;
        assert_eq!(charge["unknownTokens"], if index == 1 { 70 } else { 0 });
        f.tasks.push(task);
    }
    f.run = f.get(&f.run_uri()).await;
    f
}

fn unchanged_evidence(before: &Value, after: &Value) {
    assert_eq!(after["engine"], before["engine"]);
    for table in [
        "story_author_messages_v1",
        "story_work_budget_authorizations_v1",
        "story_action_permits_v1",
        "story_work_budgets_v1",
        "story_task_provider_budgets_v1",
        "story_tasks_v2",
        "story_work_attempts_v1",
        "story_task_outputs_v2",
        "story_planning_review_reports_v2",
        "story_planning_review_seats_v2",
        "story_object_heads_v2",
        "story_revisions_v2",
        "story_snapshots_v2",
        "story_work_decisions_v1",
    ] {
        assert_eq!(
            after["native"][table], before["native"][table],
            "changed immutable evidence {table}"
        );
    }
}

#[tokio::test]
async fn frozen_full_and_partial_reports_recover_under_unknown_finite_and_null_usage_without_execution()
 {
    for conversation in [false, true] {
        for metered in [false, true] {
            for partial in [false, true] {
                let f = fixture(conversation, metered, partial, false).await;
                let paused = f.pause().await;
                assert_eq!(paused["resumeMode"], "recoverSavedResult");
                assert_eq!(
                    paused["resumeBlockReason"],
                    if metered {
                        Value::Null
                    } else {
                        json!("unknownUsage")
                    }
                );
                let before = snapshot(&f.path);
                if !metered {
                    let (code, rejected) = f.control("resume", &paused["controlRevision"]).await;
                    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
                    assert_eq!(rejected["error"]["code"], "work_control_rejected");
                    assert_eq!(rejected["outcome"]["receipt"]["reason"], "WorkUsageUnknown");
                    super::unknown_usage_admission::assert_rejected_control_only(
                        &f.path, &before, &rejected,
                    );
                } else {
                    assert_eq!(snapshot(&f.path), before);
                }
                let before = snapshot(&f.path);
                let (code, rejected) = call(
                    &f.path,
                    "POST",
                    &format!("{}/finalize", f.run_uri()),
                    json!({}),
                    AUTHOR,
                )
                .await;
                assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
                assert_eq!(snapshot(&f.path), before);
                let (code, recovered) = f
                    .control("recoverSavedResult", &paused["controlRevision"])
                    .await;
                assert_eq!(code, StatusCode::OK, "{recovered}");
                assert_eq!(
                    recovered["status"],
                    if conversation { "paused" } else { "completed" }
                );
                assert_eq!(
                    recovered["resumeMode"],
                    if conversation && metered {
                        json!("continueExecution")
                    } else {
                        Value::Null
                    }
                );
                if conversation {
                    assert_eq!(
                        recovered["resumeBlockReason"],
                        if metered {
                            Value::Null
                        } else {
                            json!("unknownUsage")
                        }
                    );
                }
                assert!(
                    recovered["steps"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|step| step["status"] == "completed")
                );
                unchanged_evidence(&before, &snapshot(&f.path));
                let aggregated = f.get(&f.run_uri()).await;
                assert_eq!(
                    aggregated["status"],
                    if partial { "partial" } else { "completed" }
                );
                assert_eq!(aggregated["reports"], f.run["reports"]);
                assert_eq!(aggregated["sourcePacketHash"], f.run["sourcePacketHash"]);
                assert_eq!(
                    aggregated["synthesis"]["incompleteRoles"],
                    if partial {
                        json!(["character"])
                    } else {
                        json!([])
                    }
                );
                let after = snapshot(&f.path);
                let (code, replayed) = call(
                    &f.path,
                    "POST",
                    &format!("{}/finalize", f.run_uri()),
                    json!({}),
                    AUTHOR,
                )
                .await;
                assert_eq!(code, StatusCode::OK, "{replayed}");
                assert_eq!(replayed, aggregated);
                assert_eq!(snapshot(&f.path), after);
                let (code, rejected) = f
                    .control("recoverSavedResult", &paused["controlRevision"])
                    .await;
                assert_eq!(code, StatusCode::CONFLICT);
                assert_eq!(rejected["outcome"]["receipt"]["reason"], "WorkTaskStale");
                super::unknown_usage_admission::assert_rejected_control_only(
                    &f.path, &after, &rejected,
                );
            }
        }
    }
}

#[tokio::test]
async fn saved_report_recovery_rejects_pending_or_broken_facts_and_preserves_all_records() {
    for fence in [
        "queued",
        "activeTask",
        "unsettledUsage",
        "acceptedOperation",
        "roleBinding",
        "outputReference",
        "reportTask",
        "coverage",
        "sourceAuthorRoot",
        "cancelled",
        "permission",
        "foreignProject",
    ] {
        let f = fixture(true, false, false, fence == "queued").await;
        let paused = f.pause().await;
        let task = f.tasks[1]["id"].as_str().unwrap();
        let source = f.source.as_ref().unwrap()["id"].as_str().unwrap();
        let db = rusqlite::Connection::open(&f.path).unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        match fence {
            "activeTask" => {
                db.execute("UPDATE story_tasks_v2 SET status='running',capability_revoked_at=NULL WHERE task_id=?1",[task]).unwrap();
            }
            "unsettledUsage" => {
                db.execute(
                    "UPDATE story_task_provider_budgets_v1 SET settled=0 WHERE task_id=?1",
                    [task],
                )
                .unwrap();
            }
            "acceptedOperation" => {
                db.execute("UPDATE story_tool_operations_v1 SET state='accepted',receipt_ref=NULL WHERE task_id=?1 AND tool_name='request_planning_review'",[source]).unwrap();
            }
            "roleBinding" => {
                db.execute("UPDATE story_planning_review_seats_v2 SET task_id=?1 WHERE run_id=?2 AND role='character'",rusqlite::params![source,f.run["id"].as_str().unwrap()]).unwrap();
            }
            "outputReference" => {
                db.execute("UPDATE story_task_outputs_v2 SET revision_id=(SELECT revision_id FROM story_task_outputs_v2 WHERE task_id=?2 LIMIT 1) WHERE task_id=?1",rusqlite::params![task,f.tasks[0]["id"].as_str().unwrap()]).unwrap();
            }
            "reportTask" => {
                db.execute(
                    "UPDATE story_planning_review_reports_v2 SET task_id=?2 WHERE task_id=?1",
                    rusqlite::params![task, source],
                )
                .unwrap();
            }
            "coverage" => {
                db.execute("UPDATE story_planning_review_reports_v2 SET report_json=json_set(report_json,'$.coverage',json('[]')) WHERE task_id=?1",[task]).unwrap();
            }
            "sourceAuthorRoot" => {
                db.execute("UPDATE story_author_messages_v1 SET author_text='篡改作者来源' WHERE work_task_id=?1",[&f.work_id]).unwrap();
            }
            "queued" | "cancelled" | "permission" | "foreignProject" => {}
            _ => unreachable!(),
        }
        drop(db);
        let mut uri = format!("{}/control", f.work_uri());
        let mut version = paused["controlRevision"].clone();
        let token = if fence == "permission" {
            CANDIDATE
        } else {
            AUTHOR
        };
        if fence == "cancelled" {
            let (code, cancelled) = f.control("cancel", &version).await;
            assert_eq!(code, StatusCode::OK, "{cancelled}");
            version = cancelled["controlRevision"].clone();
        }
        if fence == "foreignProject" {
            let (code, foreign) = call(
                &f.path,
                "POST",
                "/api/projects",
                json!({"logline":"","medium":"novel",
                "idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"真实外部项目"}),
                AUTHOR,
            )
            .await;
            assert_eq!(code, StatusCode::CREATED, "{foreign}");
            uri = format!(
                "/api/projects/{}/work-tasks/{}/control",
                foreign["project"]["id"].as_str().unwrap(),
                f.work_id
            );
        }
        let before = snapshot(&f.path);
        let (code, rejected) = call(
            &f.path,
            "POST",
            &uri,
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":paused["threadId"],"command":"recoverSavedResult","expectedControlRevision":version}),
            token,
        )
        .await;
        assert!(!code.is_success(), "{fence}: {rejected}");
        super::unknown_usage_admission::assert_rejected_control_only(&f.path, &before, &rejected);
    }
}
