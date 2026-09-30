use super::{
    fixture::*,
    flow::{Flow, Goals},
};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn modeling_completion_and_assessment_enqueue_are_atomic_and_replay_does_not_duplicate_steps()
{
    let flow = Flow::modeled(Goals::Approved).await;
    let f = &flow.fixture;
    let before = f.ledger_counts();
    let work = f.work().await;
    let task_path = format!("/tasks/{}", flow.model["id"].as_str().unwrap());
    let before_task = f.request("GET", &task_path, AUTHOR, Value::Null).await;
    let db = f.db();
    db.execute_batch("CREATE TRIGGER reject_outcome_step BEFORE INSERT ON story_work_steps_v1
        WHEN NEW.operation='assessRevisionOutcome' BEGIN SELECT RAISE(ABORT,'outcome enqueue failure'); END;").unwrap();
    let (status, error) = f.complete(&flow.model).await;
    assert!(status.is_server_error(), "{error}");
    assert_eq!(f.ledger_counts(), before);
    assert_eq!(f.work().await, work);
    assert_eq!(
        f.request("GET", &task_path, AUTHOR, Value::Null).await,
        before_task
    );
    db.execute_batch("DROP TRIGGER reject_outcome_step;")
        .unwrap();
    let (status, completed) = f.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let work = f.work().await;
    let counts = f.ledger_counts();
    // Completion revokes Candidate capability; only the trusted Author transport
    // can reconcile the exact terminal result, without granting another execution.
    assert_eq!(f.complete(&flow.model).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        f.request(
            "POST",
            &format!("/tasks/{}/complete", flow.model["id"].as_str().unwrap()),
            AUTHOR,
            json!({"threadId":flow.model["threadId"],"summary":"完成当前步骤"})
        )
        .await,
        (StatusCode::OK, completed)
    );
    assert_eq!(f.ledger_counts(), counts);
    assert_eq!(f.work().await, work);
    assert_eq!(step(&work, "assessRevisionOutcome")["status"], "queued");
}

#[tokio::test]
async fn report_publication_rolls_back_outputs_heads_receipts_and_retries_exactly_once() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let before = f.ledger_counts();
    let heads = f.repo().object_heads(&f.project()).unwrap();
    let work = f.work().await;
    let request = flow.submission(&task, flow.report(), "rollback-report");
    let db = f.db();
    db.execute_batch("CREATE TRIGGER reject_outcome_output BEFORE INSERT ON story_task_outputs_v2
        WHEN NEW.output_kind='revisionOutcome' BEGIN SELECT RAISE(ABORT,'output registration failure'); END;").unwrap();
    let (status, error) = f
        .request("POST", &flow.path(), CANDIDATE, request.clone())
        .await;
    assert!(status.is_server_error(), "{error}");
    assert_eq!(f.ledger_counts(), before);
    assert_eq!(f.repo().object_heads(&f.project()).unwrap(), heads);
    assert_eq!(f.work().await, work);
    assert_eq!(
        f.request("GET", &flow.path(), AUTHOR, Value::Null).await,
        (StatusCode::OK, Value::Null)
    );
    db.execute_batch("DROP TRIGGER reject_outcome_output;")
        .unwrap();
    let (status, receipt) = f
        .request("POST", &flow.path(), CANDIDATE, request.clone())
        .await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    let counts = f.ledger_counts();
    assert_eq!(
        f.request("POST", &flow.path(), CANDIDATE, request.clone())
            .await,
        (StatusCode::CREATED, receipt)
    );
    let mut changed = request;
    changed["payload"]["outcomes"][0]["reason"] = json!("改写已有收据的解释");
    let (status, error) = f.request("POST", &flow.path(), CANDIDATE, changed).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(f.ledger_counts(), counts);
    let work = f.work().await;
    db.execute_batch(
        "CREATE TRIGGER reject_outcome_completion BEFORE UPDATE ON story_work_tasks_v1
        BEGIN SELECT RAISE(ABORT,'work completion failure'); END;",
    )
    .unwrap();
    let (status, error) = f.complete(&task).await;
    assert!(status.is_server_error(), "{error}");
    assert_eq!(f.ledger_counts(), counts);
    assert_eq!(f.work().await, work);
    db.execute_batch("DROP TRIGGER reject_outcome_completion;")
        .unwrap();
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
}

#[tokio::test]
async fn assessment_claim_requires_the_registered_target_and_rolls_back_late_budget_failure() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let target = flow.assessment_target().await;
    let before = f.ledger_counts();
    let work = f.work().await;
    for field in [
        "writerTaskId",
        "chapterId",
        "originalRevisionId",
        "draftRevisionId",
        "knowledgeBatchId",
        "resumeStepId",
    ] {
        let mut forged = target.clone();
        forged[field] = json!("fake-source");
        let (status, error) = f.create_task("assessRevisionOutcome", forged).await;
        assert!(status.is_client_error(), "{field}: {error}");
        assert_eq!(f.ledger_counts(), before);
        assert_eq!(f.work().await, work);
    }
    let mut missing_step = target.clone();
    missing_step.as_object_mut().unwrap().remove("resumeStepId");
    let mut mixed = target.clone();
    mixed["sourceTaskId"] = flow.writer["id"].clone();
    for forged in [missing_step, mixed] {
        let (status, error) = f.create_task("assessRevisionOutcome", forged).await;
        assert!(status.is_client_error(), "{error}");
        assert_eq!(f.ledger_counts(), before);
    }
    let mut wrong_writer = target.clone();
    wrong_writer["writerTaskId"] = flow.model["id"].clone();
    let (status, error) = f.create_task("assessRevisionOutcome", wrong_writer).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(f.ledger_counts(), before);
    let db = f.db();
    db.execute_batch(
        "CREATE TRIGGER reject_assessment_grant BEFORE INSERT ON story_task_provider_budgets_v1
        BEGIN SELECT RAISE(ABORT,'assessment grant failure'); END;",
    )
    .unwrap();
    let (status, error) = f.create_task("assessRevisionOutcome", target).await;
    assert!(status.is_server_error(), "{error}");
    assert_eq!(f.ledger_counts(), before);
    assert_eq!(f.work().await, work);
    db.execute_batch("DROP TRIGGER reject_assessment_grant;")
        .unwrap();
    let task = flow.assessment().await;
    assert_eq!(f.grant(&task).await["workTaskId"], work["id"]);
}

