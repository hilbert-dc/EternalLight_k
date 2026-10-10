use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use eternallight_engine_core::PersistentBudget;
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository, TaskProviderBudget};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "checkpoint-author-1234567890123456";
const CANDIDATE: &str = "checkpoint-candidate-1234567890123456";
const CAPABILITY: &str = "checkpoint-capability-1234567890123456";

#[tokio::test]
async fn author_rebinds_an_edited_interrupted_chapter_without_reopening_old_attempt() {
    rebind_interrupted_chapter(RebindRunState::Running).await;
}

#[tokio::test]
async fn author_rebinds_a_paused_chapter_without_resuming_or_refunding_the_run() {
    rebind_interrupted_chapter(RebindRunState::Paused).await;
}

enum RebindRunState {
    Running,
    Paused,
}

async fn rebind_interrupted_chapter(run_state: RebindRunState) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chapter.sqlite3");
    let app = router(
        task_fixture::state(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (_,run) = call(&app,"POST",&format!("{root}/generation-runs"),AUTHOR,Some(json!({"expectedProjectRevision":0,
        "authorIntent":"写一章港城故事","targetChapters":1,"maxTaskAttempts":12,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600}))).await;
    let uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
    for (kind, payload) in [
        ("brief", json!({"premise":"抄写员保留密函"})),
        (
            "outline",
            json!({"nodes":[{"id":"book","kind":"book","position":0,"title":"港城","purpose":"保留记忆","summary":"见证者"}]}),
        ),
        (
            "plan",
            json!({"chapters":[{"chapterPlanId":"plan-1","chapterNumber":1,"goal":"发现信件","publicTitle":"发现密函","writerGuidance":"按原章纲发现密函","publicCommitments":[],"povCharacterId":"linxi","storyTime":10,"scenes":[{"id":"scene-1","goal":"找到密函","beats":[],"expectedEffects":[{"id":"effect-1","dimension":"knowledge","description":"发现密函","commitmentRefs":[]}]}]}]}),
        ),
    ] {
        let operation = match kind {
            "brief" => "generateBrief",
            "outline" => "generateOutline",
            "plan" => "generatePlan",
            _ => unreachable!(),
        };
        let next = current_model_action(&app, &path, &root, &uri, operation).await;
        let (status,task) = call(&app,"POST",&format!("{root}/tasks"),AUTHOR,Some(json!({
            "operation":next["action"]["operation"],"inputRevision":0,"threadId":kind,"target":next["action"]["target"],"authorIntent":"写港城故事",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}))).await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let id = task["id"].as_str().unwrap();
        if kind == "plan" {
            let (_, before_plan) = call(
                &app,
                "GET",
                &format!("{root}/creative-workspace"),
                AUTHOR,
                /*payload*/ None,
            )
            .await;
            for (field, value) in [
                ("povCharacterId", json!(123)),
                ("povCharacterId", json!(" ")),
                ("storyTime", json!("tomorrow")),
                ("storyTime", json!(1.5)),
            ] {
                let mut invalid = payload.clone();
                invalid["chapters"][0][field] = value;
                let (status, rejected) = call(
                    &app,
                    "POST",
                    &format!("{root}/artifacts/plan"),
                    CANDIDATE,
                    Some(json!({
                        "payload":invalid,"expectedRevision":0,"expectedCandidateRevisionId":null,
                        "idempotencyKey":"invalid-plan-position","taskId":id,"threadId":kind
                    })),
                )
                .await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
                assert_eq!(rejected["error"]["message"], "TaskOutputForbidden");
                assert_eq!(
                    call(
                        &app,
                        "GET",
                        &format!("{root}/creative-workspace"),
                        AUTHOR,
                        /*payload*/ None
                    )
                    .await,
                    (StatusCode::OK, before_plan.clone())
                );
            }
        }
        let (status,saved) = call(&app,"POST",&format!("{root}/artifacts/{kind}"),CANDIDATE,Some(json!({
            "payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":kind,"taskId":id,"threadId":kind}))).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        let (status, done) = call(
            &app,
            "POST",
            &format!("{root}/tasks/{id}/complete"),
            CANDIDATE,
            Some(json!({"threadId":kind,"summary":"已保存"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        charge_budget(&app, &path, &uri, id).await;
        let (_, claimed) = call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await;
        let (status, advanced) = call(
            &app,
            "POST",
            &format!("{uri}/advance-task"),
            AUTHOR,
            Some(json!({"expectedVersion":claimed["version"],"taskId":id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{advanced}");
    }
    // The model selects writeNextChapter; Native materializes the selected plan
    // while returning the actual generateChapter Step, within the same budget.
    let next = current_model_action(&app, &path, &root, &uri, "generateChapter").await;
    let materialized = next["run"].clone();
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    let revision = workspace["revision"].clone();
    let create = json!({"operation":"generateChapter","inputRevision":revision,"threadId":"draft","target":next["action"]["target"],"authorIntent":"写港城故事",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600});
    assert_eq!(create["target"]["povCharacterId"], "linxi");
    assert_eq!(create["target"]["storyTime"], 10);
    let (status, task) = call(
        &app,
        "POST",
        &format!("{root}/tasks"),
        AUTHOR,
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    read_current_inputs(&app, &root, &task).await;
    let id = task["id"].as_str().unwrap();
    let (_, before_invalid) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    // A full planned chapter without explicit mapping is rejected before any save.
    let invalid = json!({"payload":{"chapterId":materialized["currentChapterId"],"chapterNumber":1,
        "title":"信","body":"他烧掉密函。","completeness":"chapterComplete"},
        "expectedRevision":revision,"expectedCandidateRevisionId":null,"idempotencyKey":"unmapped",
        "taskId":id,"threadId":"draft"});
    let (status, rejected) = call(
        &app,
        "POST",
        &format!("{root}/drafts/checkpoints"),
        CANDIDATE,
        Some(invalid),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Planned chapterComplete requires explicit sceneGroups")
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/creative-workspace"),
            AUTHOR,
            /*payload*/ None
        )
        .await,
        (StatusCode::OK, before_invalid)
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let proof: (String, Option<String>, i64) = connection.query_row(
        "SELECT state,receipt_ref,(SELECT COUNT(*) FROM story_task_outputs_v2 WHERE task_id=?1) FROM story_tool_operations_v1 WHERE task_id=?1 AND call_id='fixture-unmapped'",
        [id], |row| Ok((row.get(/*idx*/ 0)?,row.get(/*idx*/ 1)?,row.get(/*idx*/ 2)?))).unwrap();
    assert_eq!(proof, ("rejected".into(), None, 0));
    drop(connection);
    let (status,saved) = call(&app,"POST",&format!("{root}/drafts/checkpoints"),CANDIDATE,Some(json!({
        "payload":{"chapterId":materialized["currentChapterId"],"chapterNumber":1,"title":"信","body":"他烧掉密函。",
          "sceneGroups":[{"id":"group-1","sceneId":"scene-1","body":"他烧掉密函。","locked":false}]},
        "expectedRevision":revision,"expectedCandidateRevisionId":null,"idempotencyKey":"draft","taskId":id,"threadId":"draft"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let connection = rusqlite::Connection::open(&path).unwrap();
    let proof: (String, String) = connection.query_row(
        "SELECT state,receipt_ref FROM story_tool_operations_v1 WHERE task_id=?1 AND call_id='fixture-draft'",
        [id], |row| Ok((row.get(/*idx*/ 0)?,row.get(/*idx*/ 1)?))).unwrap();
    assert_eq!(
        proof,
        (
            "committed".into(),
            format!(
                "runtime:candidate:{}",
                saved["revisionId"].as_str().unwrap()
            )
        )
    );
    drop(connection);
    let (_, failed) = call(
        &app,
        "PATCH",
        &format!("{root}/tasks/{id}"),
        AUTHOR,
        Some(json!({"threadId":"draft","status":"failed","error":"interrupted"})),
    )
    .await;
    assert_eq!(failed["status"], "failed");
    charge_budget(&app, &path, &uri, id).await;
    let (_, blocked) = call(
        &app,
        "POST",
        &format!("{uri}/next"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    let mut draft = workspace["chapters"][0]["candidate"].clone();
    draft["body"] = json!("他藏好密函。");
    draft["sceneGroups"][0]["body"] = json!("他藏好密函。");
    let (status, edited) = call(
        &app,
        "POST",
        &format!("{root}/drafts/checkpoints"),
        AUTHOR,
        Some(json!({"payload":draft,"expectedRevision":revision,
        "expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"author-edit"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    assert!(
        call(
            &app,
            "POST",
            &format!("{uri}/retry-failed-task"),
            AUTHOR,
            Some(json!({"expectedVersion":blocked["run"]["version"]}))
        )
        .await
        .0
        .is_client_error()
    );
    let (status, preview) = call(
        &app,
        "GET",
        &format!("{uri}/replan"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["availableFrom"], json!(["currentChapter"]));
    assert_eq!(preview["activeAttemptStatus"], "failed");
    let request = json!({"expectedVersion":preview["run"]["version"],"expectedProjectRevision":revision,
        "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter","instruction":"基于藏信修改补全本章"});
    let (status, replanned) = call(
        &app,
        "POST",
        &format!("{uri}/replan"),
        AUTHOR,
        Some(request),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replanned}");
    assert_eq!(
        replanned["currentChapterId"],
        materialized["currentChapterId"]
    );
    assert_eq!(replanned["stage"], "coordinate");
    assert_eq!(
        replanned["usedTaskAttempts"],
        blocked["run"]["usedTaskAttempts"]
    );
    assert_eq!(replanned["replans"][0]["previousTaskId"], id);
    let repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repo.task(&ProjectId::parse(project.to_owned()).unwrap(), id)
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    let next = current_model_action(&app, &path, &root, &uri, "generateChapter").await;
    let mut create = create;
    create["threadId"] = json!("rebound");
    create["target"] = next["action"]["target"].clone();
    let (status, rebound) =
        call(&app, "POST", &format!("{root}/tasks"), AUTHOR, Some(create)).await;
    assert_eq!(status, StatusCode::CREATED, "{rebound}");
    read_current_inputs(&app, &root, &rebound).await;
    assert_eq!(
        rebound["inputManifest"]["expectedCandidateRevisionId"],
        edited["revisionId"]
    );
    assert_eq!(rebound["target"]["intentGeneration"], 1);
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    let plan = &workspace["artifacts"]["plan"]["candidate"];
    let previous_plan_revision = plan["id"].clone();
    let mut changed_plan = plan["payload"].clone();
    changed_plan["chapters"][0]["goal"] = json!("作者改变当前章规划");
    changed_plan["chapters"][0]["publicTitle"] = json!("藏信");
    changed_plan["chapters"][0]["writerGuidance"] = json!("保留作者藏信稿");
    changed_plan["chapters"][0]["povCharacterId"] = json!("zhangheng");
    changed_plan["chapters"][0]["storyTime"] = json!(20);
    let (status,saved) = call(&app,"POST",&format!("{root}/artifacts/plan"),AUTHOR,Some(json!({
        "payload":changed_plan,"expectedRevision":revision,"expectedCandidateRevisionId":plan["id"],"idempotencyKey":"new-plan"
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let context_uri = format!(
        "{root}/context?purpose=writer&projectRevision={revision}&readingChapter=1&sceneOrder=2147483647&povCharacterId=linxi&storyTime=10&budget=24&byteBudget=60000"
    );
    let (status, before_context) = call(&app, "GET", &context_uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(status, StatusCode::OK, "{before_context}");
    assert_eq!(
        before_context["plannedEffects"],
        json!([{
            "source":"plan", "value":{"planRevisionId":previous_plan_revision,"chapters":plan["payload"]["chapters"]}
        }])
    );
    let (status, before_workspace) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{before_workspace}");
    assert_eq!(
        before_workspace["chapterPlanBindings"][0]["writerProjection"]["writerGuidance"],
        "按原章纲发现密函"
    );
    // A broken binding must not substitute the readable newer plan head.
    let corrupted = rusqlite::Connection::open(&path).unwrap();
    corrupted.execute("UPDATE story_chapter_plan_bindings_v2 SET chapter_plan_id='missing-bound-entry' WHERE project_id=?1", [project]).unwrap();
    assert!(
        call(&app, "GET", &context_uri, AUTHOR, /*payload*/ None)
            .await
            .0
            .is_server_error()
    );
    assert!(
        call(
            &app,
            "GET",
            &format!("{root}/creative-workspace"),
            AUTHOR,
            /*payload*/ None
        )
        .await
        .0
        .is_server_error()
    );
    corrupted.execute("UPDATE story_chapter_plan_bindings_v2 SET chapter_plan_id='plan-1' WHERE project_id=?1", [project]).unwrap();
    assert!(
        call(
            &app,
            "GET",
            &format!("{uri}/replan"),
            AUTHOR,
            /*payload*/ None
        )
        .await
        .0
        .is_client_error()
    );
    let rebind = json!({
        "chapterId":materialized["currentChapterId"],"previousPlanRevisionId":previous_plan_revision,
        "previousChapterPlanId":"plan-1","sourceRevisionId":edited["revisionId"],
        "planRevisionId":saved["revisionId"],"chapterPlanId":"plan-1",
        "expectedRevision":revision,"idempotencyKey":"rebind-plan"
    });
    let rebind_uri = format!("{root}/chapters/rebind-plan");
    assert_eq!(
        call(&app, "POST", &rebind_uri, CANDIDATE, Some(rebind.clone()))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let binding_before = repo
        .chapter_bindings(&ProjectId::parse(project.to_owned()).unwrap())
        .unwrap();
    for (field, value) in [
        ("previousPlanRevisionId", json!("stale-plan")),
        ("previousChapterPlanId", json!("stale-entry")),
        ("sourceRevisionId", json!("stale-draft")),
        ("planRevisionId", previous_plan_revision.clone()),
        ("chapterPlanId", json!("missing-entry")),
        ("expectedRevision", json!(999)),
    ] {
        let mut invalid = rebind.clone();
        invalid[field] = value;
        assert!(
            call(&app, "POST", &rebind_uri, AUTHOR, Some(invalid))
                .await
                .0
                .is_client_error()
        );
        assert_eq!(
            repo.chapter_bindings(&ProjectId::parse(project.to_owned()).unwrap())
                .unwrap(),
            binding_before
        );
    }
    let (_, before_invalid_plan) = call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await;
    let plan_content: String = corrupted
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
            [saved["revisionId"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let mut malformed: Value = serde_json::from_str(&plan_content).unwrap();
    malformed["payload"]["chapters"][0]["povCharacterId"] = json!(17);
    corrupted
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id=?2",
            rusqlite::params![malformed.to_string(), saved["revisionId"].as_str().unwrap()],
        )
        .unwrap();
    let (status, rejected_plan) =
        call(&app, "POST", &rebind_uri, AUTHOR, Some(rebind.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected_plan}");
    assert_eq!(rejected_plan["error"]["message"], "InvalidChapter");
    assert_eq!(
        repo.chapter_bindings(&ProjectId::parse(project.to_owned()).unwrap())
            .unwrap(),
        binding_before
    );
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, before_invalid_plan)
    );
    corrupted
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id=?2",
            rusqlite::params![plan_content, saved["revisionId"].as_str().unwrap()],
        )
        .unwrap();
    let (_, before_rebind) = call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await;
    if matches!(run_state, RebindRunState::Paused) {
        let (status, paused) = call(
            &app,
            "POST",
            &format!("{uri}/control"),
            AUTHOR,
            Some(json!({"expectedVersion":before_rebind["version"],"command":"pause"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{paused}");
        assert_eq!(paused["status"], "paused");
    }
    let (_, before_rebind) = call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await;
    let work_uri = format!("{root}/work-tasks/work-{}", run["id"].as_str().unwrap());
    let (_, before_work) = call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await;
    let project_id = ProjectId::parse(project.to_owned()).unwrap();
    let old_task = repo
        .task(&project_id, rebound["id"].as_str().unwrap())
        .unwrap();
    // A malformed projection must roll back the earlier binding, workspace and task writes.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let work_id = format!("work-{}", run["id"].as_str().unwrap());
    let raw_run_before: String = connection
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [run["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE story_work_tasks_v1 SET kind='conversation' WHERE project_id=?1 AND id=?2",
            rusqlite::params![project, work_id],
        )
        .unwrap();
    let corrupt_work = call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await;
    if matches!(run_state, RebindRunState::Paused) {
        assert_eq!(corrupt_work.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            corrupt_work.1,
            json!({"error":{"code":"invalid_request","details":{},"message":"TaskBindingInvalid","recovery":null}})
        );
    } else {
        assert_eq!(corrupt_work.0, StatusCode::OK);
    }
    assert!(
        call(&app, "POST", &rebind_uri, AUTHOR, Some(rebind.clone()))
            .await
            .0
            .is_server_error()
    );
    let (status, rejected) = call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(rejected["error"]["message"], "TaskBindingInvalid");
    assert_eq!(
        connection
            .query_row(
                "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
                [run["id"].as_str().unwrap()],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        raw_run_before
    );
    assert_eq!(
        call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await,
        corrupt_work
    );
    assert_eq!(repo.chapter_bindings(&project_id).unwrap(), binding_before);
    assert_eq!(
        repo.project_revision(&project_id).unwrap(),
        revision.as_i64().unwrap()
    );
    assert_eq!(
        repo.task(&project_id, rebound["id"].as_str().unwrap())
            .unwrap(),
        old_task
    );
    let receipts:i64 = connection.query_row("SELECT COUNT(*) FROM story_domain_receipts_v2 WHERE project_id=?1 AND operation='rebind-plan'",[project],|row|row.get(0)).unwrap();
    assert_eq!(receipts, 0);
    connection
        .execute(
            "UPDATE story_work_tasks_v1 SET kind='generation' WHERE project_id=?1 AND id=?2",
            rusqlite::params![project, work_id],
        )
        .unwrap();
    assert_eq!(
        call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, before_work)
    );
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, before_rebind.clone())
    );
    let (status, decision) = call(&app, "POST", &rebind_uri, AUTHOR, Some(rebind.clone())).await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    let mut expected_run = before_rebind.clone();
    expected_run["version"] = json!(before_rebind["version"].as_i64().unwrap() + 1);
    let expected_status = match run_state {
        RebindRunState::Running => "blocked",
        RebindRunState::Paused => "paused",
    };
    expected_run["status"] = json!(expected_status);
    expected_run["blockedReason"] = json!("chapter_plan_rebound");
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, expected_run.clone())
    );
    let (_, work) = call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await;
    assert_eq!(work["status"], expected_status);
    assert_eq!(
        decision,
        json!({
            "previousPlanRevisionId":previous_plan_revision,"previousChapterPlanId":"plan-1",
            "sourceRevisionId":edited["revisionId"],"binding":{
                "planRevisionId":saved["revisionId"],"chapterPlanId":"plan-1",
                "chapterId":materialized["currentChapterId"],"chapterNumber":1,
                "resultRevision":revision.as_i64().unwrap()+1
            }
        })
    );
    assert_eq!(
        call(&app, "POST", &rebind_uri, AUTHOR, Some(rebind)).await,
        (StatusCode::OK, decision.clone())
    );
    assert_eq!(
        call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, expected_run.clone())
    );
    let reopened = router(
        task_fixture::state(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    assert_eq!(
        call(&reopened, "GET", &uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, expected_run)
    );
    assert_eq!(
        call(&reopened, "GET", &work_uri, AUTHOR, /*payload*/ None).await,
        (StatusCode::OK, work)
    );
    let (_, after) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(
        after["chapters"][0]["candidate"],
        workspace["chapters"][0]["candidate"]
    );
    assert_eq!(
        after["chapterPlanBindings"][0]["writerProjection"],
        json!({
            "chapterPlanId":"plan-1", "chapterNumber":1, "primaryNodeId":null,
            "publicTitle":"藏信", "writerGuidance":"保留作者藏信稿", "publicCommitments":[],
            "povCharacterId":"zhangheng","storyTime":20
        })
    );
    assert_eq!(
        repo.task(
            &ProjectId::parse(project.to_owned()).unwrap(),
            rebound["id"].as_str().unwrap()
        )
        .unwrap()
        .unwrap()
        .status,
        "cancelled"
    );
    let charged_rebound = charge_budget(&app, &path, &uri, rebound["id"].as_str().unwrap()).await;
    let next_counts_before: (i64,i64,i64) = connection.query_row(
        "SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    let cancelled_before_next = repo
        .task(&project_id, rebound["id"].as_str().unwrap())
        .unwrap();
    let work_before_next = call(&app, "GET", &work_uri, AUTHOR, None).await;
    let (status, blocked) = call(
        &app,
        "POST",
        &format!("{uri}/next"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{blocked}");
    assert_eq!(blocked["run"]["status"], expected_status);
    if matches!(run_state, RebindRunState::Paused) {
        assert_eq!(blocked["action"], Value::Null);
        assert_eq!(blocked["run"], charged_rebound);
        assert_eq!(
            call(&app, "GET", &work_uri, AUTHOR, None).await,
            work_before_next
        );
        assert_eq!(
            repo.task(&project_id, rebound["id"].as_str().unwrap())
                .unwrap(),
            cancelled_before_next
        );
        let next_counts_after = connection.query_row(
            "SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",
            [], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?)),
        ).unwrap();
        assert_eq!(next_counts_after, next_counts_before);
    }
    let (status, preview) = call(
        &app,
        "GET",
        &format!("{uri}/replan"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["availableFrom"], json!(["currentChapter"]));
    let (status, renewed) = call(&app,"POST",&format!("{uri}/replan"),AUTHOR,Some(json!({
        "expectedVersion":preview["run"]["version"],"expectedProjectRevision":preview["projectRevision"],
        "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter",
        "instruction":"保留作者藏信稿，按新章纲补全"
    }))).await;
    assert_eq!(status, StatusCode::OK, "{renewed}");
    assert_eq!(
        renewed["usedTaskAttempts"],
        blocked["run"]["usedTaskAttempts"]
    );
    assert_eq!(renewed["intentGeneration"], 2);
    assert_eq!(
        renewed["currentChapterId"],
        materialized["currentChapterId"]
    );
    let next = current_model_action(&app, &path, &root, &uri, "generateChapter").await;
    assert_eq!(next["action"]["target"]["povCharacterId"], "zhangheng");
    assert_eq!(next["action"]["target"]["storyTime"], 20);
    let admitted_run = next["run"].clone();
    for field in [
        "id",
        "projectId",
        "authorMessageId",
        "maxTaskAttempts",
        "maxRetryAttempts",
        "retryAttempts",
        "deadlineAt",
        "startChapterNumber",
        "targetChapters",
        "completedChapters",
        "currentChapterId",
        "currentChapterNumber",
        "intentGeneration",
        "intentRevisions",
        "replans",
        "baseSnapshotId",
        "sourceDigest",
    ] {
        assert_eq!(
            admitted_run[field], renewed[field],
            "合法coordinator不得改授权或作者资料: {field}"
        );
    }
    assert_eq!(
        admitted_run["usedTaskAttempts"],
        renewed["usedTaskAttempts"].as_i64().unwrap() + 1
    );
    let old_charges = renewed["providerBudget"]["charges"].as_array().unwrap();
    let charges = admitted_run["providerBudget"]["charges"]
        .as_array()
        .unwrap();
    assert_eq!(charges.len(), old_charges.len() + 1);
    assert_eq!(&charges[..old_charges.len()], old_charges.as_slice());
    let last_charge = charges.last().unwrap();
    assert_eq!(
        last_charge,
        &json!({"taskId":admitted_run["lastTaskId"],"budgetId":format!("budget-{}",admitted_run["lastTaskId"].as_str().unwrap()),"status":"terminal","requests":0,"tokens":0,"unknownTokens":0,"overrun":false})
    );
    for field in [
        "maxRequests",
        "maxTokens",
        "usedRequests",
        "usedTokens",
        "unknownTokens",
        "overrun",
    ] {
        assert_eq!(
            admitted_run["providerBudget"][field],
            renewed["providerBudget"][field]
        );
    }
    assert!(admitted_run["version"].as_i64().unwrap() > renewed["version"].as_i64().unwrap());
    assert_ne!(admitted_run["stepToken"], renewed["stepToken"]);
    assert_eq!(admitted_run["activeTaskId"], Value::Null);
    assert_eq!(admitted_run["status"], "running");
    assert_eq!(admitted_run["stage"], "coordinate");
    let rows_before: (i64,i64,i64) = connection.query_row(
        "SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    let (_, work_before) = call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await;
    for (field, value) in [
        ("povCharacterId", json!("linxi")),
        ("povCharacterId", Value::Null),
        ("storyTime", json!(10)),
        ("storyTime", Value::Null),
        ("readingChapter", Value::Null),
        ("sceneOrder", json!(2)),
        ("scope", json!({"type":"throughScene","endSceneOrder":1})),
    ] {
        let mut target = next["action"]["target"].clone();
        if value.is_null() {
            target.as_object_mut().unwrap().remove(field);
        } else {
            target[field] = value;
        }
        let (status, rejected) = call(&app,"POST",&format!("{root}/tasks"),AUTHOR,Some(json!({
            "operation":"generateChapter","inputRevision":decision["binding"]["resultRevision"],
            "threadId":"invalid-writer-scope","target":target,"authorIntent":"按新章纲继续",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600
        }))).await;
        assert!(status.is_client_error(), "{rejected}");
        assert_eq!(
            call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await,
            (StatusCode::OK, admitted_run.clone())
        );
        assert_eq!(
            call(&app, "GET", &work_uri, AUTHOR, /*payload*/ None).await,
            (StatusCode::OK, work_before.clone())
        );
        let rows_after = connection.query_row(
            "SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_work_attempts_v1)",
            [], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?)),
        ).unwrap();
        assert_eq!(rows_after, rows_before);
    }
    let (status,writer) = call(&app,"POST",&format!("{root}/tasks"),AUTHOR,Some(json!({
        "operation":"generateChapter","inputRevision":decision["binding"]["resultRevision"],
        "threadId":"new-plan-writer","target":next["action"]["target"],"authorIntent":"按新章纲保留密函",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{writer}");
    let writer_inputs = read_current_inputs(&app, &root, &writer).await;
    let writer_context_uri = format!(
        "{root}/context?purpose=writer&projectRevision={}&readingChapter=1&sceneOrder={}&povCharacterId=zhangheng&storyTime=20&budget=24&byteBudget=60000&taskId={}&threadId=new-plan-writer",
        decision["binding"]["resultRevision"],
        writer["target"]["sceneOrder"],
        writer["id"].as_str().unwrap()
    );
    let (status, writer_context) = call(
        &app,
        "GET",
        &writer_context_uri,
        CANDIDATE,
        /*payload*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{writer_context}");
    assert_eq!(
        writer_context["target"]["position"],
        json!({"readingChapter":1,"sceneOrder":writer["target"]["sceneOrder"],"povCharacterId":"zhangheng","storyTime":20})
    );
    let (status, inherited_position) = call(
        &app,
        "GET",
        &writer_context_uri.replace("&povCharacterId=zhangheng", ""),
        CANDIDATE,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{inherited_position}");
    for field in [
        "target",
        "queryScope",
        "sourceWatermarks",
        "snapshotId",
        "policyDigest",
        "contentHash",
    ] {
        assert_eq!(
            inherited_position[field], writer_context[field],
            "omission must retain the exact frozen range: {field}"
        );
    }
    let inherited_inputs = read_pack_sources(&app, &root, &writer, &inherited_position).await;
    assert_eq!(
        inherited_inputs.as_object().unwrap().len(),
        writer_inputs.as_object().unwrap().len()
    );
    for (section, original) in writer_inputs.as_object().unwrap() {
        assert_eq!(
            inherited_inputs[section]["content"], original["content"],
            "complete original must be identical: {section}"
        );
        for field in ["sourceId", "contentHash", "payloadBytes", "sectionPath"] {
            assert_eq!(
                inherited_inputs[section]["source"][field],
                original["source"][field]
            );
        }
    }
    assert_eq!(
        inherited_inputs["/authorRequirementsSource"]["content"]["authorText"],
        writer["inputManifest"]["authorIntent"]
    );
    assert_eq!(
        inherited_inputs["/relevantText"]["content"],
        writer_inputs["/relevantText"]["content"]
    );
    for denied_uri in [
        writer_context_uri.replace("povCharacterId=zhangheng", "povCharacterId=linxi"),
        writer_context_uri.replace("storyTime=20", "storyTime=10"),
    ] {
        let (status, denied) = call(&app, "GET", &denied_uri, CANDIDATE, /*payload*/ None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
        assert_eq!(denied["error"]["code"], "writer_scope_mismatch");
    }
    assert_eq!(writer_context["plannedEffects"]["textDelivered"], false);
    assert_eq!(
        writer_context["plannedEffects"]["sourceId"],
        writer_inputs["/plannedEffects"]["source"]["sourceId"]
    );
    assert_eq!(
        writer_inputs["/plannedEffects"]["content"],
        json!([{
            "source":"plan", "value":{"planRevisionId":saved["revisionId"],"chapters":changed_plan["chapters"]}
        }])
    );
    assert_eq!(
        writer["inputManifest"]["expectedCandidateRevisionId"],
        edited["revisionId"]
    );
    let output = json!({
        "payload":{"chapterId":materialized["currentChapterId"],"chapterNumber":1,"title":"信",
            "body":"他藏好密函，等待白塔亮灯。","sceneGroups":[{"id":"group-1","sceneId":"scene-1",
            "body":"他藏好密函，等待白塔亮灯。","locked":false}]},
        "expectedRevision":decision["binding"]["resultRevision"],"expectedCandidateRevisionId":edited["revisionId"],
        "idempotencyKey":"new-plan-draft","taskId":writer["id"],"threadId":"new-plan-writer"
    });
    let mut late = output.clone();
    late["taskId"] = rebound["id"].clone();
    late["threadId"] = json!("rebound");
    assert!(
        call(
            &app,
            "POST",
            &format!("{root}/drafts/checkpoints"),
            CANDIDATE,
            Some(late)
        )
        .await
        .0
        .is_client_error()
    );
    let (status, new_draft) = call(
        &app,
        "POST",
        &format!("{root}/drafts/checkpoints"),
        CANDIDATE,
        Some(output),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{new_draft}");
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        /*payload*/ None,
    )
    .await;
    assert_eq!(
        workspace["chapters"][0]["candidate"]["sourcePlanRefs"]["planRevisionId"],
        saved["revisionId"]
    );
    assert_eq!(
        workspace["chapters"][0]["candidate"]["sourcePlanRefs"]["chapterPlanId"],
        "plan-1"
    );
    assert_eq!(workspace["chapters"][0]["accepted"], Value::Null);
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let mut payload = payload;
    if method == "POST"
        && uri.ends_with("/generation-runs")
        && let Some(body) = payload.as_mut()
        && body.get("expectedBaseSnapshotId").is_none()
    {
        let workspace_uri = format!(
            "{}/creative-workspace",
            uri.trim_end_matches("/generation-runs")
        );
        let (_, workspace) = Box::pin(call(app, "GET", &workspace_uri, actor, None)).await;
        body["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
        body["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    }
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if actor == CANDIDATE && uri.ends_with("/drafts/checkpoints") {
        let body = payload.as_ref().unwrap();
        let thread = body["threadId"].as_str().unwrap();
        request = request
            .header("x-novel-tool-name", "save_draft_checkpoint")
            .header("x-novel-root-thread", thread)
            .header("x-novel-caller-thread", thread)
            .header("x-novel-turn", "draft-turn")
            .header(
                "x-novel-tool-call",
                format!("fixture-{}", body["idempotencyKey"].as_str().unwrap()),
            )
            .header(
                "x-novel-request-hash",
                format!("{:x}", Sha256::digest(body.to_string().as_bytes())),
            );
    }
    let response = app
        .clone()
        .author_fixture_request(
            request
                .body(payload.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|parse_error| {
        panic!(
            "{method} {uri}: HTTP {status}; invalid JSON: {parse_error}; body: {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

// Only fixture model choices are scripted. Native still admits the real action,
// freezes its target/acceptance and charges every actual coordinator Attempt.
async fn current_model_action(
    app: &Router,
    path: &std::path::Path,
    root: &str,
    run_uri: &str,
    operation: &str,
) -> Value {
    let (status, next) = call(app, "POST", &format!("{run_uri}/next"), AUTHOR, None).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["kind"], "agentTask", "{next}");
    if next["action"]["operation"] == operation {
        return next;
    }
    assert_eq!(next["action"]["operation"], "advisory", "{next}");
    // Host may read next once for recovery and again for dispatch. The queued
    // decision must retain its instruction rather than querying a writer action.
    let (repeated_status, repeated) = call(app,"POST",&format!("{run_uri}/next"),AUTHOR,None).await;
    assert_eq!(repeated_status,StatusCode::OK,"{repeated}");
    assert_eq!(repeated["action"]["instruction"],next["action"]["instruction"]);
    assert!(!repeated["action"]["instruction"].as_str().unwrap().is_empty());
    assert_eq!(repeated["action"]["target"]["resumeStepId"],next["action"]["target"]["resumeStepId"]);

    let (_, workspace) = call(
        app,
        "GET",
        &format!("{root}/creative-workspace"),
        AUTHOR,
        None,
    )
    .await;
    let author_intent = next["run"]["intentRevisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|revision| revision["instruction"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!author_intent.trim().is_empty());
    let (status, coordinator) = call(
        app,
        "POST",
        &format!("{root}/tasks"),
        AUTHOR,
        Some(json!({
            "operation":next["action"]["operation"],"inputRevision":workspace["revision"],
            "threadId":"coordinator","target":next["action"]["target"],
            "authorIntent":author_intent,
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":next["run"]["deadlineAt"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{coordinator}");
    read_current_inputs(app, root, &coordinator).await;
    let id = coordinator["id"].as_str().unwrap();
    let uri = format!(
        "{root}/tasks/{id}/work-decision?threadId=coordinator&expectedRevision={}",
        coordinator["inputRevision"]
    );
    let (status, mut decision) = call(app, "GET", &uri, CANDIDATE, None).await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    let mut cursor = decision["pagination"]["nextCursor"].clone();
    while let Some(next_cursor) = cursor.as_str() {
        let (status, page) = call(
            app,
            "GET",
            &format!("{uri}&cursor={}", next_cursor.replace(':', "%3A")),
            CANDIDATE,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["version"], decision["version"]);
        assert_eq!(page["sourceDigest"], decision["sourceDigest"]);
        decision["readyActions"]
            .as_array_mut()
            .unwrap()
            .extend(page["readyActions"].as_array().unwrap().iter().cloned());
        cursor = page["pagination"]["nextCursor"].clone();
    }
    let selected = if operation == "generateChapter" && next["run"]["currentChapterId"].is_null() {
        "writeNextChapter"
    } else {
        operation
    };
    let action = decision["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["operation"] == selected)
        .unwrap_or_else(|| panic!("missing actual model action {selected}: {decision}"));
    let mut request = json!({"taskId":id,"threadId":"coordinator","expectedRevision":coordinator["inputRevision"],
        "expectedWorkVersion":decision["version"],"expectedSourceDigest":decision["sourceDigest"],
        "actionKey":action["actionKey"],"instruction":"按当前作者原话及已保存来源处理港城故事",
        "idempotencyKey":format!("choose-{selected}")});
    if operation == "generateChapter" {
        request["chapterAcceptance"] = if decision["chapterAcceptance"].is_object() {
            decision["chapterAcceptance"].clone()
        } else {
            json!({"requirements":[{"id":"port-story","text":"写一章港城故事，沿用当前作者要求和绑定章纲"}],
                "length":{"requirement":"本测试原作者没有篇幅要求","minCharacters":null,"maxCharacters":null}})
        };
    }
    let (status, selected) = call(
        app,
        "POST",
        &format!("{root}/tasks/{id}/work-decision/actions"),
        CANDIDATE,
        Some(request),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    let (status, completed) = call(
        app,
        "POST",
        &format!("{root}/tasks/{id}/complete"),
        CANDIDATE,
        Some(json!({"threadId":"coordinator","summary":"已按原作者要求选择真实后继行动"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let charged = charge_budget(app, path, run_uri, id).await;
    let (status, advanced) = call(
        app,
        "POST",
        &format!("{run_uri}/advance-task"),
        AUTHOR,
        Some(json!({"expectedVersion":charged["version"],"taskId":id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{advanced}");
    let (status, next) = call(app, "POST", &format!("{run_uri}/next"), AUTHOR, None).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["action"]["operation"], operation, "{next}");
    next
}

async fn read_current_inputs(app: &Router, root: &str, task: &Value) -> Value {
    let mut binding = format!(
        "taskId={}&threadId={}&projectRevision={}&purpose={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        task["purpose"].as_str().unwrap()
    );
    if task["purpose"] == "writer" {
        let target = &task["target"];
        binding.push_str(&format!(
            "&readingChapter={}&sceneOrder={}&povCharacterId={}&storyTime={}",
            target["readingChapter"].as_i64().unwrap(),
            target["sceneOrder"].as_i64().unwrap(),
            target["povCharacterId"].as_str().unwrap(),
            target["storyTime"].as_i64().unwrap()
        ));
    }
    let (status, context) = call(
        app,
        "GET",
        &format!("{root}/context?{binding}&budget=24&byteBudget=60000"),
        CANDIDATE,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    read_pack_sources(app, root, task, &context).await
}

async fn read_pack_sources(app: &Router, root: &str, task: &Value, context: &Value) -> Value {
    let sources = context["taskSources"]["entries"].as_array().unwrap();
    let mut originals = json!({});
    assert!(
        sources
            .iter()
            .any(|source| source["sectionPath"] == "/authorRequirementsSource"),
        "{context}"
    );
    for source in sources
        .iter()
        .filter(|source| source["sectionPath"].is_string())
    {
        let uri = format!(
            "{root}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
            task["id"].as_str().unwrap(),
            task["threadId"].as_str().unwrap(),
            task["inputRevision"],
            source["sourceId"].as_str().unwrap()
        );
        let mut suffix = String::new();
        let mut payload = String::new();
        loop {
            let (status, page) = call(app, "GET", &format!("{uri}{suffix}"), CANDIDATE, None).await;
            assert_eq!(status, StatusCode::OK, "{page}");
            assert_eq!(page["sourceId"], source["sourceId"]);
            assert_eq!(page["contentHash"], source["contentHash"]);
            assert_eq!(page["byteRange"]["start"], json!(payload.len()));
            let part = page["payloadPart"].as_str().unwrap();
            assert!(part.len() <= 2_000);
            assert!(page.to_string().len() <= 6_500);
            payload.push_str(part);
            assert_eq!(page["byteRange"]["end"], json!(payload.len()));
            if let Some(cursor) = page["nextCursor"].as_str() {
                suffix = format!("&cursor={}", cursor.replace(':', "%3A"));
            } else {
                assert_eq!(page["byteRange"]["total"], json!(payload.len()));
                assert_eq!(page["endOfInputFromCursor"], true);
                break;
            }
        }
        assert_eq!(source["payloadBytes"], json!(payload.len()));
        assert_eq!(
            source["contentHash"],
            format!("{:x}", Sha256::digest(payload.as_bytes()))
        );
        let original: Value = serde_json::from_str(&payload).unwrap();
        if source["sectionPath"] == "/authorRequirementsSource" {
            assert!(
                original.to_string().contains("写一章港城故事"),
                "{original}"
            );
        }
        originals[source["sectionPath"].as_str().unwrap()] =
            json!({"source":source,"content":original});
    }
    originals
}

async fn charge_budget(
    app: &Router,
    path: &std::path::Path,
    run_uri: &str,
    task_id: &str,
) -> Value {
    let root = run_uri.split("/generation-runs/").next().unwrap();
    // The trusted Host settles its already issued grant after terminal Task
    // state. This is billing metadata, not a model source/read permission.
    let stored: String = rusqlite::Connection::open(path).unwrap().query_row(
        "SELECT b.grant_json FROM story_task_provider_budgets_v1 b JOIN story_tasks_v2 t ON t.task_id=b.task_id WHERE b.task_id=?1 AND t.project_id=?2",
        rusqlite::params![task_id, root.rsplit('/').next().unwrap()], |row| row.get(0),
    ).unwrap();
    let grant: TaskProviderBudget = serde_json::from_str(&stored).unwrap();
    assert_eq!(grant.task_id, task_id);
    assert_eq!(
        grant.work_task_id,
        format!("work-{}", run_uri.rsplit('/').next().unwrap())
    );
    assert_eq!(grant.budget_id, format!("budget-{task_id}"));
    let mut budget = PersistentBudget::new(
        &path.with_extension("budget.sqlite3"),
        task_id,
        json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,
            "maxTokens":grant.max_tokens,"maxDurationMs":600000,"startedAt":1000},"now":1000}),
    )
    .unwrap();
    budget.complete("terminal").unwrap();
    let (status, charged) = call(
        app,
        "POST",
        &format!("{run_uri}/provider-usage"),
        AUTHOR,
        Some(json!({"taskId":task_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    charged
}

#[tokio::test]
async fn checkpoint_retry_preserves_history_and_rejects_external_edits() {
    for scenario in ["ownCheckpoint", "authorEditedCheckpoint"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("story.sqlite3");
        let app = router(
            task_fixture::state(path.clone(), AUTHOR.into(), CANDIDATE.into())
                .with_engine_budget_database(path.with_extension("budget.sqlite3")),
        );
        let (_, created) = call(
            &app,
            "POST",
            "/api/projects",
            AUTHOR,
            Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
        )
        .await;
        let project = created["project"]["id"].as_str().unwrap();
        let root = format!("/api/projects/{project}");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let (_,run) = call(&app,"POST",&format!("{root}/generation-runs"),AUTHOR,Some(json!({
            "expectedProjectRevision":0,"authorIntent":"写一章港城故事","targetChapters":1,"maxTaskAttempts":9,"maxProviderRequests":256,"maxProviderTokens":4000000,"deadlineAt":now+600
        }))).await;
        let uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
        let next = current_model_action(&app, &path, &root, &uri, "generateBrief").await;
        let mut create = json!({"operation":"generateBrief","inputRevision":0,"threadId":"first","target":next["action"]["target"],
            "authorIntent":"写一章港城故事","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600});
        let (status, task) = call(
            &app,
            "POST",
            &format!("{root}/tasks"),
            AUTHOR,
            Some(create.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let task_id = task["id"].as_str().unwrap();
        let mut payload = json!({"payload":{"premise":"抄写员发现密函"},"expectedRevision":0,"expectedCandidateRevisionId":null,
            "idempotencyKey":"checkpoint","taskId":task_id,"threadId":"first"});
        let (status, saved) = call(
            &app,
            "POST",
            &format!("{root}/artifacts/brief"),
            CANDIDATE,
            Some(payload.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        let (status,failed) = call(&app,"PATCH",&format!("{root}/tasks/{task_id}"),AUTHOR,
            Some(json!({"threadId":"first","status":"failed","error":"connection lost after checkpoint"}))).await;
        assert_eq!(status, StatusCode::OK, "{failed}");
        charge_budget(&app, &path, &uri, task_id).await;
        let (_, blocked) = call(
            &app,
            "POST",
            &format!("{uri}/next"),
            AUTHOR,
            /*payload*/ None,
        )
        .await;
        assert_eq!(blocked["run"]["blockedReason"], "executionBlocked");
        let project_id = ProjectId::parse(project.to_owned()).unwrap();
        let before = SqliteStoryRepository::open(&path)
            .unwrap()
            .task(&project_id, task_id)
            .unwrap()
            .unwrap();
        if scenario == "authorEditedCheckpoint" {
            let (status,edit) = call(&app,"POST",&format!("{root}/artifacts/brief"),AUTHOR,Some(json!({
                "payload":{"premise":"作者改为藏信"},"expectedRevision":0,"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"author-edit"
            }))).await;
            assert_eq!(status, StatusCode::CREATED, "{edit}");
        }
        let (status, retried) = call(
            &app,
            "POST",
            &format!("{uri}/retry-failed-task"),
            AUTHOR,
            Some(json!({"expectedVersion":blocked["run"]["version"]})),
        )
        .await;
        assert_eq!(
            SqliteStoryRepository::open(&path)
                .unwrap()
                .task(&project_id, task_id)
                .unwrap()
                .unwrap(),
            before
        );
        if scenario == "authorEditedCheckpoint" {
            assert!(status.is_client_error(), "{retried}");
            assert_eq!(
                call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await.1,
                blocked["run"]
            );
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{retried}");
        assert_eq!(
            (
                retried["stage"].clone(),
                retried["usedTaskAttempts"].clone(),
                retried["retryAttempts"].clone()
            ),
            (json!("coordinate"), json!(2), json!(1))
        );
        let (_, next) = call(
            &app,
            "POST",
            &format!("{uri}/next"),
            AUTHOR,
            /*payload*/ None,
        )
        .await;
        create["threadId"] = json!("second");
        create["target"] = next["action"]["target"].clone();
        let (status, replacement) =
            call(&app, "POST", &format!("{root}/tasks"), AUTHOR, Some(create)).await;
        assert_eq!(status, StatusCode::CREATED, "{replacement}");
        assert_ne!(replacement["id"], task["id"]);
        assert_eq!(
            replacement["inputManifest"]["expectedCandidateRevisionId"],
            saved["revisionId"]
        );
        assert_eq!(
            call(&app, "GET", &uri, AUTHOR, /*payload*/ None).await.1["usedTaskAttempts"],
            3
        );
        payload["expectedCandidateRevisionId"] = saved["revisionId"].clone();
        payload["idempotencyKey"] = json!("late-output");
        assert!(
            call(
                &app,
                "POST",
                &format!("{root}/artifacts/brief"),
                CANDIDATE,
                Some(payload.clone())
            )
            .await
            .0
            .is_client_error()
        );
        payload["taskId"] = replacement["id"].clone();
        payload["threadId"] = json!("second");
        payload["idempotencyKey"] = json!("resumed-output");
        payload["payload"]["premise"] = json!("抄写员完成藏信计划");
        let (status, saved) = call(
            &app,
            "POST",
            &format!("{root}/artifacts/brief"),
            CANDIDATE,
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        let (status, work) = call(
            &app,
            "GET",
            &format!("{root}/tasks/{task_id}/work-task"),
            AUTHOR,
            /*payload*/ None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{work}");
        let steps = work["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2);
        let decision_step = steps
            .iter()
            .find(|step| step["operation"] == "advisory")
            .unwrap();
        assert_eq!(decision_step["attempts"].as_array().unwrap().len(), 1);
        let write_step = steps
            .iter()
            .find(|step| step["operation"] == "generateBrief")
            .unwrap();
        assert_eq!(write_step["attempts"].as_array().unwrap().len(), 2);
    }
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
