use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "../../story-core/tests/support/chapter_acceptance.rs"]
mod fixture;
use fixture::*;
const AUTHOR: &str = "chapter-accept-author-123456789012345678";
const CANDIDATE: &str = "chapter-accept-candidate-123456789012345";

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

#[tokio::test]
async fn http_partial_adoption_materializes_a_plus_b_preserves_candidate_and_can_undo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chapter.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let base = chapter(
        "正式标题",
        vec![group("a", "正式 A"), group("b", "正式 B")],
        json!({"planRevisionId":"base-plan"}),
    );
    let candidate = chapter(
        "候选标题",
        vec![group("a", "候选 A"), group("b", "候选 B")],
        json!({"planRevisionId":"new-plan"}),
    );
    setup(&mut repo, base.clone(), candidate.clone());
    let project_id = project();
    let object_id = ObjectId::parse("draft-chapter-one").unwrap();
    let source_revision = RevisionId::parse("candidate").unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project_id,
            object_id: &object_id,
            source_revision_id: &source_revision,
            expected_revision: 1,
            extractor_version: "ui-review-fixture",
            now: 2,
        })
        .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "model-ui-review".into(),
        project_id: project_id.clone(),
        thread_id: "model-ui-review".into(),
        operation: "modelKnowledge".into(),
        input_revision: 1,
        model: None,
        target_json: json!({"batchId":batch.id}).to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project_id,
        batch_id: &batch.id,
        task_id: "model-ui-review",
        thread_id: "model-ui-review",
        entities: &[],
        assertions: &[],
        now: 3,
    })
    .unwrap();
    repo.create_review(CreateReviewRequest {
        review_id: "review-current-ui".into(),
        project_id,
        object_id,
        object_kind: "draft".into(),
        revision_id: source_revision,
        idempotency_key: IdempotencyKey::parse("review-current-ui").unwrap(),
        request_digest: "current-ui".into(),
        checks_json: "[]".into(),
        proposals_json: "[]".into(),
        modeling_batch_id: Some(batch.id.clone()),
        selections_json: json!({"modelingBatchId":batch.id}).to_string(),
    })
    .unwrap();
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()));
    let uri = format!(
        "/api/projects/{}/reviews/review-current-ui/accept",
        project()
    );
    let selections = json!({"sceneGroupIds":["a"],"factDecisions":{},"noConfirmedChanges":true,"factDependencyLinks":[],"directionChangeDecisions":{},"manualDirectionDecisions":{},"modelingBatchId":batch.id});
    let request = json!({"expectedRevision":1,"idempotencyKey":"partial","selections":selections});
    assert_eq!(
        call(&app, "POST", &uri, request.clone(), CANDIDATE).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, receipt) = call(&app, "POST", &uri, request.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(call(&app, "POST", &uri, request, AUTHOR).await.1, receipt);
    assert_eq!(
        serde_json::from_str::<Value>(
            &repo
                .review(&project(), "review-current-ui")
                .unwrap()
                .unwrap()
                .selections_json
        )
        .unwrap(),
        selections
    );
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("/api/projects/{}/creative-workspace", project()),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        workspace["chapters"][0]["accepted"]["body"],
        "候选 A\n\n---\n\n正式 B"
    );
    assert_eq!(
        workspace["chapters"][0]["accepted"]["sceneGroups"],
        json!([candidate["sceneGroups"][0], base["sceneGroups"][1]])
    );
    assert_eq!(
        workspace["chapters"][0]["accepted"]["sourcePlanRefs"],
        base["sourcePlanRefs"]
    );
    assert_eq!(
        workspace["chapters"][0]["candidate"]["body"],
        candidate["body"]
    );
    assert_eq!(accepted(&repo)["title"], base["title"]);
    let (_, diffs) = call(
        &app,
        "GET",
        &format!("/api/projects/{}/chapters/chapter-one/diffs", project()),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        diffs["diffs"][0]["sceneChanges"],
        json!([{"sceneId":"scene-b","sceneGroupId":"b","kind":"modified","fromIndex":1,"toIndex":1,"before":base["sceneGroups"][1],"after":candidate["sceneGroups"][1]}])
    );
    assert_eq!(
        diffs["diffs"][0]["metadataChanges"],
        json!([{"field":"title","before":"正式标题","after":"候选标题"},{"field":"sourcePlanRefs","before":base["sourcePlanRefs"],"after":candidate["sourcePlanRefs"]}])
    );
    let (status, fresh) = call(&app,"POST",&format!("/api/projects/{}/reviews",project()),json!({"artifactKind":"draft","revisionId":"candidate","proposals":[],"selections":{},"idempotencyKey":"review-remaining"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{fresh}");
    assert_eq!(
        (fresh["status"].clone(), fresh["projectRevision"].clone()),
        (json!("ready"), json!(2))
    );
    assert_eq!(fresh["artifactRevisionId"], "candidate");
    let (status, undo) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/{}/commits/{}/undo",
            project(),
            receipt["commitId"].as_str().unwrap()
        ),
        json!({"expectedRevision":2,"idempotencyKey":"undo-partial"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undo}");
    assert_eq!(accepted(&repo), base);
}

#[tokio::test]
async fn http_select_all_adopts_order_deletion_and_metadata_while_invalid_choices_never_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("whole.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let base = chapter(
        "正式",
        vec![group("a", "A"), group("b", "B"), group("c", "C")],
        Value::Null,
    );
    let candidate = chapter(
        "候选",
        vec![group("c", "C"), group("a", "A")],
        json!({"localOnly":true}),
    );
    setup(&mut repo, base.clone(), candidate.clone());
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()));
    let uri = format!(
        "/api/projects/{}/reviews/review-candidate/accept",
        project()
    );
    for selection in [
        Value::Null,
        json!([]),
        json!({"sceneGroupIds":[]}),
        json!({"sceneGroupIds":["unknown"]}),
        json!({"sceneGroupIds":["a","a"]}),
    ] {
        let (status, _) = call(
            &app,
            "POST",
            &uri,
            json!({"expectedRevision":1,"idempotencyKey":"bad","selections":selection}),
            AUTHOR,
        )
        .await;
        assert!(!status.is_success());
        assert_eq!(accepted(&repo), base);
        assert_eq!(repo.project_revision(&project()).unwrap(), 1);
    }
    let (_, diffs) = call(
        &app,
        "GET",
        &format!("/api/projects/{}/chapters/chapter-one/diffs", project()),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(
        diffs["diffs"][0]["sceneChanges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| (change["sceneId"].clone(), change["kind"].clone()))
            .collect::<Vec<_>>(),
        vec![
            (json!("scene-a"), json!("moved")),
            (json!("scene-c"), json!("moved")),
            (json!("scene-b"), json!("deleted"))
        ]
    );
    let (status,receipt)=call(&app,"POST",&uri,json!({"expectedRevision":1,"idempotencyKey":"all","selections":{"sceneGroupIds":["c","a"],"noConfirmedChanges":true}}),AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(accepted(&repo), candidate);
    assert_eq!(repo.facts(&project()).unwrap(), Vec::<Value>::new());
    let (_, diffs) = call(
        &app,
        "GET",
        &format!("/api/projects/{}/chapters/chapter-one/diffs", project()),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(diffs, json!({"diffs":[]}));
}

#[tokio::test]
async fn grouped_proposal_preview_apply_and_undo_share_the_same_scene_merge() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("grouped.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let base = chapter(
        "正式",
        vec![group("a", "正式 A"), group("b", "正式 B")],
        Value::Null,
    );
    let candidate = chapter(
        "候选",
        vec![group("a", "候选 A"), group("b", "候选 B")],
        Value::Null,
    );
    setup(&mut repo, base.clone(), candidate);
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()));
    let root = format!("/api/projects/{}", project());
    let (status,brief)=call(&app,"POST",&format!("{root}/artifacts/brief"),json!({"payload":{"premise":"保留正文选择边界"},"expectedRevision":1,"expectedCandidateRevisionId":null,"idempotencyKey":"grouped-brief"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{brief}");
    let (status,brief_review)=call(&app,"POST",&format!("{root}/reviews"),json!({"artifactKind":"brief","revisionId":brief["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"grouped-brief-review"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{brief_review}");
    let snapshot = repo
        .branch_head(&project(), &branch())
        .unwrap()
        .unwrap()
        .snapshot_id;
    let (status,proposal)=call(&app,"POST",&format!("{root}/proposal-sets"),json!({"baseSnapshotId":snapshot.as_str(),"intent":"调整简报并只采用 A 场景","operations":[{"operationId":"brief","reviewId":brief_review["id"],"dependsOn":[]},{"operationId":"draft","reviewId":"review-candidate","dependsOn":[]}],"idempotencyKey":"grouped"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{proposal}");
    let uri = format!("{root}/proposal-sets/{}", proposal["id"].as_str().unwrap());
    let choices = json!({"selectedOperationIds":["brief","draft"],"selectionsByOperationId":{"brief":{},"draft":{"sceneGroupIds":["a"]}}});
    let (status, preview) = call(
        &app,
        "POST",
        &format!("{uri}/preview"),
        choices.clone(),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["selection"]["applyReady"], true);
    assert_eq!(accepted(&repo), base);
    let mut request = choices;
    request["idempotencyKey"] = json!("grouped-apply");
    let (status, receipt) = call(&app, "POST", &format!("{uri}/apply"), request, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(accepted(&repo)["body"], "候选 A\n\n---\n\n正式 B");
    assert_eq!(accepted(&repo)["title"], "正式");
    let (status, undo) = call(
        &app,
        "POST",
        &format!(
            "{root}/proposal-applications/{}/undo",
            receipt["applicationId"].as_str().unwrap()
        ),
        json!({"expectedProjectRevision":2,"idempotencyKey":"grouped-undo"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undo}");
    assert_eq!(accepted(&repo), base);
}

#[tokio::test]
async fn partial_adoption_confirms_only_facts_with_evidence_in_selected_candidate_scenes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("facts.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    setup(
        &mut repo,
        chapter("正式", vec![group("a", "A"), group("b", "B")], Value::Null),
        chapter(
            "候选",
            vec![group("a", "新 A"), group("b", "新 B")],
            Value::Null,
        ),
    );
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()));
    let root = format!("/api/projects/{}", project());
    let (status,review)=call(&app,"POST",&format!("{root}/reviews"),json!({"artifactKind":"draft","revisionId":"candidate","proposals":[{"sceneGroupId":"a","sceneId":"scene-a","predicate":"chosen","value":true,"evidence":"新 A"},{"sceneGroupId":"b","predicate":"unchosen","value":true,"evidence":"新 B"},{"sceneGroupId":"a","predicate":"wrongEvidence","value":true,"evidence":"新 B"}],"selections":{},"idempotencyKey":"facts-review"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let ids = review["factProposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fact| fact["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    let uri = format!("{root}/reviews/{}/accept", review["id"].as_str().unwrap());
    for id in [ids[1], ids[2]] {
        let (status,failed)=call(&app,"POST",&uri,json!({"expectedRevision":1,"idempotencyKey":"facts-adopt","selections":{"sceneGroupIds":["a"],"factDecisions":{id:"confirmed"}}}),AUTHOR).await;
        assert_eq!(status, StatusCode::CONFLICT, "{failed}");
        assert_eq!(repo.project_revision(&project()).unwrap(), 1);
        assert!(repo.facts(&project()).unwrap().is_empty());
    }
    let (status,adopted)=call(&app,"POST",&uri,json!({"expectedRevision":1,"idempotencyKey":"facts-adopt","selections":{"sceneGroupIds":["a"],"factDecisions":{ids[0]:"confirmed",ids[1]:"rejected",ids[2]:"rejected"}}}),AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{adopted}");
    assert_eq!(accepted(&repo)["body"], "新 A\n\n---\n\nB");
    assert_eq!(
        repo.facts(&project())
            .unwrap()
            .iter()
            .map(|fact| fact["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!(ids[0])]
    );
}

fn publish_chapter_analysis(repo: &mut SqliteStoryRepository, label: &str) -> String {
    let project_id = project();
    let object_id = ObjectId::parse("draft-chapter-one").unwrap();
    let source = RevisionId::parse("candidate").unwrap();
    let batch = repo
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project_id,
            object_id: &object_id,
            source_revision_id: &source,
            expected_revision: 1,
            extractor_version: label,
            now: 2,
        })
        .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: label.into(),
        project_id: project_id.clone(),
        thread_id: label.into(),
        operation: "modelKnowledge".into(),
        input_revision: 1,
        model: None,
        target_json: json!({"batchId":batch.id}).to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap();
    let evidence = json!({"quote":"C","start":0,"end":1});
    let entities = [
        json!({"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":evidence}),
    ];
    let assertions = [
        json!({"kind":"state","subjectId":"linxi","predicate":"arrived","value":true,
        "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":1,"evidence":evidence}),
    ];
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project_id,
        batch_id: &batch.id,
        task_id: label,
        thread_id: label,
        entities: &entities,
        assertions: &assertions,
        now: 3,
    })
    .unwrap();
    repo.complete_task(
        &project_id,
        label,
        label,
        "analysis saved".into(), None,
        /*now*/ 4,
    )
    .unwrap();
    batch.id
}

#[tokio::test]
async fn publication_checks_current_analysis_atomically_and_publishes_whole_chapter_with_facts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("publication-analysis.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let base = chapter(
        "已发布标题",
        vec![group("a", "A"), group("b", "B"), group("c", "C")],
        Value::Null,
    );
    let candidate = chapter(
        "新标题",
        vec![group("c", "C"), group("a", "A")],
        json!({"localOnly":true}),
    );
    setup(&mut repo, base, candidate.clone());
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()));
    let root = format!("/api/projects/{}", project());
    let before = (
        repo.project_revision(&project()).unwrap(),
        repo.branch_head(&project(), &branch()).unwrap(),
        repo.object_heads(&project()).unwrap(),
        repo.acceptances(&project()).unwrap(),
        repo.facts(&project()).unwrap(),
    );
    let mut previous_review = Value::Null;
    let mut previous_batch = String::new();
    for label in ["first-analysis", "second-analysis"] {
        let batch_id = publish_chapter_analysis(&mut repo, label);
        let previous_reviews = repo.reviews(&project()).unwrap();
        for (index, model) in [Value::Null, json!(batch_id)].into_iter().enumerate() {
            let (status, failure) = call(&app, "POST", &format!("{root}/reviews"), json!({"artifactKind":"draft","revisionId":"candidate","proposals":[],"modelingBatchId":model,"selections":{"modelingBatchId":if model.is_null() {batch_id.clone()} else {"forged-batch".to_owned()}},"idempotencyKey":format!("forged-{label}-{index}")}), AUTHOR).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{failure}");
            assert!(
                failure["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("分析批次与请求不一致")
            );
            assert_eq!(repo.reviews(&project()).unwrap(), previous_reviews);
        }
        let (status, review) = call(&app, "POST", &format!("{root}/reviews"), json!({"artifactKind":"draft","revisionId":"candidate","proposals":[],"modelingBatchId":batch_id,"selections":{},"idempotencyKey":label}), AUTHOR).await;
        assert_eq!(status, StatusCode::CREATED, "{review}");
        assert_eq!(review["factProposals"].as_array().unwrap().len(), 1);
        let stored = repo
            .review(&project(), review["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored.selections_json).unwrap(),
            json!({"modelingBatchId":batch_id})
        );
        let stale_id = if previous_review.is_null() {
            "review-candidate"
        } else {
            previous_review["id"].as_str().unwrap()
        };
        let stale_choices = if previous_review.is_null() {
            json!({"sceneGroupIds":["c","a"],"noConfirmedChanges":true})
        } else {
            json!({"sceneGroupIds":["c","a"],"modelingBatchId":previous_batch,"factDecisions":{previous_review["factProposals"][0]["id"].as_str().unwrap():"rejected"}})
        };
        let (status, failure) = call(&app, "POST", &format!("{root}/reviews/{stale_id}/accept"), json!({"expectedRevision":1,"idempotencyKey":format!("reject-{label}"),"selections":stale_choices}), AUTHOR).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{failure}");
        assert!(
            failure["error"]["message"]
                .as_str()
                .unwrap()
                .contains("重新准备发布检查")
        );
        assert_eq!(
            (
                repo.project_revision(&project()).unwrap(),
                repo.branch_head(&project(), &branch()).unwrap(),
                repo.object_heads(&project()).unwrap(),
                repo.acceptances(&project()).unwrap(),
                repo.facts(&project()).unwrap()
            ),
            before
        );
        let (status, failure) = call(&app, "POST", &format!("{root}/reviews/{}/accept", review["id"].as_str().unwrap()), json!({"expectedRevision":1,"idempotencyKey":format!("wrong-binding-{label}"),"selections":{"sceneGroupIds":["c","a"],"modelingBatchId":"forged-batch","noConfirmedChanges":true}}), AUTHOR).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{failure}");
        previous_review = review;
        previous_batch = batch_id;
    }
    let review_id = previous_review["id"].as_str().unwrap();
    let fact_id = previous_review["factProposals"][0]["id"].as_str().unwrap();
    let (status, failure) = call(&app, "POST", &format!("{root}/reviews/{review_id}/accept"), json!({"expectedRevision":1,"idempotencyKey":"missing-scenes","selections":{"modelingBatchId":previous_batch,"factDecisions":{fact_id:"confirmed"}}}), AUTHOR).await;
    assert_eq!(status, StatusCode::CONFLICT, "{failure}");
    assert_eq!(repo.project_revision(&project()).unwrap(), 1);
    assert_eq!(repo.facts(&project()).unwrap(), Vec::<Value>::new());
    let (status, receipt) = call(&app, "POST", &format!("{root}/reviews/{review_id}/accept"), json!({"expectedRevision":1,"idempotencyKey":"whole-with-facts","selections":{"sceneGroupIds":["c","a"],"factDecisions":{fact_id:"confirmed"}}}), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(accepted(&repo), candidate);
    assert_eq!(
        repo.facts(&project())
            .unwrap()
            .iter()
            .map(|fact| fact["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!(fact_id)]
    );
    assert_eq!(repo.project_revision(&project()).unwrap(), 2);
    let handwritten = chapter(
        "作者修改后的标题",
        vec![group("c", "手写 C"), group("a", "A")],
        json!({"localOnly":true}),
    );
    save(&mut repo, "handwritten", handwritten.clone());
    review(&mut repo, "handwritten");
    let (status, receipt) = call(&app, "POST", &format!("{root}/reviews/review-handwritten/accept"), json!({"expectedRevision":2,"idempotencyKey":"manual-after-analysis","selections":{"sceneGroupIds":["c","a"],"noConfirmedChanges":true}}), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(accepted(&repo), handwritten);
    assert_eq!(repo.project_revision(&project()).unwrap(), 3);
}
