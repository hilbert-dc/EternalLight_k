use super::{
    fixture::*,
    flow::{Flow, Goals, evidence},
};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

async fn three_criterion_flow() -> Flow {
    let mut fixture = Fixture::new().await;
    fixture
        .target
        .as_object_mut()
        .unwrap()
        .remove("desiredOutcomes");
    fixture.target["chapterAcceptance"] = json!({
        "requirements":[
            {"id":"preserve-opening","text":"保留开场"},
            {"id":"preserve-witness","text":"保留守卫"},
            {"id":"preserve-ending","text":"保留结尾"}],
        "length":{"requirement":"本次作者批准750至900字符","minCharacters":750,"maxCharacters":900}});
    let flow = Flow::from_fixture(fixture, Goals::Absent).await;
    let f = &flow.fixture;
    let (status, completed) = f.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&flow.model);
    f.choose(&flow.model, "assessRevisionOutcome").await;
    flow
}

fn three_criterion_report(flow: &Flow) -> Value {
    json!({"writerTaskId":flow.writer["id"],"chapterId":"chapter-one",
        "originalRevisionId":flow.fixture.original["revisionId"],"draftRevisionId":flow.draft["revisionId"],
        "knowledgeBatchId":flow.batch["id"],"uncoveredRequirements":[],"outcomes":[
            {"id":"preserve-opening","status":"supported","reason":"开场保留。","issue":null,"evidence":[evidence("opening","林夕走进港口。","林夕走进港口。")]},
            {"id":"preserve-witness","status":"supported","reason":"守卫保留。","issue":null,"evidence":[evidence("witness","守卫沉默。","守卫沉默。")]},
            {"id":"preserve-ending","status":"supported","reason":"结尾保留。","issue":null,"evidence":[evidence("ending","白塔熄灭。","白塔熄灭。")]}]})
}

