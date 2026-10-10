use super::{
    fixture::*,
    flow::{Flow, Goals},
};
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn thousand_chapter_assessment_constructs_bounded_inputs_and_reads_far_evidence_lazily() {
    let flow = Flow::from_fixture(Fixture::with_long_book(1000).await, Goals::Approved).await;
    let f = &flow.fixture;
    let (status, completed) = f.complete(&flow.model).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    f.settle(&flow.model);
    f.choose(&flow.model, "assessRevisionOutcome").await;
    let task = flow.assessment().await;
    let record = f
        .repo()
        .task(&f.project(), task["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let input = f.repo().revision_outcome_input(&record).unwrap();
    assert!(input.get("priorChapters").is_none());
    assert!(input.to_string().len() < 12000);
    let db = f.db();
    let bytes:i64=db.query_row("SELECT SUM(length(CAST(payload_json AS BLOB))) FROM story_task_source_items_v2 WHERE task_id=?1",[task["id"].as_str().unwrap()],|r|r.get(0)).unwrap();
    assert!(
        bytes < 20000,
        "assessment constructor must not copy prior prose: {bytes}"
    );
    let source = input["workspaceRef"]["sourceId"].as_str().unwrap();
    let endpoint = format!(
        "/tasks/{}/task-source-content?threadId={}&expectedRevision={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let (status, dir) = f
        .request(
            "GET",
            &format!(
                "{endpoint}&sourceId={source}&view=workspace&chapterNumber=7&objectKind=draft"
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{dir}");
    assert_eq!(dir["items"][0]["revisionId"], "prior-revision-7");
    assert_eq!(dir["items"][0]["purpose"], "priorText");
    let leaf = dir["items"][0]["sourceId"].as_str().unwrap();
    let (status, text) = f
        .request(
            "GET",
            &format!("{endpoint}&sourceId={leaf}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.to_string().contains("远章独有的实际证据"));
    assert!(text.to_string().len() <= 6500);
    let cache:i64=db.query_row("SELECT COUNT(*) FROM story_task_source_items_v2 WHERE task_id=?1 AND policy_digest='workspaceObject'",[task["id"].as_str().unwrap()],|r|r.get(0)).unwrap();
    assert_eq!(cache, 1);
    db.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='unrelated-future-change' WHERE project_id=?1 AND object_id='draft-prior-999'",[f.project().as_str()]).unwrap();
    let (status, text) = f
        .request(
            "GET",
            &format!("{endpoint}&sourceId={leaf}"),
            CANDIDATE,
            Value::Null,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "unread unrelated source must not invalidate assessment: {text}"
    );
    db.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='relevant-source-change' WHERE project_id=?1 AND object_id='draft-prior-7'",[f.project().as_str()]).unwrap();
    assert!(
        f.request(
            "GET",
            &format!("{endpoint}&sourceId={leaf}"),
            CANDIDATE,
            Value::Null
        )
        .await
        .0
        .is_client_error()
    );
}

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
        input["chapterAcceptance"],
        flow.writer["target"]["chapterAcceptance"]
    );
    assert_eq!(input["chapterNumber"], 1);
    assert!(input.get("priorChapters").is_none());
    assert_eq!(input["workspaceRef"]["view"], "workspace");
    assert_eq!(
        json!(&input["desiredOutcomes"].as_array().unwrap()[1..]),
        desired_outcomes()
    );
    assert_eq!(input["desiredOutcomes"][0]["id"], "whole-chapter");
    assert_eq!(input["beforeScenes"].as_array().unwrap().len(), 4);
    assert_eq!(input["afterScenes"][1]["body"], NEW_BODY);
    assert_eq!(
        input["actualLength"]["characterCount"],
        flow.draft["body"]
            .as_str()
            .unwrap()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{feff}')
            .count()
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
    assert_eq!(context["queryReport"], Value::Null);
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
        "state":"current","assessment":report,"decisions":[],
        "issues":expected_issue_summaries(&flow.draft["revisionId"], "open", 1)});
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
    stale["issues"] = expected_issue_summaries(&edited["revisionId"], "stale", 1);
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
    assert_eq!(
        json!(&input["desiredOutcomes"].as_array().unwrap()[1..]),
        desired_outcomes()
    );
    assert_eq!(
        input["coverage"],
        json!({"complete":false,"delivery":"taskSourcePages","optionalPriorText":"taskWorkspace"})
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
    assert!(scenes.clone().all(|scene| {
        ["opening", "letter", "witness", "ending"]
            .iter()
            .any(|id| scene["groupId"] == *id)
    }));
    assert_eq!(input["beforeScenes"][1]["body"], "潮".repeat(5000));
    assert_eq!(input["afterScenes"][1]["body"], NEW_BODY);
    for side in ["beforeScenes", "afterScenes"] {
        assert_eq!(input[side].as_array().unwrap().len(), 4);
        assert_eq!(input[side][2]["body"], "守卫沉默。");
    }
}

#[tokio::test]
async fn optional_modeling_never_replaces_independent_chapter_acceptance() {
    let flow = Flow::modeled(Goals::Absent).await;
    assert_eq!(flow.fixture.complete(&flow.model).await.0, StatusCode::OK);
    flow.fixture.settle(&flow.model);
    let (parent, read) = flow.fixture.decision(&flow.model).await;
    assert_eq!(read["canFinish"], false);
    assert!(
        read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["operation"] == "assessRevisionOutcome" && a["required"] == true)
    );
    assert!(flow.fixture.complete(&parent).await.0.is_client_error());
}

#[tokio::test]
async fn ordinary_revision_requires_independent_assessment_without_mandatory_modeling() {
    for desired in [false, true] {
        let f = Fixture::new().await;
        let mut target = f.target.clone();
        if !desired {
            target.as_object_mut().unwrap().remove("desiredOutcomes");
        }
        let writer = f.writer(target).await;
        let mut draft = f.original.clone();
        draft["sceneGroups"][1]["body"] = json!(NEW_BODY);
        join_body(&mut draft);
        assert_eq!(
            f.checkpoint(Some(&writer), &f.original, draft, "chapter-saved")
                .await
                .0,
            StatusCode::CREATED
        );
        assert_eq!(f.complete(&writer).await.0, StatusCode::OK);
        f.settle(&writer);
        let (parent, read) = f.decision(&writer).await;
        assert_eq!(read["canFinish"], false);
        assert!(
            read["readyActions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["operation"] == "modelKnowledge" && a["required"] == false)
        );
        assert!(
            read["readyActions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["operation"] == "assessRevisionOutcome" && a["required"] == true)
        );
        assert!(f.complete(&parent).await.0.is_client_error());
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
    assert_eq!(read["canFinish"], false);
    assert_eq!(read["issues"]["totalCount"], 2);
    assert_eq!(read["issues"]["openCount"], 2);
    assert!(
        read["issues"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|issue| issue["observations"] == json!([]))
    );
    for kind in ["revisionOutcome"] {
        assert!(
            read["requirements"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == kind && r["status"] == "pending"),
            "{read}"
        );
    }
    let before = f.ledger_counts();
    let (code, blocked) = f.request("POST", &format!("/tasks/{}/generation-handoff", parent["id"].as_str().unwrap()), CANDIDATE,
        json!({"taskId":parent["id"],"threadId":parent["threadId"],"expectedRevision":parent["inputRevision"],"expectedIntentRevision":0,"targetChapters":2,"instruction":"完成核对后再继续下一章","idempotencyKey":"unmet-report-handoff"})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{blocked}");
    assert!(
        blocked.to_string().contains("independent assessment"),
        "{blocked}"
    );
    assert_eq!(f.ledger_counts(), before);
    assert!(f.complete(&parent).await.0.is_client_error());
    assert_ne!(f.work().await["status"], "completed");
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
    assert_eq!(input["coverage"]["complete"], false);
    assert_eq!(input["coverage"]["optionalPriorText"], "taskWorkspace");
    assert_eq!(input["afterScenes"][1]["body"], NEW_BODY);
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
