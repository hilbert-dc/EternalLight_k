use super::{AUTHOR, CANDIDATE, CAPABILITY, call, propose, settlement, setup};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "intent_adjustment_tests.rs"]
mod intent_adjustment;

fn followup_claim(permit: &Value, step: &Value) -> Value {
    let mut target = step["target"].clone();
    target["resumeStepId"] = step["id"].clone();
    json!({"operation":"advisory","inputRevision":0,"threadId":"scope-thread","target":target,"authorIntent":"篡改为重写结局","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":permit["scope"]["deadlineAt"]})
}

async fn ready_continuation(
    path: &std::path::Path,
    rounds: u8,
    attempts: u8,
) -> (String, Value, Value, Value) {
    let (base, source, action) = setup(path).await;
    let scope_uri = format!(
        "{base}/tasks/{}/execution-scope",
        source["id"].as_str().unwrap()
    );
    let request = json!({"actionIds":[action["id"]],"maxActions":2,"maxTaskAttempts":attempts,"maxAdvisoryRounds":rounds,"expectedIntentRevision":0});
    let (status, preview) = call(
        path,
        "POST",
        &format!("{scope_uri}/preview"),
        request.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let (status, permit) = call(
        path,
        "POST",
        &scope_uri,
        json!({"request":request,"previewDigest":preview["previewDigest"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{permit}");
    let source_id = source["id"].as_str().unwrap();
    assert_eq!(
        call(
            path,
            "POST",
            &format!("{base}/tasks/{source_id}/complete"),
            json!({"threadId":"scope-thread","summary":"拟定方案"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    let project = base.trim_start_matches("/api/projects/");
    settlement::settle(path, project, source_id);
    let (status,child) = call(path,"POST",&format!("{base}/tasks"),json!({"operation":"generateBrief","inputRevision":0,"threadId":"scope-thread","authorIntent":"客户端不可替换原话",
        "target":{"type":"artifact","requestedByTaskId":source_id,"requestedActionId":action["id"]},"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":preview["deadlineAt"]}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    let (status,saved) = call(path,"POST",&format!("{base}/artifacts/brief"),json!({"taskId":child["id"],"threadId":"scope-thread","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"brief-output","payload":{"premise":"港城证词"}}),CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(
        call(
            path,
            "POST",
            &format!("{base}/tasks/{}/complete", child["id"].as_str().unwrap()),
            json!({"threadId":"scope-thread","summary":"候选简报"}),
            CANDIDATE
        )
        .await
        .0,
        StatusCode::OK
    );
    (base, source, child, permit)
}

#[tokio::test]
async fn continuation_reuses_author_provenance_and_allowance_and_transfers_only_exact_targets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("continue.sqlite3");
    let (base, source, child, permit) = ready_continuation(&path, 1, 8).await;
    let work = permit["scope"]["workTaskId"].as_str().unwrap();
    let uri = format!("{base}/work-tasks/{work}/execution-scope/continue");
    assert_eq!(
        call(&path, "POST", &uri, Value::Null, CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        !call(&path, "POST", &uri, Value::Null, AUTHOR)
            .await
            .0
            .is_success()
    );
    let project = base.trim_start_matches("/api/projects/");
    settlement::settle(&path, project, child["id"].as_str().unwrap());
    let (status, queued) = call(&path, "POST", &uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{queued}");
    assert_eq!(queued["steps"].as_array().unwrap().len(), 3);
    assert_eq!(
        call(&path, "POST", &uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, queued.clone())
    );
    let step = queued["steps"].as_array().unwrap().last().unwrap();
    let mut claim = followup_claim(&permit, step);
    claim["target"]["scopeContinuation"]["sourceTaskId"] = json!("foreign-source");
    assert!(
        !call(
            &path,
            "POST",
            &format!("{base}/tasks"),
            claim.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    claim["target"]["scopeContinuation"]["sourceTaskId"] = source["id"].clone();
    let (status, advisory) = call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{advisory}");
    assert_eq!(
        json!({"text":advisory["inputManifest"]["authorIntent"],"binding":advisory["inputManifest"]["authorIntentBinding"]}),
        json!({"text":source["inputManifest"]["authorIntent"],"binding":{"sourceTaskId":source["id"],"intentRevision":0,"actionId":null}})
    );
    let budget = call(
        &path,
        "GET",
        &format!(
            "{base}/tasks/{}/provider-budget",
            advisory["id"].as_str().unwrap()
        ),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(budget.0, StatusCode::OK, "{}", budget.1);
    assert_eq!(
        json!({"owner":budget.1["workTaskId"],"requests":budget.1["maxRequests"],"tokens":budget.1["maxTokens"]}),
        json!({"owner":work,"requests":16,"tokens":200000})
    );
    let covered = propose(&path, &base, &advisory, "followup-covered", "generateBrief").await;
    assert_eq!(covered["status"], "queued");
    assert_eq!(
        propose(&path, &base, &advisory, "outside", "generateOutline").await["status"],
        "proposed"
    );
    assert_eq!(
        propose(&path, &base, &advisory, "exhausted", "generateBrief").await["status"],
        "proposed"
    );
    let permission_uri = format!(
        "{base}/tasks/{}/actions/{}/permission",
        advisory["id"].as_str().unwrap(),
        covered["id"].as_str().unwrap()
    );
    let (_, permission) = call(&path, "GET", &permission_uri, Value::Null, AUTHOR).await;
    assert_eq!(permission["permission"]["scopePermitId"], permit["id"]);
    assert_eq!(
        permission["permission"]["evidence"]["authorMessageId"],
        source["inputManifest"]["authorMessageId"]
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_author_messages_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_work_budget_authorizations_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).unwrap(),(1,1,1,3));
    let (status, intent) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{}/intents", advisory["id"].as_str().unwrap()),
        json!({"expectedIntentRevision":0,"authorText":"停止余下修订，先重新考虑方向"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{intent}");
    assert!(
        !call(
            &path,
            "GET",
            &format!(
                "{base}/tasks/{}/provider-budget",
                advisory["id"].as_str().unwrap()
            ),
            Value::Null,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(&path, "GET", &permission_uri, Value::Null, AUTHOR)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn continuation_retry_after_reopen_keeps_its_step_round_and_original_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("analysis-retry.sqlite3");
    let (base, source, child, permit) = ready_continuation(&path, 1, 4).await;
    let project = base.trim_start_matches("/api/projects/");
    settlement::settle(&path, project, child["id"].as_str().unwrap());
    let work = permit["scope"]["workTaskId"].as_str().unwrap();
    let work_uri = format!("{base}/work-tasks/{work}");
    let continuation_uri = format!("{work_uri}/execution-scope/continue");
    let (_, queued) = call(&path, "POST", &continuation_uri, Value::Null, AUTHOR).await;
    let step = queued["steps"].as_array().unwrap().last().unwrap();
    let claim = followup_claim(&permit, step);
    let (status, first) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        claim.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let first_id = first["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "PATCH",
            &format!("{base}/tasks/{first_id}"),
            json!({"threadId":"scope-thread","status":"failed","error":"interrupted"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, blocked) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(blocked["status"], "blocked");
    assert_eq!(
        call(&path, "POST", &continuation_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, blocked.clone())
    );
    let (status, resumed) = call(
        &path,
        "POST",
        &format!("{work_uri}/control"),
        json!({"expectedVersion":blocked["version"],"command":"resume"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    // Resuming the existing step is not permission to execute ahead of unsettled usage.
    assert!(
        !call(
            &path,
            "POST",
            &format!("{base}/tasks"),
            claim.clone(),
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await,
        (StatusCode::OK, resumed)
    );
    settlement::settle(&path, project, first_id);
    // Every HTTP call reopens SQLite; the retry must derive authority from durable lineage.
    let (status, retry) = call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{retry}");
    assert_eq!(
        retry["inputManifest"]["authorIntentBinding"],
        json!({"sourceTaskId":first["id"],"intentRevision":0,"actionId":null})
    );
    assert_eq!(
        retry["inputManifest"]["authorIntent"],
        source["inputManifest"]["authorIntent"]
    );
    let id = retry["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/tasks/{id}/complete"),
            json!({"threadId":"scope-thread","summary":"恢复分析完成，无新候选"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    settlement::settle(&path, project, id);
    let completed = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    assert_eq!(completed.1["status"], "completed");
    assert_eq!(
        call(&path, "POST", &continuation_uri, Value::Null, AUTHOR).await,
        completed
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1),(SELECT COUNT(*) FROM story_work_budgets_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap(),(3,4,1,1));
}

#[tokio::test]
async fn completed_analysis_needs_new_outputs_and_paused_or_cancelled_continuations_cannot_claim() {
    for control in ["none", "pause", "cancel"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("analysis-control.sqlite3");
        let (base, _source, child, permit) = ready_continuation(&path, 2, 8).await;
        let project = base.trim_start_matches("/api/projects/");
        settlement::settle(&path, project, child["id"].as_str().unwrap());
        let work = permit["scope"]["workTaskId"].as_str().unwrap();
        let work_uri = format!("{base}/work-tasks/{work}");
        let (_, queued) = call(
            &path,
            "POST",
            &format!("{work_uri}/execution-scope/continue"),
            Value::Null,
            AUTHOR,
        )
        .await;
        let step = queued["steps"].as_array().unwrap().last().unwrap();
        let claim = followup_claim(&permit, step);
        if control != "none" {
            let paused = call(
                &path,
                "POST",
                &format!("{work_uri}/control"),
                json!({"expectedVersion":queued["version"],"command":control}),
                AUTHOR,
            )
            .await;
            assert_eq!(paused.0, StatusCode::OK, "{}", paused.1);
            assert_eq!(
                call(
                    &path,
                    "POST",
                    &format!("{work_uri}/execution-scope/continue"),
                    Value::Null,
                    AUTHOR
                )
                .await,
                paused.clone()
            );
            assert!(
                !call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR)
                    .await
                    .0
                    .is_success()
            );
            assert_eq!(
                call(&path, "GET", &work_uri, Value::Null, AUTHOR).await,
                paused
            );
        } else {
            let (status, advisory) =
                call(&path, "POST", &format!("{base}/tasks"), claim, AUTHOR).await;
            assert_eq!(status, StatusCode::CREATED, "{advisory}");
            let id = advisory["id"].as_str().unwrap();
            assert_eq!(
                call(
                    &path,
                    "POST",
                    &format!("{base}/tasks/{id}/complete"),
                    json!({"threadId":"scope-thread","summary":"分析完成，无新动作"}),
                    AUTHOR
                )
                .await
                .0,
                StatusCode::OK
            );
            settlement::settle(&path, project, id);
            let completed = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
            assert_eq!(completed.1["status"], "completed");
            assert_eq!(
                call(
                    &path,
                    "POST",
                    &format!("{work_uri}/execution-scope/continue"),
                    Value::Null,
                    AUTHOR
                )
                .await,
                completed
            );
        }
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?))).unwrap(),(3,if control=="none" {3}else{2},1));
    }
}

#[tokio::test]
async fn continuation_limits_external_edits_and_late_failures_cannot_create_steps_or_fresh_budget()
{
    for case in [
        "noRounds",
        "attemptLimit",
        "lateFailure",
        "externalEdit",
        "cancel",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("continue-gates.sqlite3");
        let (base, _source, child, permit) = ready_continuation(
            &path,
            if case == "noRounds" { 0 } else { 1 },
            if case == "attemptLimit" { 2 } else { 8 },
        )
        .await;
        let project = base.trim_start_matches("/api/projects/");
        settlement::settle(&path, project, child["id"].as_str().unwrap());
        let work = permit["scope"]["workTaskId"].as_str().unwrap();
        let work_uri = format!("{base}/work-tasks/{work}");
        let db = rusqlite::Connection::open(&path).unwrap();
        if case == "lateFailure" {
            db.execute_batch("CREATE TRIGGER reject_followup_version BEFORE UPDATE ON story_work_tasks_v1 BEGIN SELECT RAISE(ABORT,'late failure'); END;").unwrap();
        }
        if case == "externalEdit" {
            let candidate: String=db.query_row("SELECT candidate_revision_id FROM story_object_heads_v2 WHERE object_id='artifact-brief'",[],|row|row.get(0)).unwrap();
            let (status,changed)=call(&path,"POST",&format!("{base}/artifacts/brief"),json!({"expectedRevision":0,"expectedCandidateRevisionId":candidate,"idempotencyKey":"external","payload":{"premise":"外部改稿"}}),AUTHOR).await;
            assert_eq!(status, StatusCode::CREATED, "{changed}");
        }
        if case == "cancel" {
            db.execute(
                "UPDATE story_work_tasks_v1 SET status='cancelled' WHERE id=?1",
                [work],
            )
            .unwrap();
        }
        let before = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
        let result = call(
            &path,
            "POST",
            &format!("{work_uri}/execution-scope/continue"),
            Value::Null,
            AUTHOR,
        )
        .await;
        if matches!(case, "noRounds" | "cancel") {
            assert_eq!(result, before.clone());
        } else {
            assert!(!result.0.is_success(), "{case}: {}", result.1);
        }
        assert_eq!(
            call(&path, "GET", &work_uri, Value::Null, AUTHOR).await,
            before
        );
        assert_eq!(db.query_row("SELECT (SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1),(SELECT COUNT(*) FROM story_work_budgets_v1)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?))).unwrap(),(2,2,1));
    }
}
