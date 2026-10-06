use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;

const AUTHOR: &str = "analysis-author-token-1234567890123456";
const CANDIDATE: &str = "analysis-candidate-token-1234567890123";
const CAPABILITY: &str = "analysis-capability-token-12345678901";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Value,
    caller: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if !caller.is_empty() {
        request = request
            .header("x-novel-tool-name", "analyze_story_impact")
            .header("x-novel-root-thread", "discussion")
            .header("x-novel-caller-thread", caller);
    }
    let response = app
        .clone()
        .author_fixture_request(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"message":String::from_utf8_lossy(&bytes)})),
    )
}

fn setup(chapters: usize) -> (tempfile::TempDir, std::path::PathBuf, Router) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("analysis.sqlite3");
    fixture::build(&mut SqliteStoryRepository::open(&path).unwrap(), chapters);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    (directory, path, app)
}

async fn root(app: &Router, operation: &str, target: Value) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status, task) = call(app, "POST", "/api/projects/long-work-benchmark/tasks", AUTHOR,
        json!({"operation":operation,"inputRevision":0,"threadId":"discussion","target":target,
            "authorIntent":"分析本章的故事设定、伏笔和修改影响，不发布正文或确认事实",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}), "").await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    task
}

#[tokio::test]
async fn manual_save_only_tracks_freshness_and_requested_analysis_inherits_exact_authority_and_budget()
 {
    let (_directory, path, app) = setup(2);
    let mut saved_repo = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse("long-work-benchmark").unwrap();
    let original_task_count = saved_repo.tasks(&project).unwrap().len();
    let head = saved_repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap();
    let content =
        json!({"chapterNumber":1,"title":"手动改稿","body":"她把信放回旧钟内，决定独自赴约。"})
            .to_string();
    saved_repo
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("draft-chapter-1").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("manual-source").unwrap(),
            commit_id: CommitId::parse("manual-save").unwrap(),
            idempotency_key: IdempotencyKey::parse("manual-save").unwrap(),
            request_digest: content.clone(),
            content_digest: content.clone(),
            content_json: content,
            expected_project_revision: 0,
            expected_snapshot_id: head.snapshot_id,
            next_snapshot_id: SnapshotId::parse("snapshot-manual-saved").unwrap(),
            expected_candidate_revision_id: Some(RevisionId::parse("revision-1").unwrap()),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "手动改稿".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        saved_repo.tasks(&project).unwrap().len(),
        original_task_count
    );
    let freshness = saved_repo
        .modeling_outbox_page(ModelingOutboxQuery {
            project_id: &project,
            cursor: None,
            limit: 20,
        })
        .unwrap();
    assert_eq!(freshness["pendingCount"], 1);
    assert_eq!(freshness["data"][0]["sourceRevisionId"], "manual-source");
    assert_eq!(freshness["authority"], "sourceFreshnessOnly");
    drop(saved_repo);
    let task = root(&app, "advisory", json!({"type":"project"})).await;
    let id = task["id"].as_str().unwrap();
    let uri = format!("/api/projects/long-work-benchmark/tasks/{id}/actions");
    let request = json!({"taskId":id,"threadId":"discussion","expectedRevision":0,"expectedIntentRevision":0,
        "idempotencyKey":"knowledge-action","operation":"modelKnowledge",
        "target":{"chapterId":"chapter-1","chapterNumber":1,"sourceRevisionId":"manual-source"},"instruction":"分析本章有证据的设定"});
    for target in [
        json!({"chapterId":"foreign","chapterNumber":1,"sourceRevisionId":"manual-source"}),
        json!({"chapterId":"chapter-1","chapterNumber":2,"sourceRevisionId":"manual-source"}),
        json!({"chapterId":"chapter-1","chapterNumber":1,"sourceRevisionId":"revision-2"}),
    ] {
        let mut invalid = request.clone();
        invalid["target"] = target;
        assert!(
            !call(&app, "POST", &uri, CANDIDATE, invalid, "")
                .await
                .0
                .is_success()
        );
    }
    let mut old_intent = request.clone();
    old_intent["expectedIntentRevision"] = json!(1);
    assert!(
        !call(&app, "POST", &uri, CANDIDATE, old_intent, "")
            .await
            .0
            .is_success()
    );
    let (status, action) = call(&app, "POST", &uri, CANDIDATE, request.clone(), "").await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(
        call(&app, "POST", &uri, CANDIDATE, request, "").await.1,
        action
    );
    let action_id = action["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/projects/long-work-benchmark/tasks/{id}/complete"),
            AUTHOR,
            json!({"threadId":"discussion","summary":"登记分析"}),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    settle_fixture::settle(&path, "long-work-benchmark", id);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{uri}/{action_id}/approve"),
            AUTHOR,
            json!({"expectedIntentRevision":0}),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse("long-work-benchmark").unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-chapter-1").unwrap(),
            source_revision_id: &RevisionId::parse("manual-source").unwrap(),
            expected_revision: 0,
            extractor_version: "conversation-analysis",
            now: 5,
        })
        .unwrap();
    let wrong = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-chapter-2").unwrap(),
            source_revision_id: &RevisionId::parse("revision-2").unwrap(),
            expected_revision: 0,
            extractor_version: "conversation-analysis",
            now: 5,
        })
        .unwrap();
    let mut target = action["target"].clone();
    target["batchId"] = json!(wrong.id);
    target["requestedByTaskId"] = json!(id);
    target["requestedActionId"] = json!(action_id);
    let mut create = json!({"operation":"modelKnowledge","inputRevision":0,"threadId":"analysis","target":target,
        "authorIntent":"分析本章的故事设定、伏笔和修改影响，不发布正文或确认事实",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600});
    assert!(
        !call(
            &app,
            "POST",
            "/api/projects/long-work-benchmark/tasks",
            AUTHOR,
            create.clone(),
            ""
        )
        .await
        .0
        .is_success()
    );
    create["target"]["batchId"] = json!(batch.id);
    let (status, child) = call(
        &app,
        "POST",
        "/api/projects/long-work-benchmark/tasks",
        AUTHOR,
        create,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    let child_id = child["id"].as_str().unwrap();
    assert_eq!(
        repo.work_task_for_attempt(&project, child_id).unwrap().id,
        repo.work_task_for_attempt(&project, id).unwrap().id
    );
    assert_eq!(
        repo.task_provider_budget(&project, child_id, /*now*/ 5)
            .unwrap()
            .work_task_id,
        repo.work_task_for_attempt(&project, id).unwrap().id
    );
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch.id,
        task_id: child_id,
        thread_id: "analysis",
        entities: &[],
        assertions: &[],
        now: 6,
    })
    .unwrap();
    repo.complete_task(
        &project,
        child_id,
        "analysis",
        "分析候选已保存".into(),
        None,
        /*now*/ 7,
    )
    .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{uri}/{action_id}/finish"),
            AUTHOR,
            json!({"resultTaskId":child_id}),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        repo.branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id
            .as_str(),
        "snapshot-manual-saved"
    );
}