#[tokio::test]
async fn a_fresh_author_budgeted_modeling_batch_invalidates_report_publication_for_the_old_batch() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let (status, batch) = f
        .request(
            "POST",
            "/modeling-batches",
            AUTHOR,
            json!({"objectId":"draft-chapter-one","sourceRevisionId":flow.draft["revisionId"],
            "expectedRevision":0,"extractorVersion":"author-remodel"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let (_, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    let mut request = f.task_request("modelKnowledge", json!({"batchId":batch["id"]}));
    request["authorIntent"] = json!("重新抽取当前改稿的知识，复核密函位置。");
    request["initialBudget"] = json!({"authorizationId":"remodel-budget","authorMessageId":"remodel-author",
        "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":2,"maxTokens":1000,"deadlineAt":request["capabilityExpiresAt"]});
    let (status, model) = f.request("POST", "/tasks", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{model}");
    let (status, published) = f
        .request(
            "POST",
            &format!(
                "/modeling-batches/{}/publish",
                batch["id"].as_str().unwrap()
            ),
            CANDIDATE,
            json!({"taskId":model["id"],"threadId":model["threadId"],"expectedRevision":0,
            "entities":flow.batch["entities"],"assertions":[]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (status, completed) = f.complete(&model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let counts = f.ledger_counts();
    let (status, error) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "old-batch-report"),
        )
        .await;
    assert!(status.is_client_error(), "{error}");
    let (status, error) = f.complete(&task).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(f.ledger_counts(), counts);
}

#[tokio::test]
async fn paused_work_cannot_claim_and_cancellation_revokes_live_assessment_submission() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let work = f.work().await;
    let path = format!("/work-tasks/{}/control", work["id"].as_str().unwrap());
    let (status, paused) = f
        .request(
            "POST",
            &path,
            AUTHOR,
            json!({"command":"pause","expectedVersion":work["version"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    let counts = f.ledger_counts();
    let (status, error) = f
        .create_task("assessRevisionOutcome", flow.assessment_target().await)
        .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(f.ledger_counts(), counts);
    assert_eq!(f.work().await, paused);
    let (status, resumed) = f
        .request(
            "POST",
            &path,
            AUTHOR,
            json!({"command":"resume","expectedVersion":paused["version"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let task = flow.assessment().await;
    let active = f.work().await;
    let (status, cancelled) = f
        .request(
            "POST",
            &path,
            AUTHOR,
            json!({"command":"cancel","expectedVersion":active["version"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    let counts = f.ledger_counts();
    let (status, error) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "cancelled-report"),
        )
        .await;
    assert!(status.is_client_error(), "{error}");
    let (status, error) = f.complete(&task).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(f.ledger_counts(), counts);
    assert_eq!(f.work().await, cancelled);
}

#[tokio::test]
async fn changing_the_draft_revokes_queued_claim_live_report_and_required_output_completion() {
    for state in ["queued", "running", "reported"] {
        let flow = Flow::queued().await;
        let f = &flow.fixture;
        let task = if state == "queued" {
            None
        } else {
            Some(flow.assessment().await)
        };
        if state == "reported" {
            let (status, receipt) = f
                .request(
                    "POST",
                    &flow.path(),
                    CANDIDATE,
                    flow.submission(task.as_ref().unwrap(), flow.report(), "before-stale"),
                )
                .await;
            assert_eq!(status, StatusCode::CREATED, "{receipt}");
        }
        let mut next = f.original.clone();
        next["sceneGroups"][1]["body"] = json!("她把密函交给守卫。");
        join_body(&mut next);
        let (status, draft) = f
            .checkpoint(None, &flow.draft, next, "invalidate-assessment")
            .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        let before = f.ledger_counts();
        if let Some(task) = task {
            let (status, error) = f
                .request(
                    "POST",
                    &flow.path(),
                    CANDIDATE,
                    flow.submission(&task, flow.report(), "stale-report"),
                )
                .await;
            assert!(status.is_client_error(), "{state}: {error}");
            let (status, error) = f.complete(&task).await;
            assert!(status.is_client_error(), "{state}: {error}");
        } else {
            let (status, error) = f
                .create_task("assessRevisionOutcome", flow.assessment_target().await)
                .await;
            assert!(status.is_client_error(), "{error}");
        }
        assert_eq!(f.ledger_counts(), before);
    }
}