#[tokio::test]
async fn frozen_three_criterion_feedback_allows_same_task_correction_without_passing_short_prose() {
    let flow = three_criterion_flow().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let (status, pack) = f
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
    assert_eq!(status, StatusCode::OK, "{pack}");
    let input =
        read_context_source(f, &task, &pack, "/operationContext/revisionOutcomeInput").await;
    let expected = json!(["preserve-opening", "preserve-witness", "preserve-ending"]);
    assert_eq!(input["criterionContract"]["expectedCriterionIds"], expected);
    assert_eq!(input["criterionContract"]["exactlyOnce"], true);
    assert_eq!(
        input["criterionContract"]["lengthAuthority"],
        "nativeObjectiveCheck"
    );
    assert_eq!(input["actualLength"]["satisfied"], false);
    assert_eq!(input["chapterAcceptance"]["length"]["minCharacters"], 750);
    let valid = three_criterion_report(&flow);
    assert_eq!(valid["outcomes"].as_array().unwrap().len(), 3);
    let mut bad_quote = valid.clone();
    let quote = bad_quote["outcomes"][1]["evidence"][0].as_object_mut().unwrap();
    quote.remove("start"); quote.remove("end");
    quote.insert("quote".into(), json!("原稿没有这段引文"));
    let before_quote = f.ledger_counts();
    let (status, rejected_quote) = f.request("POST", &flow.path(), CANDIDATE,
        flow.submission(&task, bad_quote, "quote-location-rejected")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected_quote}");
    assert_eq!(rejected_quote["error"]["code"], "invalid_output_payload");
    assert_eq!(rejected_quote["error"]["details"]["reason"], "evidence_quote_not_found");
    assert_eq!(rejected_quote["error"]["details"]["instancePath"], "/outcomes/1/evidence/0");
    assert_eq!(rejected_quote["error"]["details"]["sourceRef"]["groupId"], "witness");
    assert!(!rejected_quote.to_string().contains("原稿没有这段引文"));
    assert_eq!(f.ledger_counts(), before_quote);
    let mut extra = valid.clone();
    let mut invented = valid["outcomes"][0].clone();
    invented["id"] = json!("model-length-750-900");
    extra["outcomes"].as_array_mut().unwrap().push(invented);
    let mut missing = valid.clone();
    missing["outcomes"].as_array_mut().unwrap().remove(1);
    let mut duplicate = valid.clone();
    duplicate["outcomes"][1]["id"] = json!("preserve-opening");
    let mut foreign = valid.clone();
    foreign["outcomes"][1]["id"] = json!("foreign-criterion");
    let before = f.ledger_counts();
    let before_work = f.work().await;
    let before_heads = f.repo().object_heads(&f.project()).unwrap();
    let before_task = f
        .repo()
        .task(&f.project(), task["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let issue_count = || {
        f.db().query_row("SELECT (SELECT count(*) FROM story_work_issues_v1)+(SELECT count(*) FROM story_issue_observations_v1)",[],|row|row.get::<_,i64>(0)).unwrap()
    };
    let before_issues = issue_count();
    let (code, rejected) = f.complete(&task).await;
    assert_eq!(code, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(rejected["error"]["message"], "TaskOutputMissing");
    for (index, (payload, unexpected, missing_ids, duplicates)) in [
        (extra, json!(["model-length-750-900"]), json!([]), json!([])),
        (missing, json!([]), json!(["preserve-witness"]), json!([])),
        (
            duplicate,
            json!([]),
            json!(["preserve-witness"]),
            json!(["preserve-opening"]),
        ),
        (
            foreign,
            json!(["foreign-criterion"]),
            json!(["preserve-witness"]),
            json!([]),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (code, error) = f
            .request(
                "POST",
                &flow.path(),
                CANDIDATE,
                flow.submission(&task, payload, &format!("criterion-mismatch-{index}")),
            )
            .await;
        assert_eq!(code, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["error"]["code"], "invalid_output_payload");
        let detail = &error["error"]["details"];
        assert_eq!(detail["reason"], "criterion_set_mismatch");
        assert_eq!(detail["instancePath"], "/outcomes");
        assert_eq!(detail["expectedCriterionIds"], expected);
        assert_eq!(detail["unexpectedCriterionIds"], unexpected);
        assert_eq!(detail["missingCriterionIds"], missing_ids);
        assert_eq!(detail["duplicateCriterionIds"], duplicates);
        assert_eq!(
            detail["frozenRefs"],
            input["criterionContract"]["frozenRefs"]
        );
        assert!(error.to_string().len() <= 24_000);
        assert_eq!(f.ledger_counts(), before);
        assert_eq!(f.work().await, before_work);
        assert_eq!(f.repo().object_heads(&f.project()).unwrap(), before_heads);
        assert_eq!(
            f.repo()
                .task(&f.project(), task["id"].as_str().unwrap())
                .unwrap()
                .unwrap(),
            before_task
        );
        assert_eq!(issue_count(), before_issues);
    }
    let mut stale = flow.submission(&task, valid.clone(), "stale-contract-correction");
    stale["expectedRevision"] = json!(1);
    assert!(
        f.request("POST", &flow.path(), CANDIDATE, stale)
            .await
            .0
            .is_client_error()
    );
    assert_eq!(f.ledger_counts(), before);
    assert_eq!(issue_count(), before_issues);
    let (code, saved) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, valid.clone(), "same-task-corrected-criteria"),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{saved}");
    let (code, completed) = f.complete(&task).await;
    assert_eq!(code, StatusCode::OK, "{completed}");
    assert_eq!(completed["id"], task["id"]);
    assert_eq!(completed["status"], "completed");
    let stored = f
        .repo()
        .revision_outcome(&f.project(), flow.writer["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.content, valid);
    assert_eq!(stored.revision_id, saved["revisionId"]);
    assert_eq!(issue_count(), before_issues);
    f.settle(&task);
    let (_, decision) = f.decision(&task).await;
    let assessment = decision["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "revisionOutcome")
        .unwrap();
    assert_eq!(
        assessment["assessment"]["checkCounts"],
        json!({"total":3,"supported":3,"unmet":0,"insufficientEvidence":0})
    );
    assert_eq!(stored.content["uncoveredRequirements"], json!([]));
    assert_eq!(assessment["assessment"]["verdict"], "notMet");
    assert_eq!(assessment["assessment"]["length"]["satisfied"], false);
    assert_eq!(
        assessment["assessment"]["length"]["characterCount"],
        input["actualLength"]["characterCount"]
    );
    assert_eq!(
        decision["chapterAcceptance"]["length"]["minCharacters"],
        750
    );
    assert_eq!(decision["canFinish"], false);
    assert_ne!(f.work().await["status"], "completed");
}

#[tokio::test]
async fn frozen_three_criteria_allow_a_negative_report_as_a_saved_completed_task() {
    let flow = three_criterion_flow().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let mut report = three_criterion_report(&flow);
    report["outcomes"][1]["status"] = json!("insufficientEvidence");
    report["outcomes"][1]["reason"] = json!("本次独立检查尚不能确认守卫的保留完整性。");
    report["outcomes"][1]["evidence"] = json!([]);
    report["outcomes"][1]["issue"] = json!({"issueId":"witness-reading-incomplete","newIssue":{"summary":"守卫保留完整性待核对"},"resolutionOf":[]});
    let (status, saved) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, report.clone(), "legal-three-criterion-negative"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["id"], task["id"]);
    assert_eq!(completed["status"], "completed");
    let stored = f
        .repo()
        .revision_outcome(&f.project(), flow.writer["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.content, report);
    assert_eq!(stored.revision_id, saved["revisionId"]);
    f.settle(&task);
    let (_, decision) = f.decision(&task).await;
    let assessment = decision["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "revisionOutcome")
        .unwrap();
    assert_eq!(assessment["assessment"]["length"]["satisfied"], false);
    assert_eq!(assessment["assessment"]["verdict"], "notMet");
    assert_eq!(decision["canFinish"], false);
}

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
        ("/outcomes/1/issue", Value::Null),
        ("/outcomes/2/issue", Value::Null),
        ("/outcomes/1/issue", json!("未向守卫坦白")),
        ("/outcomes/1/issue/issueId", json!("坦白问题")),
        (
            "/outcomes/1/issue/resolutionOf",
            json!(["invented-observation"]),
        ),
        ("/uncoveredRequirements", json!(["保留告别信"])),
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
    for field in ["uncoveredRequirements", "outcomes"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        invalid.push(missing);
    }
    for index in [0, 1, 2] {
        let mut missing = valid.clone();
        missing["outcomes"][index]
            .as_object_mut()
            .unwrap()
            .remove("issue");
        invalid.push(missing);
    }
    for field in ["issueId", "newIssue", "resolutionOf"] {
        let mut missing = valid.clone();
        missing["outcomes"][1]["issue"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        invalid.push(missing);
    }
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
    for level in [
        "",
        "/outcomes/0",
        "/outcomes/0/evidence/0",
        "/outcomes/1/issue",
        "/outcomes/1/issue/newIssue",
    ] {
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

use novel_codex_story_core::{ProjectRevisionRepository, RevisionOutcomeRepository, TasksRepository};