#[tokio::test]
async fn impact_is_root_advisory_only_readonly_and_rejects_stale_or_caller_snapshots() {
    let (_directory, path, app) = setup(2);
    let task = root(&app, "advisory", json!({"type":"project"})).await;
    let id = task["id"].as_str().unwrap();
    let uri = format!("/api/projects/long-work-benchmark/tasks/{id}/impact-analysis");
    let input = json!({"threadId":"discussion","expectedRevision":0,"entityId":"person-0","maxHops":1,"limit":1});
    let repo = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse("long-work-benchmark").unwrap();
    let before = repo.object_heads(&project).unwrap();
    let (status, report) = call(&app, "POST", &uri, CANDIDATE, input.clone(), "discussion").await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["baseSnapshotId"], "snapshot-captured");
    assert_eq!(report["taskId"], id);
    assert_eq!(report["coverage"]["globalImpactComplete"], false);
    let item = json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":json!({"content":[{"type":"text","text":report.to_string()}]}).to_string()});
    assert!(
        eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
            .unwrap()
            .as_u64()
            .unwrap()
            <= 9000
    );
    assert_eq!(repo.object_heads(&project).unwrap(), before);
    for (actor, caller) in [
        (AUTHOR, "discussion"),
        (CANDIDATE, ""),
        (CANDIDATE, "child"),
    ] {
        assert!(
            !call(&app, "POST", &uri, actor, input.clone(), caller)
                .await
                .0
                .is_success()
        );
    }
    let mut forged = input.clone();
    forged["snapshotId"] = json!("snapshot-initial");
    assert!(
        !call(&app, "POST", &uri, CANDIDATE, forged, "discussion")
            .await
            .0
            .is_success()
    );
    let mut other_thread = input.clone();
    other_thread["threadId"] = json!("other");
    assert!(
        !call(&app, "POST", &uri, CANDIDATE, other_thread, "discussion")
            .await
            .0
            .is_success()
    );
    let writer = root(&app,"generateChapter",json!({"chapterId":"chapter-1","chapterNumber":1,"readingChapter":1,"sceneOrder":1,"scope":{"type":"wholeChapter"}})).await;
    assert!(
        !call(
            &app,
            "POST",
            &format!(
                "/api/projects/long-work-benchmark/tasks/{}/impact-analysis",
                writer["id"].as_str().unwrap()
            ),
            CANDIDATE,
            input.clone(),
            "discussion"
        )
        .await
        .0
        .is_success()
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE project_id='long-work-benchmark' AND object_id='draft-chapter-1'",[]).unwrap();
    assert!(
        !call(&app, "POST", &uri, CANDIDATE, input, "discussion")
            .await
            .0
            .is_success()
    );
}

