use super::{
    fixture::*,
    flow::{Flow, evidence},
};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn proposal_validates_bounded_scoped_goals_and_approval_freezes_the_exact_criteria() {
    let f = Fixture::new().await;
    let mut invalid = Vec::new();
    for goals in [
        Value::Null,
        json!([]),
        json!(vec![desired_outcomes()[0].clone(); 5]),
    ] {
        let mut target = f.target.clone();
        target["desiredOutcomes"] = goals;
        invalid.push(target);
    }
    for (pointer, value) in [
        ("/desiredOutcomes/0/id", json!("")),
        ("/desiredOutcomes/0/id", json!("目标")),
        ("/desiredOutcomes/0/id", json!("a".repeat(65))),
        ("/desiredOutcomes/1/id", json!("hide-letter")),
        ("/desiredOutcomes/0/criterion", json!("  ")),
        ("/desiredOutcomes/0/criterion", json!("月".repeat(501))),
        ("/desiredOutcomes/0/groupIds", json!([])),
        ("/desiredOutcomes/0/groupIds", json!(["letter", "letter"])),
        ("/desiredOutcomes/0/groupIds", json!(["opening"])),
        ("/desiredOutcomes/0/groupIds", json!(["ending"])),
        ("/desiredOutcomes/0/groupIds", json!(["missing"])),
        (
            "/desiredOutcomes/0/groupIds",
            json!((0..17).map(|i| format!("group-{i}")).collect::<Vec<_>>()),
        ),
        ("/scope/type", json!("wholeChapter")),
    ] {
        let mut target = f.target.clone();
        *target.pointer_mut(pointer).unwrap() = value;
        invalid.push(target);
    }
    for (index, target) in invalid.into_iter().enumerate() {
        let before = f.ledger_counts();
        let (status, error) = f.propose(target, &format!("bad-goal-{index}")).await;
        assert!(status.is_client_error(), "{index}: {error}");
        assert_eq!(f.ledger_counts(), before);
        assert_eq!(
            f.request("GET", &f.actions_path(), AUTHOR, Value::Null)
                .await,
            (StatusCode::OK, json!({"actions":[]}))
        );
    }
    let before = f.ledger_counts();
    let (_, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    let mut unapproved = f.task_request("generateChapter", f.target.clone());
    unapproved["initialBudget"] = json!({"authorizationId":"unapproved-goals-budget",
        "authorMessageId":"unapproved-goals-author","expectedBaseSnapshotId":workspace["headSnapshotId"],
        "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":2,"maxTokens":1000,
        "deadlineAt":unapproved["capabilityExpiresAt"]});
    let (status, error) = f.request("POST", "/tasks", AUTHOR, unapproved).await;
    assert!(status.is_client_error(), "Unapproved root goals: {error}");
    assert_eq!(f.ledger_counts(), before);
    let writer = f.writer(f.target.clone()).await;
    for (pointer, value) in [
        ("/desiredOutcomes/0/criterion", json!("改成烧信。")),
        ("/desiredOutcomes/0/groupIds", json!(["witness"])),
        ("/requestedByTaskId", json!("fake-author-source")),
        ("/requestedActionId", json!("fake-author-action")),
    ] {
        let mut target = writer["target"].clone();
        *target.pointer_mut(pointer).unwrap() = value;
        let before = f.ledger_counts();
        let (status, error) = f.create_task("generateChapter", target).await;
        assert!(status.is_client_error(), "{pointer}: {error}");
        assert_eq!(f.ledger_counts(), before);
    }
}

#[tokio::test]
async fn reports_enforce_source_identity_complete_goals_changed_scene_evidence_and_unicode_bounds()
{
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let before = f.ledger_counts();
    let before_work = f.work().await;
    let (status, error) = f.complete(&task).await;
    assert!(status.is_client_error(), "No required output: {error}");
    assert_eq!(f.work().await, before_work);
    let valid = flow.report();
    let mut invalid = Vec::new();
    for field in [
        "writerTaskId",
        "chapterId",
        "originalRevisionId",
        "draftRevisionId",
        "knowledgeBatchId",
    ] {
        let mut report = valid.clone();
        report[field] = json!("fake-source");
        invalid.push(report);
    }
    for (pointer, value) in [
        ("/outcomes/0/id", json!("invented-criterion")),
        ("/outcomes/1/id", json!("hide-letter")),
        ("/outcomes/0/status", json!("achieved")),
        ("/outcomes/0/reason", json!("")),
        ("/outcomes/0/reason", json!("月".repeat(1001))),
        ("/outcomes/0/evidence", json!([])),
        ("/outcomes/1/evidence", json!([])),
        ("/outcomes/0/evidence/0/groupId", json!("opening")),
        ("/outcomes/0/evidence/0/groupId", json!("missing")),
        ("/outcomes/0/evidence/0/quote", json!("她烧掉密函。")),
        ("/outcomes/0/evidence/0/quote", json!("月".repeat(1001))),
        ("/outcomes/0/evidence/0/start", json!(-1)),
        ("/outcomes/0/evidence/0/start", json!(4)), // Emoji byte width is not a character offset.
        ("/outcomes/0/evidence/0/end", json!(0)),
        (
            "/outcomes/0/evidence/0/end",
            json!(NEW_BODY.chars().count() + 1),
        ),
    ] {
        let mut report = valid.clone();
        *report.pointer_mut(pointer).unwrap() = value;
        invalid.push(report);
    }
    let mut missing = valid.clone();
    missing["outcomes"].as_array_mut().unwrap().pop();
    invalid.push(missing);
    let mut criterion_scope = valid.clone();
    criterion_scope["outcomes"][0]["evidence"] =
        json!([evidence("witness", "守卫沉默。", "守卫沉默。")]);
    invalid.push(criterion_scope);
    let mut extra = valid.clone();
    extra["outcomes"]
        .as_array_mut()
        .unwrap()
        .push(valid["outcomes"][0].clone());
    invalid.push(extra);
    let mut too_many = valid.clone();
    too_many["outcomes"][0]["evidence"] =
        json!(vec![valid["outcomes"][0]["evidence"][0].clone(); 5]);
    invalid.push(too_many);
    let mut unchanged = valid.clone();
    unchanged["outcomes"][2]["status"] = json!("supported");
    unchanged["outcomes"][2]["evidence"] = json!([evidence("witness", "守卫沉默。", "守卫沉默。")]);
    invalid.push(unchanged);
    for level in ["", "/outcomes/0", "/outcomes/0/evidence/0"] {
        let mut report = valid.clone();
        report.pointer_mut(level).unwrap()["invented"] = json!(true);
        invalid.push(report);
    }
    for (index, report) in invalid.into_iter().enumerate() {
        let (status, error) = f
            .request(
                "POST",
                &flow.path(),
                CANDIDATE,
                flow.submission(&task, report, &format!("bad-report-{index}")),
            )
            .await;
        assert!(status.is_client_error(), "{index}: {error}");
        assert_eq!(f.ledger_counts(), before);
        assert_eq!(f.work().await, before_work);
    }
    let request = flow.submission(&task, valid.clone(), "corrected-report");
    assert_eq!(
        f.request("POST", &flow.path(), AUTHOR, request.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request("GET", &flow.path(), CANDIDATE, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for wrong in [&flow.writer, &flow.model] {
        let (status, error) = f
            .request(
                "POST",
                &flow.path(),
                CANDIDATE,
                flow.submission(wrong, valid.clone(), "wrong-operation"),
            )
            .await;
        assert!(status.is_client_error(), "{error}");
    }
    let (status, error) = f
        .request(
            "POST",
            "/revision-outcomes/fake-writer",
            CANDIDATE,
            request.clone(),
        )
        .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(f.ledger_counts(), before);
    let (status, receipt) = f.request("POST", &flow.path(), CANDIDATE, request).await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
}
