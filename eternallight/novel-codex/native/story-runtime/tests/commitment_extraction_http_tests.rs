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

const AUTHOR: &str = "extraction-author-token-1234567890123456";
const CANDIDATE: &str = "extraction-candidate-token-123456789012";
const CAPABILITY: &str = "extraction-capability-token-1234567890";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {actor}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn conversation_extraction_uses_bound_candidate_output_and_an_author_only_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("承诺提案.sqlite3");
    let project = ProjectId::parse("extraction-http").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "港城".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch.clone(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    for (chapter, body) in [(1, "白塔的灯将再亮。"), (2, "未来的秘密不可读取。")]
    {
        repo.save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: branch.clone(),
            object_id: ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse(format!("source-{chapter}")).unwrap(),
            commit_id: CommitId::parse(format!("save-{chapter}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("save-{chapter}")).unwrap(),
            request_digest: body.into(),
            content_digest: body.into(),
            content_json: json!({"chapterNumber":chapter,"body":body}).to_string(),
            expected_project_revision: 0,
            expected_snapshot_id: repo
                .branch_head(&project, &branch)
                .unwrap()
                .unwrap()
                .snapshot_id,
            next_snapshot_id: SnapshotId::parse(format!("snapshot-{chapter}")).unwrap(),
            expected_candidate_revision_id: None,
            chapter_projection: Some(ChapterProjection {
                chapter_id: format!("chapter-{chapter}"),
                number: chapter,
                title: body.into(),
            }),
            task_output: None,
            linked_impact_ids: vec![],
        })
        .unwrap();
    }
    drop(repo);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/projects/extraction-http/chapters/chapter-1/commitment-extractions",
            AUTHOR,
            /*body*/ None
        )
        .await,
        (StatusCode::OK, Value::Null)
    );
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let create = json!({"operation":"advisory","inputRevision":0,"threadId":"discussion","target":{"type":"project"},"authorIntent":"提取并保存第一章的伏笔候选",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, discussion) = call(
        &app,
        "POST",
        "/api/projects/extraction-http/tasks",
        AUTHOR,
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{discussion}");
    let root = discussion["id"].as_str().unwrap();
    let actions = format!("/api/projects/extraction-http/tasks/{root}/actions");
    let action_input = json!({"taskId":root,"threadId":"discussion","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"extract-request","operation":"extractCommitments",
        "target":{"chapterId":"chapter-1","chapterNumber":1,"sourceRevisionId":"source-1"},"instruction":"提取有原文证据的开放承诺候选"});
    let (status, action) = call(
        &app,
        "POST",
        &actions,
        CANDIDATE,
        Some(action_input.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(
        call(&app, "POST", &actions, CANDIDATE, Some(action_input))
            .await
            .1,
        action
    );
    let action_id = action["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/projects/extraction-http/tasks/{root}/complete"),
            AUTHOR,
            Some(json!({"threadId":"discussion","summary":"已经登记提取任务"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    settle_fixture::settle(&path, "extraction-http", root);
    let (status, approved) = call(
        &app,
        "POST",
        &format!("{actions}/{action_id}/approve"),
        AUTHOR,
        Some(json!({"expectedIntentRevision":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let mut create = create;
    create["operation"] = json!("extractCommitments");
    create["threadId"] = json!("extraction");
    create["target"] = action["target"].clone();
    create["target"]["requestedByTaskId"] = json!(root);
    create["target"]["requestedActionId"] = json!(action_id);
    let (status, task) = call(
        &app,
        "POST",
        "/api/projects/extraction-http/tasks",
        AUTHOR,
        Some(create),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "/api/projects/extraction-http/context?taskId={id}&threadId=extraction&projectRevision=0&purpose=critic&budget=24&byteBudget=60000"
    );
    let (status, pack) = call(&app, "GET", &context_uri, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["relevantText"].as_array().unwrap().len(), 1);
    assert_eq!(pack["relevantText"][0]["sourceRevisionId"], "source-1");
    assert!(!pack.to_string().contains("未来的秘密不可读取"));
    let submit = "/api/projects/extraction-http/chapters/chapter-1/commitment-extractions";
    let input = json!({"taskId":id,"threadId":"extraction","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"save-extraction",
        "payload":{"chapterId":"chapter-1","sourceRevisionId":"source-1","examinedRanges":[{"start":0,"end":8}],"candidates":[{
            "id":"lamp","kind":"foreshadowing","summary":"灯会再亮","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
            "setupEvidenceRefs":[{"objectId":"draft-chapter-1","revisionId":"source-1","quote":"白塔的灯","start":0,"end":4}],
            "fulfillmentCriteria":"后续出现再次亮灯的证据","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null}]}});
    assert_eq!(
        call(&app, "POST", submit, AUTHOR, Some(input.clone()))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, saved) = call(&app, "POST", submit, CANDIDATE, Some(input.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(
        call(&app, "POST", submit, CANDIDATE, Some(input)).await.1,
        saved
    );
    assert_eq!(
        call(&app, "GET", submit, CANDIDATE, /*body*/ None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, report) = call(&app, "GET", submit, AUTHOR, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["authority"], "candidateProposalBatch");
    assert_eq!(report["coverage"]["semanticCompleteness"], "unknown");
    let (status, completed) = call(
        &app,
        "POST",
        &format!("/api/projects/extraction-http/tasks/{id}/complete"),
        CANDIDATE,
        Some(
            json!({"threadId":"extraction","expectedRevision":0,"summary":"已保存一项开放承诺提案"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let work = SqliteStoryRepository::open(&path)
        .unwrap()
        .work_task(&project, &format!("work-{root}"))
        .unwrap();
    assert_eq!(work.status, "completed");
    assert_eq!(
        work.steps.last().unwrap().output_refs[0]["kind"],
        "commitmentExtraction"
    );
    assert_ne!(call(&app,"POST",submit,CANDIDATE,Some(json!({"taskId":id,"threadId":"extraction","expectedRevision":0,"idempotencyKey":"late","payload":{}}))).await.0,StatusCode::CREATED);

    let (_, report) = call(&app, "GET", submit, AUTHOR, /*body*/ None).await;
    let establish_uri = format!("{submit}/establish");
    let choice = json!({"reportRevisionId":saved["revisionId"],"proposalId":"lamp","baseSnapshotId":report["snapshotId"],
        "expectedRevision":report["projectRevision"],"idempotencyKey":"author-choice"});
    assert_eq!(
        call(
            &app,
            "POST",
            &establish_uri,
            CANDIDATE,
            Some(choice.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, established) =
        call(&app, "POST", &establish_uri, AUTHOR, Some(choice.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{established}");
    assert_eq!(
        call(&app, "POST", &establish_uri, AUTHOR, Some(choice.clone())).await,
        (status, established.clone())
    );
    assert_eq!(established["confirmationState"], "candidate");
    let (_, current_report) = call(
        &app,
        "GET",
        "/api/projects/extraction-http/chapters/chapter-1/commitment-extractions",
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(
        current_report["established"],
        json!([{ "proposalId": "lamp", "commitmentId": established["commitmentId"] }])
    );
    let commitment_uri = format!(
        "/api/projects/extraction-http/commitments/{}",
        established["commitmentId"].as_str().unwrap()
    );
    let (status, independent) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{independent}");
    assert_eq!(independent["confirmationCommitId"], Value::Null);
    assert_eq!(independent["authorControls"]["projectRevision"], 0);
    assert_eq!(
        independent["extractionOrigin"],
        json!({"chapterId":"chapter-1","reportRevisionId":saved["revisionId"],"proposalId":"lamp"})
    );
    let (status, review) = call(
        &app,
        "POST",
        &format!("{commitment_uri}/reviews"),
        AUTHOR,
        Some(json!({"revisionId":established["revisionId"],"idempotencyKey":"review-established"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, accepted) = call(&app,"POST",&format!("/api/projects/extraction-http/reviews/{}/accept",review["id"].as_str().unwrap()),AUTHOR,
        Some(json!({"expectedRevision":report["projectRevision"],"idempotencyKey":"accept-established","selections":{}}))).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let (_, confirmed) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(confirmed["confirmationCommitId"], accepted["commitId"]);
    assert_eq!(
        confirmed["authorControls"],
        json!({"projectRevision":1,"undoAvailable":true})
    );
    assert_eq!(
        call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None)
            .await
            .1["confirmationState"],
        "confirmed"
    );
    assert_eq!(
        call(&app, "POST", &establish_uri, AUTHOR, Some(choice))
            .await
            .1,
        established
    );
    let (status, undone) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/extraction-http/commits/{}/undo",
            accepted["commitId"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":1,"idempotencyKey":"undo-established"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let (_, withdrawn) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(withdrawn["confirmationCommitId"], Value::Null);
    assert_eq!(withdrawn["authorControls"]["projectRevision"], 2);
    assert_eq!(withdrawn["sourceCurrent"], true);
    assert_eq!(withdrawn["authorControls"]["undoAvailable"], false);
    assert_eq!(
        call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None)
            .await
            .1["confirmationState"],
        "candidate"
    );
    let mut adjusted = withdrawn["payload"].clone();
    adjusted["summary"] = json!("作者重新决定这条伏笔");
    adjusted["status"] = json!("abandoned");
    adjusted["decisionReason"] = json!("转为氛围描写，不再承诺后文解释");
    adjusted["targetWindow"] =
        json!({"kind":"chapterRange","fromChapterId":"chapter-1","throughChapterId":"chapter-2"});
    let edit_request = json!({"payload":adjusted,"extractionOrigin":withdrawn["extractionOrigin"],"expectedRevision":2,
        "baseSnapshotId":withdrawn["snapshotId"],"expectedCandidateRevisionId":withdrawn["revisionId"],"idempotencyKey":"edit-established"});
    assert_eq!(
        call(
            &app,
            "POST",
            &commitment_uri,
            CANDIDATE,
            Some(edit_request.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, edited) = call(
        &app,
        "POST",
        &commitment_uri,
        AUTHOR,
        Some(edit_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    assert_eq!(
        call(&app, "POST", &commitment_uri, AUTHOR, Some(edit_request)).await,
        (status, edited.clone())
    );
    let (_, edited_detail) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(edited_detail["payload"], adjusted);
    assert_eq!(
        edited_detail["extractionOrigin"],
        withdrawn["extractionOrigin"]
    );
    assert_eq!(edited_detail["confirmationState"], "candidate");
    let (status, second_review) = call(
        &app,
        "POST",
        &format!("{commitment_uri}/reviews"),
        AUTHOR,
        Some(json!({"revisionId":edited["revisionId"],"idempotencyKey":"review-again"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, second_accept) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/extraction-http/reviews/{}/accept",
            second_review["id"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":2,"idempotencyKey":"accept-again","selections":{}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second_accept}");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: branch.clone(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("source-changed").unwrap(),
        commit_id: CommitId::parse("save-changed").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-changed").unwrap(),
        request_digest: "changed-request".into(),
        content_digest: "changed-content".into(),
        content_json: json!({"chapterNumber":1,"body":"白塔的灯不再亮。"}).to_string(),
        expected_project_revision: 3,
        expected_snapshot_id: repo
            .branch_head(&project, &branch)
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse("snapshot-changed").unwrap(),
        expected_candidate_revision_id: Some(RevisionId::parse("source-1").unwrap()),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    drop(repo);
    let (_, stale) = call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(stale["sourceCurrent"], false);
    assert_eq!(stale["confirmationCommitId"], second_accept["commitId"]);
    assert_eq!(stale["authorControls"]["undoAvailable"], false);
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/extraction-http/commits/{}/undo",
            second_accept["commitId"].as_str().unwrap()
        ),
        AUTHOR,
        Some(json!({"expectedRevision":3,"idempotencyKey":"undo-stale"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        call(&app, "GET", &commitment_uri, AUTHOR, /*body*/ None)
            .await
            .1,
        stale
    );
    let repo = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repo.object_heads(&project)
            .unwrap()
            .into_iter()
            .find(|head| head.object_id.as_str() == "draft-chapter-1")
            .unwrap()
            .candidate_revision_id
            .unwrap()
            .as_str(),
        "source-changed"
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;