#[tokio::test]
async fn oversized_impact_never_delivers_a_model_item_or_changes_authority() {
    let (_directory, path, app) = setup(1);
    let project = ProjectId::parse("long-work-benchmark").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let source = repo
        .revision_content(&RevisionId::parse("revision-1").unwrap())
        .unwrap()
        .unwrap();
    let content: Value = serde_json::from_str(&source).unwrap();
    let quote: String = content["body"]
        .as_str()
        .unwrap()
        .chars()
        .take(1500)
        .collect();
    let short = json!({"quote":"章","start":0,"end":1});
    let mut entities: Vec<Value> = (0..7).map(|index|json!({"id":format!("event-{index}"),"kind":"event","displayName":format!("事件{index}"),"aliases":[],"evidence":short})).collect();
    let assertions: Vec<Value> = (1..7).map(|index| {
        entities.push(json!({"id":format!("cause-{index}"),"kind":"proposition","displayName":"有源因果候选","aliases":[],"evidence":short}));
        json!({"kind":"relation","subjectId":"event-0","targetId":format!("event-{index}"),"relationType":"causes","predicate":"causes",
            "propositionId":format!("cause-{index}"),"value":true,"validFrom":1,"evidence":{"quote":quote,"start":0,"end":1500}})
    }).collect();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &ObjectId::parse("draft-chapter-1").unwrap(),
            source_revision_id: &RevisionId::parse("revision-1").unwrap(),
            expected_revision: 0,
            extractor_version: "impact-budget",
            now: 5,
        })
        .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "large-impact-model".into(),
        project_id: project.clone(),
        thread_id: "large-impact".into(),
        operation: "modelKnowledge".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"batchId":batch.id}).to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(/*n*/ 64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: &batch.id,
        task_id: "large-impact-model",
        thread_id: "large-impact",
        entities: &entities,
        assertions: &assertions,
        now: 6,
    })
    .unwrap();
    repo.complete_task(
        &project,
        "large-impact-model",
        "large-impact",
        "有源影响压力样例".into(),
        None,
        /*now*/ 7,
    )
    .unwrap();
    let task = root(&app, "advisory", json!({"type":"project"})).await;
    let uri = format!(
        "/api/projects/long-work-benchmark/tasks/{}/impact-analysis",
        task["id"].as_str().unwrap()
    );
    let (status,report) = call(&app,"POST",&uri,CANDIDATE,json!({"threadId":"discussion","expectedRevision":0,"entityId":"event-0","maxHops":3,"limit":20}),"discussion").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{report}");
    assert_eq!(report["error"]["code"], "impact_model_item_budget_exceeded");
    assert!(report.get("findings").is_none());
    assert_eq!(
        SqliteStoryRepository::open(path)
            .unwrap()
            .task(
                &ProjectId::parse("long-work-benchmark").unwrap(),
                task["id"].as_str().unwrap()
            )
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );
}
