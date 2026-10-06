use super::{decision_tests::*, fixture::*};
use axum::http::StatusCode;
use eternallight_engine_core::PersistentBudget;
use novel_codex_story_core::RevisionPredecessor;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn predecessor(flow: &super::flow::Flow, receipt: &Value, decision: &Value) -> Value {
    json!({"writerTaskId":flow.writer["id"],"assessmentRevisionId":receipt["revisionId"],
        "decisionId":decision["id"]})
}

async fn start_request(f: &Fixture, origin: Value) -> Value {
    let (status, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    json!({"expectedProjectRevision":workspace["revision"],
        "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
        "authorIntent":"作者另行确认：依照当前正文继续一章，保持本次预算。",
        "targetChapters":1,"maxTaskAttempts":8,"maxProviderRequests":80,
        "maxProviderTokens":200_000,"deadlineAt":deadline,"revisionPredecessor":origin})
}

fn run_path(run: &Value) -> String {
    format!("/generation-runs/{}", run["id"].as_str().unwrap())
}

async fn next_action(f: &Fixture, run: &Value) -> Value {
    let (status, next) = f
        .request(
            "POST",
            &format!("{}/next", run_path(run)),
            AUTHOR,
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["kind"], "agentTask", "{next}");
    next["action"].clone()
}

// Generation always asks the coordinator to choose the next authorized action.
// Use the public protocol here so provenance and finite budgets cover the
// coordinator as well as the selected plan/writer task.
async fn settle_and_advance(f: &Fixture, run: &Value, task: &Value, grant: &Value) -> Value {
    let mut budget = PersistentBudget::new(
        &f.path().with_extension("engine.sqlite3"),
        task["id"].as_str().unwrap(),
        json!({"options":{"id":grant["budgetId"],
            "maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000},
            "now":1000}),
    )
    .unwrap();
    budget.complete("completed").unwrap();
    let (status, charged) = f
        .request(
            "POST",
            &format!("{}/provider-usage", run_path(run)),
            AUTHOR,
            json!({"taskId":task["id"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    let (status, advanced) = f
        .request(
            "POST",
            &format!("{}/advance-task", run_path(run)),
            AUTHOR,
            json!({"expectedVersion":charged["version"],"taskId":task["id"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{advanced}");
    advanced
}

async fn select_action(f: &Fixture, run: &Value, operation: &str) -> Value {
    let action = next_action(f, run).await;
    assert_eq!(action["operation"], "advisory");
    let (_, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    let mut request = f.task_request("advisory", action["target"].clone());
    request["inputRevision"] = workspace["revision"].clone();
    let (status, parent) = f.request("POST", "/tasks", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{parent}");
    let grant = f.grant(&parent).await;
    let id = parent["id"].as_str().unwrap();
    let thread = parent["threadId"].as_str().unwrap();
    let revision = parent["inputRevision"].as_i64().unwrap();
    let (status, read) = f
        .request(
            "GET",
            &format!("/tasks/{id}/work-decision?threadId={thread}&expectedRevision={revision}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    let key = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["operation"] == operation)
        .unwrap_or_else(|| panic!("missing {operation}: {read}"))["actionKey"]
        .clone();
    let (status, selected) = f
        .request(
            "POST",
            &format!("/tasks/{id}/work-decision/actions"),
            CANDIDATE,
            json!({"taskId":id,"threadId":thread,"expectedRevision":revision,
            "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
            "actionKey":key,"instruction":"按作者授权继续一章，保持当前预算。",
            "idempotencyKey":format!("choose-{operation}")}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    let (status, completed) = f.complete(&parent).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    settle_and_advance(f, run, &parent, &grant).await;
    next_action(f, run).await
}

#[tokio::test]
async fn accepted_origin_requires_separate_author_start_and_fresh_finite_budget() {
    let (flow, receipt) = completed_outcome().await;
    let f = &flow.fixture;
    let old_budget = flow.grants[1].clone();
    let work = f.work().await;
    let counts = f.ledger_counts();
    let decision = accept(&flow, &receipt).await;
    assert_eq!(f.ledger_counts(), counts);
    assert_eq!(f.work().await, work);
    assert_eq!(
        f.request("GET", "/generation-runs", AUTHOR, Value::Null)
            .await,
        (StatusCode::OK, json!({"runs":[]}))
    );
    let origin = predecessor(&flow, &receipt, &decision);
    let typed: RevisionPredecessor = serde_json::from_value(origin.clone()).unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), origin);
    let request = start_request(f, origin.clone()).await;
    let before = database_snapshot(f);
    let (status, error) = f
        .request("POST", "/generation-runs", CANDIDATE, request.clone())
        .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    for field in [
        "authorIntent",
        "maxTaskAttempts",
        "maxProviderRequests",
        "maxProviderTokens",
        "deadlineAt",
    ] {
        let mut missing = request.clone();
        missing.as_object_mut().unwrap().remove(field);
        let (status, error) = f.request("POST", "/generation-runs", AUTHOR, missing).await;
        assert!(status.is_client_error(), "{field}: {error}");
        assert_eq!(database_snapshot(f), before);
    }
    let (status, run) = f
        .request("POST", "/generation-runs", AUTHOR, request.clone())
        .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["revisionPredecessor"], origin);
    let typed: novel_codex_story_authoring::GenerationRun =
        serde_json::from_value(run.clone()).unwrap();
    assert_eq!(
        typed.revision_predecessor,
        Some(serde_json::from_value(origin.clone()).unwrap())
    );
    assert_eq!(run["startChapterNumber"], 2);
    assert_eq!(run["targetChapters"], 2);
    assert_eq!(run["maxTaskAttempts"], request["maxTaskAttempts"]);
    assert_eq!(run["deadlineAt"], request["deadlineAt"]);
    assert_eq!(
        run["providerBudget"],
        json!({"maxRequests":80,"maxTokens":200_000,
        "usedRequests":0,"usedTokens":0,"unknownTokens":0,"overrun":false,"charges":[]})
    );
    assert_eq!(
        run["intentRevisions"][0]["instruction"],
        request["authorIntent"]
    );
    assert_ne!(
        run["authorMessageId"],
        flow.writer["inputManifest"]["authorMessageId"]
    );
    let stored = f
        .repo()
        .generation_run(&f.project(), run["id"].as_str().unwrap())
        .unwrap();
    let stored: Value = serde_json::from_str(&stored.state_json).unwrap();
    assert_eq!(stored, run);
    let action = next_action(f, &run).await;
    let (status, task) = f
        .create_task(
            action["operation"].as_str().unwrap(),
            action["target"].clone(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let grant = f.grant(&task).await;
    assert_ne!(grant["workTaskId"], old_budget["workTaskId"]);
    assert_ne!(grant["budgetId"], old_budget["budgetId"]);
    assert_eq!(
        grant["workTaskId"],
        json!(format!("work-{}", run["id"].as_str().unwrap()))
    );
    assert_eq!(f.work().await, work);
}

#[tokio::test]
async fn invalid_unaccepted_or_cross_project_origins_reject_without_partial_authority() {
    let (flow, receipt) = completed_outcome().await;
    let f = &flow.fixture;
    let decision = accept(&flow, &receipt).await;
    let origin = predecessor(&flow, &receipt, &decision);
    let request = start_request(f, origin.clone()).await;
    let before = database_snapshot(f);
    let mut invalid = Vec::new();
    for (field, value) in [
        ("writerTaskId", flow.model["id"].clone()),
        ("assessmentRevisionId", flow.draft["revisionId"].clone()),
        ("decisionId", json!("missing-decision")),
        ("unknown", json!(true)),
    ] {
        let mut forged = request.clone();
        forged["revisionPredecessor"][field] = value;
        invalid.push(forged);
    }
    for field in ["writerTaskId", "assessmentRevisionId", "decisionId"] {
        let mut missing = request.clone();
        missing["revisionPredecessor"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        invalid.push(missing);
    }
    for forged in invalid {
        let (status, error) = f.request("POST", "/generation-runs", AUTHOR, forged).await;
        assert!(status.is_client_error(), "{error}");
        assert_eq!(database_snapshot(f), before);
    }
    let (status, other) = call(
        &f.app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"title":"其他作品"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{other}");
    let root = format!("/api/projects/{}", other["project"]["id"].as_str().unwrap());
    let (_, workspace) = call(
        &f.app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await;
    let mut foreign = request.clone();
    foreign["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
    foreign["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    let before = database_snapshot(f);
    let (status, error) = call(
        &f.app,
        "POST",
        &format!("{root}/generation-runs"),
        AUTHOR,
        foreign,
    )
    .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    let mut previous = decision["id"].clone();
    for choice in ["repairDraft", "defer"] {
        let (status, unaccepted) =
            decide(&flow, decision_request(&receipt, choice, previous, choice)).await;
        assert_eq!(status, StatusCode::CREATED, "{unaccepted}");
        previous = unaccepted["id"].clone();
        let mut unaccepted_request = request.clone();
        unaccepted_request["revisionPredecessor"] = predecessor(&flow, &receipt, &unaccepted);
        let before = database_snapshot(f);
        for body in [unaccepted_request, request.clone()] {
            let (status, error) = f.request("POST", "/generation-runs", AUTHOR, body).await;
            assert!(status.is_client_error(), "{error}");
            assert_eq!(database_snapshot(f), before);
        }
    }
}

#[tokio::test]
async fn origin_registration_failure_rolls_back_run_budget_messages_and_scope() {
    let (flow, receipt) = completed_outcome().await;
    let decision = accept(&flow, &receipt).await;
    let f = &flow.fixture;
    let request = start_request(f, predecessor(&flow, &receipt, &decision)).await;
    f.db().execute_batch("CREATE TRIGGER reject_revision_origin BEFORE INSERT ON story_generation_revision_sources_v1
        BEGIN SELECT RAISE(ABORT,'origin persistence failure'); END;").unwrap();
    let before = database_snapshot(f);
    let (status, error) = f
        .request("POST", "/generation-runs", AUTHOR, request.clone())
        .await;
    assert!(status.is_server_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    f.db()
        .execute_batch("DROP TRIGGER reject_revision_origin;")
        .unwrap();
    let (status, run) = f.request("POST", "/generation-runs", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(
        run["revisionPredecessor"],
        predecessor(&flow, &receipt, &decision)
    );
}

#[tokio::test]
async fn decision_changed_before_first_claim_blocks_the_previously_offered_action() {
    let (flow, receipt) = completed_outcome().await;
    let decision = accept(&flow, &receipt).await;
    let f = &flow.fixture;
    let request = start_request(f, predecessor(&flow, &receipt, &decision)).await;
    let (status, run) = f.request("POST", "/generation-runs", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let action = next_action(f, &run).await;
    let (status, changed) = decide(
        &flow,
        decision_request(&receipt, "before-claim", decision["id"].clone(), "defer"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{changed}");
    let before = database_snapshot(f);
    let (status, error) = f
        .create_task(
            action["operation"].as_str().unwrap(),
            action["target"].clone(),
        )
        .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    assert_eq!(
        f.request("GET", &run_path(&run), AUTHOR, Value::Null).await,
        (StatusCode::OK, run)
    );
}

#[tokio::test]
async fn stale_source_blocks_creation_and_first_claim_but_keeps_the_frozen_relation_readable() {
    let (flow, receipt) = completed_outcome().await;
    let decision = accept(&flow, &receipt).await;
    let f = &flow.fixture;
    let request = start_request(f, predecessor(&flow, &receipt, &decision)).await;
    let (status, run) = f.request("POST", "/generation-runs", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let action = next_action(f, &run).await;
    // The current chapter remains ready for ordinary generation, but the report
    // refers to a superseded knowledge batch.
    remodel(&flow).await;
    let (status, report) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["state"], "stale");
    assert_eq!(report["decisions"], json!([decision]));
    let before = database_snapshot(f);
    let (status, error) = f
        .create_task(
            action["operation"].as_str().unwrap(),
            action["target"].clone(),
        )
        .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
    assert_eq!(
        f.request("GET", &run_path(&run), AUTHOR, Value::Null).await,
        (StatusCode::OK, run.clone())
    );
    let (status, cancelled) = f
        .request(
            "POST",
            &format!("{}/control", run_path(&run)),
            AUTHOR,
            json!({"expectedVersion":run["version"],"command":"cancel"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    let request = start_request(f, predecessor(&flow, &receipt, &decision)).await;
    let before = database_snapshot(f);
    let (status, error) = f.request("POST", "/generation-runs", AUTHOR, request).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(database_snapshot(f), before);
}

#[tokio::test]
async fn origin_and_author_bounds_are_immutable_and_history_survives_own_progress() {
    let (flow, receipt) = completed_outcome().await;
    let decision = accept(&flow, &receipt).await;
    let f = &flow.fixture;
    let origin = predecessor(&flow, &receipt, &decision);
    let request = start_request(f, origin.clone()).await;
    let (status, run) = f.request("POST", "/generation-runs", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let mut repo = f.repo();
    for (pointer, value) in [
        ("/revisionPredecessor", Value::Null),
        ("/revisionPredecessor/decisionId", json!("another-decision")),
        (
            "/revisionPredecessor/writerTaskId",
            flow.model["id"].clone(),
        ),
        (
            "/revisionPredecessor/assessmentRevisionId",
            flow.draft["revisionId"].clone(),
        ),
        ("/maxTaskAttempts", json!(9)),
        ("/targetChapters", json!(3)),
        ("/providerBudget/maxRequests", json!(81)),
        ("/providerBudget/maxTokens", json!(200_001)),
    ] {
        let mut forged = run.clone();
        *forged.pointer_mut(pointer).unwrap() = value;
        forged["version"] = json!(run["version"].as_i64().unwrap() + 1);
        let before = database_snapshot(f);
        assert!(
            repo.update_generation_run(
                &f.project(),
                run["id"].as_str().unwrap(),
                run["version"].as_i64().unwrap(),
                &forged.to_string()
            )
            .is_err(),
            "{pointer}"
        );
        assert_eq!(database_snapshot(f), before);
    }
    let action = select_action(f, &run, "generatePlan").await;
    assert_eq!(action["operation"], "generatePlan");
    let (status, task) = f
        .create_task(
            action["operation"].as_str().unwrap(),
            action["target"].clone(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let grant = f.grant(&task).await;
    let (status, changed) = decide(
        &flow,
        decision_request(&receipt, "after-claim", decision["id"].clone(), "defer"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{changed}");
    // The run now changes its own inputs. The second claim must retain the first
    // accepted provenance even though the latest author decision is different.
    let (status, plan) = f.request("POST", "/artifacts/plan", CANDIDATE,
        json!({"taskId":task["id"],"threadId":task["threadId"],"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":"continued-plan",
            "payload":{"chapters":[{"chapterPlanId":"continued-plan-2","chapterNumber":2,
                "goal":"林夕藏好密函后继续走向白塔。","scenes":[{"id":"continued-scene-2",
                    "goal":"抵达白塔","beats":[],"expectedEffects":[{"id":"continued-effect-2",
                        "dimension":"narrative","description":"林夕抵达白塔。","commitmentRefs":[]}]}]}]}})).await;
    assert_eq!(status, StatusCode::CREATED, "{plan}");
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let advanced = settle_and_advance(f, &run, &task, &grant).await;
    assert_eq!(advanced["revisionPredecessor"], origin);
    assert_ne!(advanced["sourceDigest"], run["sourceDigest"]);
    assert_eq!(
        advanced["providerBudget"]["maxRequests"],
        run["providerBudget"]["maxRequests"]
    );
    assert_eq!(advanced["maxTaskAttempts"], run["maxTaskAttempts"]);
    assert_eq!(advanced["stage"], "coordinate");
    let action = select_action(f, &advanced, "writeNextChapter").await;
    assert_eq!(action["operation"], "generateChapter");
    let (_, workspace) = f
        .request("GET", "/creative-workspace", AUTHOR, Value::Null)
        .await;
    let mut task_request = f.task_request(
        action["operation"].as_str().unwrap(),
        action["target"].clone(),
    );
    task_request["inputRevision"] = workspace["revision"].clone();
    let (status, next) = f.request("POST", "/tasks", AUTHOR, task_request).await;
    assert_eq!(status, StatusCode::CREATED, "{next}");
    let (_, persisted) = f.request("GET", &run_path(&run), AUTHOR, Value::Null).await;
    assert_eq!(persisted["revisionPredecessor"], origin);
    assert_eq!(persisted["usedTaskAttempts"], 4);
    let (_, report) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(report["decisions"], json!([changed, decision]));
    let (_, history) = f
        .request("GET", "/generation-runs", AUTHOR, Value::Null)
        .await;
    assert_eq!(history["runs"], json!([persisted]));
}

#[tokio::test]
async fn ordinary_starts_with_omitted_or_null_origin_create_no_relation() {
    for omit in [true, false] {
        let f = Fixture::new().await;
        // Use an empty second project so ordinary generation needs no modeled chapter.
        let (status, project) = call(
            &f.app,
            "POST",
            "/api/projects",
            AUTHOR,
            json!({"title":"普通创作"}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{project}");
        let root = format!(
            "/api/projects/{}",
            project["project"]["id"].as_str().unwrap()
        );
        let (_, workspace) = call(
            &f.app,
            "GET",
            &format!("{root}/creative-workspace"),
            AUTHOR,
            Value::Null,
        )
        .await;
        let mut request = start_request(&f, Value::Null).await;
        request["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
        request["expectedSourceDigest"] = workspace["sourceDigest"].clone();
        if omit {
            request
                .as_object_mut()
                .unwrap()
                .remove("revisionPredecessor");
        }
        let (status, run) = call(
            &f.app,
            "POST",
            &format!("{root}/generation-runs"),
            AUTHOR,
            request,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        assert_eq!(run.get("revisionPredecessor"), Some(&Value::Null));
        let relations: i64 = f
            .db()
            .query_row(
                "SELECT COUNT(*) FROM story_generation_revision_sources_v1 WHERE run_id=?1",
                [run["id"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relations, 0);
    }
}
