use super::{fixture::*, flow::Flow};
use axum::http::StatusCode;
use novel_codex_story_core::{RevisionOutcomeDecision, RevisionOutcomeDecisionRequest};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

pub(super) async fn completed_outcome() -> (Flow, Value) {
    let flow = Flow::queued().await;
    let task = flow.assessment().await;
    let (status, receipt) = flow
        .fixture
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "decision-report"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    let (status, completed) = flow.fixture.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    flow.fixture.settle(&task);
    (flow, receipt)
}

pub(super) fn decision_request(receipt: &Value, id: &str, previous: Value, choice: &str) -> Value {
    json!({"decisionId":id,"expectedAssessmentRevisionId":receipt["revisionId"],
        "expectedDecisionId":previous,"choice":choice,"note":"作者确认当前评估。"})
}

pub(super) async fn decide(flow: &Flow, request: Value) -> (StatusCode, Value) {
    flow.fixture
        .request(
            "POST",
            &format!("{}/decisions", flow.path()),
            AUTHOR,
            request,
        )
        .await
}

pub(super) async fn accept(flow: &Flow, receipt: &Value) -> Value {
    let (status, decision) = decide(
        flow,
        decision_request(receipt, "accept-revision", Value::Null, "acceptRevision"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{decision}");
    decision
}

pub(super) async fn remodel(flow: &Flow) {
    let f = &flow.fixture;
    let (status, batch) = f
        .request(
            "POST",
            "/modeling-batches",
            AUTHOR,
            json!({"objectId":"draft-chapter-one","sourceRevisionId":flow.draft["revisionId"],
            "expectedRevision":0,"extractorVersion":"decision-remodel"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let (_, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    let mut model = f.task_request("modelKnowledge", json!({"batchId":batch["id"]}));
    model["initialBudget"] = json!({"authorizationId":"decision-remodel-budget",
        "authorMessageId":"decision-remodel-author","expectedBaseSnapshotId":workspace["headSnapshotId"],
        "expectedSourceDigest":workspace["sourceDigest"],"maxRequests":2,"maxTokens":1000,
        "deadlineAt":model["capabilityExpiresAt"]});
    let (status, task) = f.request("POST", "/tasks", AUTHOR, model).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let (status, published) = f
        .request(
            "POST",
            &format!(
                "/modeling-batches/{}/publish",
                batch["id"].as_str().unwrap()
            ),
            CANDIDATE,
            json!({"taskId":task["id"],"threadId":task["threadId"],"expectedRevision":0,
            "entities":flow.batch["entities"],"assertions":[]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&task);
}

// Compare every persisted row, including authority, budgets, new decision tables
// and sequence counters. Count-only checks would miss in-place mutations.
pub(super) fn database_snapshot(fixture: &Fixture) -> Vec<(String, Vec<Vec<String>>)> {
    let db = fixture.db();
    let mut tables = db
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap();
    let names = tables
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    names
        .into_iter()
        .map(|name| {
            let identifier = name.replace('"', "\"\"");
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{identifier}\""))
                .unwrap();
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get_ref(index).map(|value| format!("{value:?}")))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows.sort();
            (name, rows)
        })
        .collect()
}

#[tokio::test]
async fn decisions_require_author_and_completed_current_assessment_without_starting_work() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let path = format!("{}/decisions", flow.path());
    let request = decision_request(
        &json!({"revisionId":"missing-report"}),
        "premature",
        Value::Null,
        "acceptRevision",
    );
    let before = database_snapshot(f);
    let (status, error) = decide(&flow, request).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    let (status, receipt) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "running-report"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    let request = decision_request(&receipt, "author-choice", Value::Null, "acceptRevision");
    let before = database_snapshot(f);
    let (status, error) = decide(&flow, request.clone()).await;
    assert!(status.is_client_error(), "Running assessment: {error}");
    assert_eq!(database_snapshot(f), before);
    assert_eq!(
        f.request("POST", &path, CANDIDATE, request.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(database_snapshot(f), before);
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&task);
    let before = database_snapshot(f);
    assert_eq!(
        f.request("POST", &path, CANDIDATE, request.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(database_snapshot(f), before);
    let counts = f.ledger_counts();
    let work = f.work().await;
    let (status, decision) = decide(&flow, request).await;
    assert_eq!(status, StatusCode::CREATED, "{decision}");
    assert!(decision["createdAt"].is_i64(), "{decision}");
    assert_eq!(
        decision,
        json!({"id":"author-choice","writerTaskId":flow.writer["id"],
        "assessmentRevisionId":receipt["revisionId"],"draftRevisionId":flow.draft["revisionId"],
        "knowledgeBatchId":flow.batch["id"],"choice":"acceptRevision",
        "note":"作者确认当前评估。","createdAt":decision["createdAt"]})
    );
    assert_eq!(f.ledger_counts(), counts);
    assert_eq!(f.work().await, work);
    assert_eq!(
        f.request("GET", "/generation-runs", AUTHOR, Value::Null)
            .await,
        (StatusCode::OK, json!({"runs":[]}))
    );
    let (_, report) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(
        report,
        json!({"revisionId":receipt["revisionId"],"authority":"candidateAnalysis",
        "state":"current","assessment":flow.report(),"decisions":[decision],
        "issues":expected_issue_summaries(&flow.draft["revisionId"], "open", 1)})
    );
}

#[tokio::test]
async fn exact_replay_precedes_cas_but_changed_requests_and_stale_cas_roll_back() {
    let (flow, receipt) = completed_outcome().await;
    let request = decision_request(&receipt, "first", Value::Null, "acceptRevision");
    let first = decide(&flow, request.clone()).await;
    assert_eq!(first.0, StatusCode::CREATED, "{:?}", first);
    let second = decide(
        &flow,
        decision_request(&receipt, "second", json!("first"), "repairDraft"),
    )
    .await;
    assert_eq!(second.0, StatusCode::CREATED, "{:?}", second);
    let before = database_snapshot(&flow.fixture);
    assert_eq!(decide(&flow, request.clone()).await, first);
    for (field, value) in [
        ("note", json!("改写已存决定")),
        ("choice", json!("defer")),
        ("expectedDecisionId", json!("second")),
        ("expectedAssessmentRevisionId", json!("other-report")),
    ] {
        let mut changed = request.clone();
        changed[field] = value;
        let (status, error) = decide(&flow, changed).await;
        assert!(status.is_client_error(), "{field}: {error}");
        assert_eq!(database_snapshot(&flow.fixture), before);
    }
    for previous in [Value::Null, json!("first"), json!("missing-decision")] {
        let (status, error) = decide(
            &flow,
            decision_request(&receipt, "stale-cas", previous, "defer"),
        )
        .await;
        assert!(status.is_client_error(), "{error}");
        assert_eq!(database_snapshot(&flow.fixture), before);
    }
    let (status, error) = flow
        .fixture
        .request(
            "POST",
            &format!(
                "/revision-outcomes/{}/decisions",
                flow.model["id"].as_str().unwrap()
            ),
            AUTHOR,
            request,
        )
        .await;
    assert!(status.is_client_error(), "Different writer replay: {error}");
    assert_eq!(database_snapshot(&flow.fixture), before);
    let (_, report) = flow
        .fixture
        .request("GET", &flow.path(), AUTHOR, Value::Null)
        .await;
    assert_eq!(report["decisions"], json!([second.1, first.1]));
}

#[tokio::test]
async fn completed_assessment_only_accepts_its_current_report_revision() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let (status, original) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "superseded-report"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{original}");
    let overview = f
        .repo()
        .revision_outcome(&f.project(), flow.writer["id"].as_str().unwrap())
        .unwrap()
        .unwrap()
        .issues;
    assert_eq!(overview.len(), 2);
    assert!(
        overview
            .iter()
            .all(|issue| issue["observations"] == json!([]))
    );
    let mut report = flow.report();
    report["outcomes"][0]["reason"] = json!("评估解释已经更正，当前正文仍明确藏信。");
    for item in report["outcomes"].as_array_mut().unwrap() {
        if item["issue"].is_null() {
            continue;
        }
        let issue_id = item["issue"]["issueId"].as_str().unwrap().to_owned();
        let observations = f.issue_observations(&task, &issue_id).await;
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0]["reportRevisionId"], original["revisionId"]);
        assert_eq!(observations[0]["criterionId"], item["id"]);
        item["issue"] = json!({"issueId":observations[0]["issueId"],"newIssue":null,
            "resolutionOf":[observations[0]["observationId"]]});
    }
    let mut submission = flow.submission(&task, report.clone(), "replacement-report");
    submission["expectedCandidateRevisionId"] = original["revisionId"].clone();
    let before = database_snapshot(f);
    let mut forged = submission.clone();
    forged["idempotencyKey"] = json!("forged-observation");
    forged["payload"]["outcomes"][1]["issue"]["resolutionOf"] = json!(["invented-observation"]);
    let (status, error) = f.request("POST", &flow.path(), CANDIDATE, forged).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    let (status, replacement) = f.request("POST", &flow.path(), CANDIDATE, submission).await;
    assert_eq!(status, StatusCode::CREATED, "{replacement}");
    for item in report["outcomes"].as_array().unwrap() {
        if item["issue"].is_null() {
            continue;
        }
        let observations = f
            .issue_observations(&task, item["issue"]["issueId"].as_str().unwrap())
            .await;
        assert_eq!(observations.len(), 2);
        let old = observations
            .iter()
            .find(|view| view["reportRevisionId"] == original["revisionId"])
            .unwrap();
        let current = observations
            .iter()
            .find(|view| view["reportRevisionId"] == replacement["revisionId"])
            .unwrap();
        assert_eq!(current["resolutionOf"], json!([old["observationId"]]));
        assert_eq!(old["superseded"], true);
        assert_eq!(old["status"], item["status"]);
    }
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&task);
    let before = database_snapshot(f);
    let (status, error) = decide(
        &flow,
        decision_request(&original, "old-report", Value::Null, "acceptRevision"),
    )
    .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    let decision = accept(&flow, &replacement).await;
    let (_, current) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(
        current,
        json!({"revisionId":replacement["revisionId"],"authority":"candidateAnalysis",
        "state":"current","assessment":report,"decisions":[decision],
        "issues":expected_issue_summaries(&flow.draft["revisionId"], "open", 2)})
    );
}

#[tokio::test]
async fn decision_wire_and_core_enforce_strict_fields_ascii_ids_and_character_bounds() {
    let (flow, receipt) = completed_outcome().await;
    let valid = decision_request(&receipt, &"a".repeat(128), Value::Null, "defer");
    let mut invalid = Vec::new();
    for (field, value) in [
        ("decisionId", json!("")),
        ("decisionId", json!("决定")),
        ("decisionId", json!("a".repeat(129))),
        ("decisionId", json!("bad/id")),
        ("decisionId", json!("bad id")),
        ("decisionId", json!("bad\n")),
        ("note", json!("月".repeat(1001))),
        ("note", Value::Null),
        ("choice", json!("accept")),
        ("unknown", json!(true)),
        ("expectedDecisionId", json!(42)),
        ("expectedDecisionId", json!("")),
        ("expectedDecisionId", json!("决定")),
        ("expectedDecisionId", json!("a".repeat(129))),
        ("expectedDecisionId", json!("bad/id")),
        ("expectedAssessmentRevisionId", json!("")),
        ("expectedAssessmentRevisionId", json!("评估")),
    ] {
        let mut request = valid.clone();
        request[field] = value;
        invalid.push(request);
    }
    for field in [
        "decisionId",
        "expectedAssessmentRevisionId",
        "expectedDecisionId",
        "choice",
        "note",
    ] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        invalid.push(missing);
    }
    let before = database_snapshot(&flow.fixture);
    for request in invalid {
        let (status, error) = decide(&flow, request.clone()).await;
        assert!(status.is_client_error(), "{error}");
        if let Ok(request) = serde_json::from_value::<RevisionOutcomeDecisionRequest>(request) {
            assert!(
                flow.fixture
                    .repo()
                    .decide_revision_outcome(
                        &flow.fixture.project(),
                        flow.writer["id"].as_str().unwrap(),
                        request,
                    )
                    .is_err()
            );
        }
        assert_eq!(database_snapshot(&flow.fixture), before);
    }
    let mut valid = valid;
    valid["note"] = json!("月".repeat(1000));
    let request: RevisionOutcomeDecisionRequest = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(request.decision_id, "a".repeat(128));
    assert_eq!(
        request.expected_assessment_revision_id,
        receipt["revisionId"].as_str().unwrap()
    );
    assert_eq!(request.expected_decision_id, None);
    assert_eq!(request.note, "月".repeat(1000));
    let decision = flow
        .fixture
        .repo()
        .decide_revision_outcome(
            &flow.fixture.project(),
            flow.writer["id"].as_str().unwrap(),
            request,
        )
        .unwrap();
    let encoded = serde_json::to_value(decision).unwrap();
    assert_eq!(encoded["note"], valid["note"]);
    let (status, replay) = decide(&flow, valid.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{replay}");
    assert_eq!(replay, encoded);
    let mut unknown_request = valid;
    unknown_request["unknown"] = json!(true);
    assert!(serde_json::from_value::<RevisionOutcomeDecisionRequest>(unknown_request).is_err());
    let mut unknown_decision = encoded;
    unknown_decision["unknown"] = json!(true);
    assert!(serde_json::from_value::<RevisionOutcomeDecision>(unknown_decision).is_err());
}

#[tokio::test]
async fn stale_sources_reject_new_decisions_but_preserve_history_and_exact_replay() {
    for source in ["draft", "knowledge"] {
        let (flow, receipt) = completed_outcome().await;
        let f = &flow.fixture;
        let request = decision_request(&receipt, "frozen", Value::Null, "acceptRevision");
        let saved = decide(&flow, request.clone()).await;
        assert_eq!(saved.0, StatusCode::CREATED, "{:?}", saved);
        let mut current_draft = flow.draft["revisionId"].clone();
        if source == "draft" {
            let mut next = f.original.clone();
            next["sceneGroups"][1]["body"] = json!("她将密函交给守卫。");
            join_body(&mut next);
            let (status, edited) = f
                .checkpoint(None, &flow.draft, next, "decision-stale-draft")
                .await;
            assert_eq!(status, StatusCode::CREATED, "{edited}");
            current_draft = edited["revisionId"].clone();
        } else {
            remodel(&flow).await;
        }
        let before = database_snapshot(f);
        let (status, error) = decide(
            &flow,
            decision_request(&receipt, "after-stale", json!("frozen"), "defer"),
        )
        .await;
        assert!(status.is_client_error(), "{source}: {error}");
        assert_eq!(decide(&flow, request).await, saved);
        assert_eq!(database_snapshot(f), before);
        let (status, report) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{source}: {report}");
        assert_eq!(
            report,
            json!({"revisionId":receipt["revisionId"],"authority":"candidateAnalysis",
            "state":"stale","assessment":flow.report(),"decisions":[saved.1],
            "issues":expected_issue_summaries(&current_draft, "stale", 1)})
        );
    }
}

#[tokio::test]
async fn report_bounds_decision_history_to_twenty_newest_without_losing_replay_receipts() {
    let (flow, receipt) = completed_outcome().await;
    let mut previous = Value::Null;
    let mut decisions = Vec::new();
    let first_request = decision_request(&receipt, "history-0", previous.clone(), "defer");
    for index in 0..22 {
        let id = format!("history-{index}");
        let (status, decision) =
            decide(&flow, decision_request(&receipt, &id, previous, "defer")).await;
        assert_eq!(status, StatusCode::CREATED, "{decision}");
        previous = json!(id);
        decisions.push(decision);
    }
    let (_, report) = flow
        .fixture
        .request("GET", &flow.path(), AUTHOR, Value::Null)
        .await;
    assert_eq!(
        report["decisions"],
        json!(decisions.iter().rev().take(20).collect::<Vec<_>>())
    );
    let before = database_snapshot(&flow.fixture);
    let (status, replay) = decide(&flow, first_request).await;
    assert_eq!(status, StatusCode::CREATED, "{replay}");
    assert_eq!(replay, decisions[0]);
    assert_eq!(database_snapshot(&flow.fixture), before);
}

#[tokio::test]
async fn historical_report_requires_published_provenance_even_after_head_replacement() {
    let (flow, _) = completed_outcome().await;
    let f = &flow.fixture;
    remodel(&flow).await;
    let (status, report) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["state"], "stale");

    // A published batch may lose its current head, but an unpublished batch
    // must never become valid historical evidence merely by sharing its IDs.
    f.db()
        .execute(
            "UPDATE story_modeling_batches_v2 SET published_at=NULL WHERE batch_id=?1",
            [flow.batch["id"].as_str().unwrap()],
        )
        .unwrap();
    let before = database_snapshot(f);
    let (status, error) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
}

use novel_codex_story_core::{RevisionOutcomeDecisionRepository, RevisionOutcomeRepository};
