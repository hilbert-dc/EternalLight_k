use super::{
    fixture::*,
    flow::{Flow, Goals},
};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn approved_revision_models_then_assesses_under_one_author_budget_and_keeps_stale_history() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let work = f.work().await;
    let model_step = step(&work, "modelKnowledge");
    let outcome_step = step(&work, "assessRevisionOutcome");
    assert_eq!(
        outcome_step["target"],
        json!({"writerTaskId":flow.writer["id"],
        "chapterId":"chapter-one","chapterNumber":1,"originalRevisionId":f.original["revisionId"],
        "draftRevisionId":flow.draft["revisionId"],"knowledgeBatchId":flow.batch["id"]})
    );
    assert_eq!(
        (
            outcome_step["dependsOnStepId"].clone(),
            outcome_step["status"].clone(),
            outcome_step["expectedOutputKind"].clone()
        ),
        (
            model_step["id"].clone(),
            json!("queued"),
            json!("revisionOutcome")
        )
    );
    let task = flow.assessment().await;
    assert_eq!(task["target"], flow.assessment_target().await);
    assert_eq!(task["inputManifest"]["authorIntent"], INTENT);
    assert_eq!(
        task["inputManifest"]["authorIntentBinding"],
        json!({"sourceTaskId":flow.writer["id"],"intentRevision":0,"actionId":null})
    );
    let root_grant = &flow.grants[0];
    for grant in flow
        .grants
        .iter()
        .skip(1)
        .cloned()
        .chain([f.grant(&task).await])
    {
        let mut expected = root_grant.clone();
        expected["taskId"] = grant["taskId"].clone();
        expected["budgetId"] = json!(format!("budget-{}", grant["taskId"].as_str().unwrap()));
        assert_eq!(grant, expected);
    }
    let input = f
        .repo()
        .revision_outcome_input(
            &f.repo()
                .task(&f.project(), task["id"].as_str().unwrap())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        input,
        json!({"desiredOutcomes":desired_outcomes(),
        "beforeScenes":[{"groupId":"letter","body":"她烧掉密函。","truncated":false},
            {"groupId":"witness","body":"守卫沉默。","truncated":false}],
        "afterScenes":[{"groupId":"letter","body":NEW_BODY,"truncated":false},
            {"groupId":"witness","body":"守卫沉默。","truncated":false}],
        "coverage":{"complete":true,"textByteLimit":12_000}})
    );
    let (status, context) = f.request("GET", &format!(
        "/context?taskId={}&threadId=assessRevisionOutcome&projectRevision=0&purpose=critic&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["operationContext"]["revisionOutcomeInput"], input);
    assert_eq!(context["relevantText"], json!([]));
    let relations_path = format!(
        "/tasks/{}/relations?threadId=assessRevisionOutcome&expectedRevision=0&entityId=letter",
        task["id"].as_str().unwrap()
    );
    let (status, relations) = f
        .request("GET", &relations_path, CANDIDATE, Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK, "{relations}");
    assert_eq!(
        (
            relations["scope"].clone(),
            relations["visibleThroughChapter"].clone()
        ),
        (json!("revisionOutcome"), json!(1))
    );
    let (status, denied) = f
        .request(
            "GET",
            &format!("{relations_path}&snapshotId=snapshot-override"),
            CANDIDATE,
            Value::Null,
        )
        .await;
    assert_eq!(
        (status, denied["error"]["code"].clone()),
        (
            StatusCode::BAD_REQUEST,
            json!("historical_relations_forbidden")
        )
    );
    for path in [
        "/knowledge/relations?entityId=letter",
        "/creative-workspace",
    ] {
        assert_eq!(
            f.request("GET", path, CANDIDATE, Value::Null).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let report = flow.report();
    assert_eq!(
        f.request("GET", &flow.path(), AUTHOR, Value::Null).await,
        (StatusCode::OK, Value::Null)
    );
    let request = flow.submission(&task, report.clone(), "report");
    let before = f.ledger_counts();
    let (status, receipt) = f
        .request("POST", &flow.path(), CANDIDATE, request.clone())
        .await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    let after = f.ledger_counts();
    assert_eq!(
        f.request("POST", &flow.path(), CANDIDATE, request).await,
        (StatusCode::CREATED, receipt.clone())
    );
    assert_eq!(f.ledger_counts(), after);
    assert_ne!(after, before);
    let (status, completed) = f.complete(&task).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(
        completed["outputRefs"],
        json!([{"kind":"revisionOutcome",
        "revisionId":receipt["revisionId"],
        "objectId":format!("revision-outcome-{}",flow.writer["id"].as_str().unwrap())}])
    );
    f.settle(&task);
    let expected = json!({"revisionId":receipt["revisionId"],"authority":"candidateAnalysis",
        "state":"current","assessment":report,"decisions":[]});
    assert_eq!(
        f.request("GET", &flow.path(), AUTHOR, Value::Null).await,
        (StatusCode::OK, expected.clone())
    );
    let heads = f.repo().object_heads(&f.project()).unwrap();
    let object = format!("revision-outcome-{}", flow.writer["id"].as_str().unwrap());
    let head = heads
        .iter()
        .find(|head| head.object_id.as_str() == object)
        .unwrap();
    assert_eq!(
        (
            head.object_kind.as_str(),
            head.accepted_revision_id.as_ref()
        ),
        ("revisionOutcome", None)
    );
    let before = f.ledger_counts();
    for kind in ["revisionOutcome", "draft"] {
        let (status, error) = f
            .request(
                "POST",
                "/reviews",
                AUTHOR,
                json!({"artifactKind":kind,"revisionId":receipt["revisionId"],
                "idempotencyKey":format!("cannot-adopt-{kind}"),"proposals":[],"selections":{}}),
            )
            .await;
        assert!(
            status.is_client_error(),
            "Report cannot enter Canon review: {error}"
        );
        assert_eq!(f.ledger_counts(), before);
        assert_eq!(f.repo().object_heads(&f.project()).unwrap(), heads);
    }
    let mut next = f.original.clone();
    next["sceneGroups"][1]["body"] = json!("她又烧掉密函。");
    join_body(&mut next);
    let (status, edited) = f
        .checkpoint(None, &flow.draft, next, "later-author-edit")
        .await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    let mut stale = expected;
    stale["state"] = json!("stale");
    assert_eq!(
        f.request("GET", &flow.path(), AUTHOR, Value::Null).await,
        (StatusCode::OK, stale)
    );
    let historical = f
        .repo()
        .revision_outcome(&f.project(), flow.writer["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            historical.revision_id,
            historical.content,
            historical.current
        ),
        (
            receipt["revisionId"].as_str().unwrap().to_owned(),
            report,
            false
        )
    );
}

#[tokio::test]
async fn large_unicode_scene_inputs_have_explicit_partial_coverage_with_complete_approved_criteria()
{
    let f = Fixture::with_letter(&"潮".repeat(5000)).await;
    let flow = Flow::from_fixture(f, Goals::Approved).await;
    let f = &flow.fixture;
    let (status, completed) = f.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&flow.model);
    let task = flow.assessment().await;
    let input = f
        .repo()
        .revision_outcome_input(
            &f.repo()
                .task(&f.project(), task["id"].as_str().unwrap())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(input["desiredOutcomes"], desired_outcomes());
    assert_eq!(
        input["coverage"],
        json!({"complete":false,"textByteLimit":12_000})
    );
    let scenes = input["beforeScenes"]
        .as_array()
        .unwrap()
        .iter()
        .chain(input["afterScenes"].as_array().unwrap());
    let text_bytes: usize = scenes
        .clone()
        .map(|scene| scene["body"].as_str().unwrap().len())
        .sum();
    assert!(text_bytes <= 12_000, "{text_bytes}");
    assert!(scenes.clone().any(|scene| scene["truncated"] == true));
    assert!(
        scenes
            .clone()
            .all(|scene| scene["groupId"] == "letter" || scene["groupId"] == "witness")
    );
    let prefix = input["beforeScenes"][0]["body"].as_str().unwrap();
    assert!("潮".repeat(5000).starts_with(prefix));
}

#[tokio::test]
async fn writer_without_goals_finishes_modeling_without_an_assessment_step() {
    let flow = Flow::modeled(Goals::Absent).await;
    let (status, completed) = flow.fixture.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let work = flow.fixture.work().await;
    assert_eq!(work["status"], "completed");
    assert_eq!(
        work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step["operation"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("advisory"),
            json!("generateChapter"),
            json!("modelKnowledge")
        ]
    );
    let count: i64 = flow
        .fixture
        .db()
        .query_row(
            "SELECT COUNT(*) FROM story_revisions_v2 WHERE object_kind='revisionOutcome'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
