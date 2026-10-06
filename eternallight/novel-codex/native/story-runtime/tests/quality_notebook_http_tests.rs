use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::{RuntimeState, router};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const AUTHOR: &str = "quality-author-token-1234567890123456";
const CANDIDATE: &str = "quality-candidate-token-123456789012";
const CAP: &str = "quality-capability-12345678901234567890";
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {actor}"))
                .header("x-novel-task-capability", CAP)
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn quality_reader_http_is_bound_scoped_and_requires_report_before_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("quality.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = ProjectId::parse("quality").unwrap();
    let branch = BranchId::parse("main").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: p.clone(),
            title: "雨城".into(),
            logline: "秘密设计说明不应出现在阅读包".into(),
            created_at: 1,
            updated_at: 1,
        },
        branch.clone(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: p.clone(),
        branch_id: branch,
        object_id: ObjectId::parse("draft-chapter").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source-a").unwrap(),
        commit_id: CommitId::parse("save-a").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-a").unwrap(),
        request_digest: "digest".into(),
        content_digest: "digest".into(),
        content_json:
            json!({"chapterNumber":1,"title":"雨","body":"她说不必再来，却将那把伞留在门边。"})
                .to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("saved").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter".into(),
            number: 1,
            title: "雨".into(),
        }),
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    let receipt = repo
        .change_quality_notebook(
            &p,
            QualityMutation {
                expected_version: 0,
                idempotency_key: "packet".into(),
                change: QualityAction::Packet {
                    kind: "reading".into(),
                    sources: vec![QualityPacketSource {
                        revision_id: Some("source-a".into()),
                        quote: None,
                        note_id: None,
                    }],
                },
            },
            None,
        )
        .unwrap();
    let packet = receipt["result"]["id"].as_str().unwrap().to_owned();
    let task = repo
        .create_task(CreateTaskRequest {
            task_id: "reader".into(),
            project_id: p.clone(),
            thread_id: "fresh-reader".into(),
            operation: "reviewQuality".into(),
            input_revision: 0,
            model: None,
            target_json: json!({"type":"project","qualityPacketId":packet}).to_string(),
            input_manifest_json: json!({"authorIntent":"阅读匿名材料"}).to_string(),
            capability_hash: format!("{:x}", Sha256::digest(CAP.as_bytes())),
            capability_expires_at: 9999999999,
        })
        .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "writer".into(), project_id: p.clone(), thread_id: "writer-thread".into(),
        operation: "generateChapter".into(), input_revision: 0, model: None,
        target_json: json!({"chapterId":"chapter"}).to_string(),
        input_manifest_json: json!({"authorIntent":"创作正文"}).to_string(),
        capability_hash: format!("{:x}", Sha256::digest(CAP.as_bytes())), capability_expires_at: 9999999999,
    }).unwrap();
    drop(repo);
    let app = router(
        RuntimeState::new(path.clone(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    );
    let route = "/api/projects/quality/tasks/reader/quality";
    for (action, expected) in [("read", StatusCode::OK), ("change", StatusCode::FORBIDDEN), ("page", StatusCode::FORBIDDEN)] {
        let (status, body) = call(&app, "POST", "/api/projects/quality/tasks/writer/quality", CANDIDATE,
            json!({"threadId":"writer-thread","expectedRevision":0,"action":action})).await;
        assert_eq!(status, expected, "{body}");
    }
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/projects/quality/quality-notebook",
            CANDIDATE,
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let binding = json!({"threadId":"fresh-reader","expectedRevision":0});
    for route in [
        "/api/projects/quality/context?purpose=critic&projectRevision=0&taskId=reader&threadId=fresh-reader",
        "/api/projects/quality/tasks/reader/task-source?threadId=fresh-reader&expectedRevision=0&sourceId=source-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let (status, body) = call(&app, "GET", route, CANDIDATE, Value::Null).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(body.to_string().contains("reader_packet_only"));
    }
    for action in ["read", "review", "change"] {
        let mut args = binding.clone();
        args["action"] = json!(action);
        assert_eq!(
            call(&app, "POST", route, CANDIDATE, args).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let mut args = binding.clone();
    args["action"] = json!("page");
    args["page"] = json!(0);
    let (status, page) = call(&app, "POST", route, CANDIDATE, args).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(!page.to_string().contains("秘密设计说明"));
    assert!(!page.to_string().contains("source-a"));
    let finish = json!({"threadId":task.thread_id,"summary":"尚未保存报告"});
    assert!(
        !call(
            &app,
            "POST",
            "/api/projects/quality/tasks/reader/complete",
            CANDIDATE,
            finish.clone()
        )
        .await
        .0
        .is_success()
    );
    let report = json!({"packetHash":page["packetHash"],"summary":"口头拒绝与动作相反","preference":"abstain","limitations":"局部文本，模型假设而非真人反馈","findings":[{"label":"A","quote":"却将那把伞留在门边","effect":"读者可能感到未说出的关心","confidence":"medium"}]});
    let mut args = binding;
    args["action"] = json!("report");
    args["report"] = report;
    let (status, saved) = call(&app, "POST", route, CANDIDATE, args).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (status, completed) = call(
        &app,
        "POST",
        "/api/projects/quality/tasks/reader/complete",
        CANDIDATE,
        finish,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let (status, notebook) = call(
        &app,
        "GET",
        "/api/projects/quality/quality-notebook",
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(notebook["packets"][0]["hasReport"], true);
    let (status, delivered) = call(
        &app,
        "GET",
        "/api/projects/quality/tasks/reader/work-task",
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{delivered}");
    assert_eq!(
        delivered["deliveryReport"]["evidence"][0]["kind"],
        "qualityReading"
    );
    assert_eq!(
        delivered["deliveryReport"]["evidence"][0]["state"],
        "recorded"
    );
    // Exercise the same explicit, budgeted author launch used by the desktop UI.
    let(status,prepared)=call(&app,"POST","/api/projects/quality/quality-notebook",AUTHOR,json!({"expectedVersion":notebook["version"],"idempotencyKey":"second-packet","change":{"action":"packet","kind":"reading","sources":[{"revisionId":"source-a","quote":null,"noteId":null}]}})).await;
    assert_eq!(status, StatusCode::OK, "{prepared}");
    let (status, workspace) = call(
        &app,
        "GET",
        "/api/projects/quality/creative-workspace",
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let request = json!({"operation":"reviewQuality","inputRevision":workspace["revision"],"threadId":"budgeted-reader","target":{"type":"project","qualityPacketId":prepared["result"]["id"]},"authorIntent":"阅读匿名材料","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":now+600,
       "initialBudget":{"authorizationId":"quality-confirmation","authorMessageId":"quality-author-message","expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"maxRequests":5,"maxTokens":12000,"deadlineAt":now+600}});
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects/quality/tasks",
        AUTHOR,
        request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["operation"], "reviewQuality");
    let (status, replayed) =
        call(&app, "POST", "/api/projects/quality/tasks", AUTHOR, request).await;
    assert_eq!(status, StatusCode::CREATED, "{replayed}");
    assert_eq!(created["id"], replayed["id"]);
    let (status, work) = call(
        &app,
        "GET",
        &format!(
            "/api/projects/quality/tasks/{}/work-task",
            created["id"].as_str().unwrap()
        ),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{work}");
    assert!(work["id"].as_str().unwrap().starts_with("work-"));
    let (status, failed) = call(
        &app,
        "PATCH",
        &format!(
            "/api/projects/quality/tasks/{}",
            created["id"].as_str().unwrap()
        ),
        AUTHOR,
        json!({"threadId":"budgeted-reader","status":"failed","error":"模拟可恢复的读者中断"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    let (status, blocked) = call(
        &app,
        "GET",
        &format!(
            "/api/projects/quality/tasks/{}/work-task",
            created["id"].as_str().unwrap()
        ),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{blocked}");
    assert_eq!(blocked["resumeBlockReason"], "readerRestart", "{blocked}");
}
