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
    let outcome_step = step(&work, "assessRevisionOutcome");
    let mut literary_target = outcome_step["target"].clone();
    let binding = literary_target
        .as_object_mut()
        .unwrap()
        .remove("workDecisionAction")
        .unwrap();
    assert_eq!(binding["workTaskId"], work["id"]);
    assert_eq!(
        literary_target,
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
            binding["decisionStepId"].clone(),
            json!("queued"),
            json!("revisionOutcome")
        )
    );
    let task = flow.assessment().await;
    assert_eq!(task["target"], flow.assessment_target().await);
    assert_eq!(task["inputManifest"]["authorIntent"], INTENT);
    assert_eq!(
        task["inputManifest"]["authorIntentBinding"],
        json!({"sourceTaskId":f.discussion["id"],"intentRevision":0,"actionId":null})
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
        "coverage":{"complete":true,"delivery":"taskSourcePages"}})
    );
    let (status, context) = f.request("GET", &format!(
        "/context?taskId={}&threadId=assessRevisionOutcome&projectRevision=0&purpose=critic&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        read_context_source(f, &task, &context, "/operationContext/revisionOutcomeInput").await,
        input
    );
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
async fn large_unicode_scene_inputs_page_complete_originals_and_approved_criteria() {
    let f = Fixture::with_letter(&"潮".repeat(5000)).await;
    let flow = Flow::from_fixture(f, Goals::Approved).await;
    let f = &flow.fixture;
    let (status, completed) = f.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&flow.model);
    f.choose(&flow.model, "assessRevisionOutcome").await;
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
    let (status, pack) = f.request("GET", &format!(
        "/context?taskId={}&threadId={}&projectRevision=0&purpose=critic&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap(), task["threadId"].as_str().unwrap()), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(
        read_context_source(f, &task, &pack, "/operationContext/revisionOutcomeInput").await,
        input
    );
    assert_eq!(pack["coverage"]["complete"], false);
    assert_eq!(input["desiredOutcomes"], desired_outcomes());
    assert_eq!(
        input["coverage"],
        json!({"complete":true,"delivery":"taskSourcePages"})
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
    assert!(text_bytes > 15_000, "{text_bytes}");
    assert!(scenes.clone().all(|scene| scene["truncated"] == false));
    assert!(
        scenes
            .clone()
            .all(|scene| scene["groupId"] == "letter" || scene["groupId"] == "witness")
    );
    assert_eq!(input["beforeScenes"][0]["body"], "潮".repeat(5000));
    assert_eq!(input["afterScenes"][0]["body"], NEW_BODY);
    for side in ["beforeScenes", "afterScenes"] {
        assert_eq!(input[side].as_array().unwrap().len(), 2);
        assert_eq!(input[side][1]["body"], "守卫沉默。");
    }
}

#[tokio::test]
async fn policy_selected_optional_modeling_can_finish_without_an_assessment_step() {
    let flow = Flow::modeled(Goals::Absent).await;
    let (status, completed) = flow.fixture.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    flow.fixture.settle(&flow.model);
    let (parent, read) = flow.fixture.decision(&flow.model).await;
    assert_eq!(read["canFinish"], true);
    assert_eq!(flow.fixture.complete(&parent).await.0, StatusCode::OK);
    flow.fixture.settle(&parent);
    let work = flow.fixture.work().await;
    assert_eq!(work["status"], "completed");
    assert_eq!(
        work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| step["operation"] != "advisory")
            .map(|step| step["operation"].clone())
            .collect::<Vec<_>>(),
        vec![json!("generateChapter"), json!("modelKnowledge")]
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

#[tokio::test]
async fn policy_ordinary_revision_can_finish_without_modeling_and_explicit_outcomes_require_only_assessment()
 {
    for (approved_outcomes, manual_same_chapter) in [(false, false), (false, true), (true, false)] {
        let f = Fixture::new().await;
        let mut target = f.target.clone();
        if !approved_outcomes {
            target.as_object_mut().unwrap().remove("desiredOutcomes");
        }
        let writer = f.writer(target).await;
        let mut output = f.original.clone();
        output["sceneGroups"][1]["body"] = json!(NEW_BODY);
        join_body(&mut output);
        let (code, draft) = f
            .checkpoint(Some(&writer), &f.original, output, "saved-revision")
            .await;
        assert_eq!(code, StatusCode::CREATED, "{draft}");
        assert_eq!(f.complete(&writer).await.0, StatusCode::OK);
        f.settle(&writer);
        let (parent, read) = f.decision(&writer).await;
        assert_eq!(read["canFinish"], !approved_outcomes, "{read}");
        let knowledge = read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["operation"] == "modelKnowledge")
            .unwrap();
        assert_eq!(knowledge["required"], false);
        assert_eq!(
            read["requirements"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == "knowledge" && r["status"] == "pending"),
            false
        );
        let receipt = &read["receipts"]["items"][0]["outputs"][0];
        assert_eq!(receipt["chapterId"], "chapter-one");
        assert_eq!(receipt["revisionId"], draft["revisionId"]);
        let (code, result) = f.complete(&parent).await;
        assert_eq!(code.is_success(), !approved_outcomes, "{result}");
        if !approved_outcomes {
            f.settle(&parent);
            let work = f.work().await;
            assert_eq!(work["status"], "completed");
            assert!(
                !work["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|step| step["operation"] == "modelKnowledge")
            );
            let pending: (String, String) = f.db().query_row(
                "SELECT status,source_revision_id FROM story_modeling_outbox_v1 WHERE object_id='draft-chapter-one'", [],
                |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
            assert_eq!(
                pending,
                (
                    "pending".into(),
                    draft["revisionId"].as_str().unwrap().into()
                )
            );
            // A later manual edit remains source-tracked without creating model work.
            let tasks_before: i64 = f
                .db()
                .query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |row| row.get(0))
                .unwrap();
            let (manual, previous, chapter) = if manual_same_chapter {
                let mut manual = f.original.clone();
                manual["sceneGroups"][0]["body"] = json!("林夕走过港口。");
                manual["sceneGroups"][1]["body"] = json!(NEW_BODY);
                join_body(&mut manual);
                (manual, draft.clone(), "chapter-one")
            } else {
                (
                    json!({"chapterId":"chapter-two","chapterNumber":2,"title":"翌日","body":"港口下起了雨。"}),
                    json!({"revisionId":null}),
                    "chapter-two",
                )
            };
            let (code, saved) = f
                .checkpoint(None, &previous, manual, "manual-after-agent")
                .await;
            assert_eq!(code, StatusCode::CREATED, "{saved}");
            let tasks_after: i64 = f
                .db()
                .query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |row| row.get(0))
                .unwrap();
            assert_eq!(tasks_after, tasks_before);
            let tracked: String = f
                .db()
                .query_row(
                    "SELECT source_revision_id FROM story_modeling_outbox_v1 WHERE object_id=?1",
                    [format!("draft-{chapter}")],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(tracked, saved["revisionId"].as_str().unwrap());
        }
    }
}

#[tokio::test]
async fn policy_explicit_revision_outcomes_still_require_saved_assessment_before_completion() {
    let flow = Flow::queued().await;
    let f = &flow.fixture;
    let task = flow.assessment().await;
    let (code, missing) = f.complete(&task).await;
    assert!(!code.is_success(), "{missing}");
    let (code, receipt) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "required-assessment"),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{receipt}");
    assert_eq!(f.complete(&task).await.0, StatusCode::OK);
    f.settle(&task);
    let (parent, read) = f.decision(&task).await;
    assert_eq!(read["canFinish"], true);
    for kind in ["revisionOutcome"] {
        assert!(
            read["requirements"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == kind && r["status"] == "satisfied"),
            "{read}"
        );
    }
    assert_eq!(f.complete(&parent).await.0, StatusCode::OK);
    f.settle(&parent);
    assert_eq!(f.work().await["status"], "completed");
    // The assessment is persisted candidate evidence, not accepted story facts.
    let report = f
        .repo()
        .object_heads(&f.project())
        .unwrap()
        .into_iter()
        .find(|head| head.object_kind == "revisionOutcome")
        .unwrap();
    assert!(report.accepted_revision_id.is_none());
}

#[tokio::test]
async fn requested_revision_outcomes_compare_exact_prose_without_semantic_extraction() {
    let flow = Flow::unmodeled().await;
    let f = &flow.fixture;
    let target = flow.assessment_target().await;
    assert_eq!(target["knowledgeBatchId"], Value::Null);
    assert!(
        !f.work().await["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["operation"] == "modelKnowledge")
    );
    let task = flow.assessment().await;
    let (status, context) = f.request("GET", &format!(
        "/context?taskId={}&threadId=assessRevisionOutcome&projectRevision=0&purpose=critic&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()), CANDIDATE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let input =
        read_context_source(f, &task, &context, "/operationContext/revisionOutcomeInput").await;
    assert_eq!(input["coverage"]["complete"], true);
    assert_eq!(input["afterScenes"][0]["body"], NEW_BODY);
    let mut forged = flow.report();
    forged["knowledgeBatchId"] = json!("invented-batch");
    assert!(
        !f.request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, forged, "forged-batch")
        )
        .await
        .0
        .is_success()
    );
    let (status, _) = f
        .request(
            "POST",
            &flow.path(),
            CANDIDATE,
            flow.submission(&task, flow.report(), "direct-text"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(f.complete(&task).await.0, StatusCode::OK);
    let (status, receipt) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["state"], "current");
    assert_eq!(receipt["assessment"]["knowledgeBatchId"], Value::Null);
    let repo = f.repo();
    let coverage = repo
        .query_writer_knowledge_by_name(
            &f.project(),
            novel_codex_story_core::WriterKnowledgeScope::PriorChapters(2),
            "密函",
            None,
            10,
        )
        .unwrap();
    assert_eq!(coverage["coverage"]["complete"], false);
    assert_eq!(coverage["coverage"]["modeledSources"], 0);
    assert_eq!(coverage["results"], json!([]));
    f.settle(&task);
    let mut changed = f.original.clone();
    changed["sceneGroups"][1]["body"] = json!("她重新烧掉密函。");
    join_body(&mut changed);
    assert_eq!(
        f.checkpoint(None, &flow.draft, changed, "after-direct-check")
            .await
            .0,
        StatusCode::CREATED
    );
    let (_, stale) = f.request("GET", &flow.path(), AUTHOR, Value::Null).await;
    assert_eq!(stale["state"], "stale");
    assert_eq!(stale["assessment"]["knowledgeBatchId"], Value::Null);
}
