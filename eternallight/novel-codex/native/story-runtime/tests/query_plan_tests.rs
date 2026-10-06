use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{
    BeginModelingBatch, BranchId, ChapterProjection, CommitId, CreateTaskRequest, IdempotencyKey,
    ObjectId, ProjectId, PublishModelingBatch, RevisionId, SaveCandidateRequest, SnapshotId,
    SqliteStoryRepository, TaskUpdate,
};
use novel_codex_story_runtime::router;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../story-core/examples/support/long_work_fixture.rs"]
mod fixture;

#[path = "support/revision_query_checks.rs"]
mod revision_query_checks;

const AUTHOR: &str = "query-author-token-123456789012345678";
const CANDIDATE: &str = "query-candidate-token-123456789012345";
const CAPABILITY: &str = "query-plan-capability-12345678901234";

fn encode_plan(plan: &Value) -> String {
    plan.to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect()
}

#[tokio::test]
async fn context_composes_only_runtime_executed_bound_query_reports_with_one_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("context-query.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 4);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let id = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let plan = json!({"question":"人物在白塔的状态","byteBudget":4000,"ranking":{"type":"literal"},"steps":[{"id":"state","operation":{"type":"state","entityId":"person-0","perspective":"author"}}]});
    let encoded = encode_plan(&plan);
    let uri = format!(
        "/api/projects/{project}/context?taskId={id}&threadId=query-thread&projectRevision=0&purpose=planner&byteBudget=60000&queryPlan={encoded}"
    );
    let (status, pack) = call(&app, "GET", &uri, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    let canonical_plan = serde_json::to_value(
        serde_json::from_value::<novel_codex_story_core::StoryQueryPlan>(plan.clone()).unwrap(),
    )
    .unwrap();
    assert_eq!(pack["queryReport"]["plan"], canonical_plan);
    assert_eq!(pack["queryReport"]["snapshotId"], snapshot.as_str());
    assert_eq!(pack["queryReport"]["taskId"], pack["taskId"]);
    assert_eq!(
        pack["queryReport"]["sourceWatermarks"],
        repository.project_source_watermarks(&project).unwrap()
    );
    assert_eq!(pack["queryReport"]["coverage"]["complete"], false);
    assert!(
        pack["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("queryRetrieval"))
    );
    assert_eq!(pack["coverage"]["complete"], false);
    assert_eq!(pack["budgetsUsed"]["byteSize"], pack.to_string().len());
    assert!(pack.to_string().len() <= 60_000);
    let mut content = pack.clone();
    content.as_object_mut().unwrap().remove("id");
    content.as_object_mut().unwrap().remove("contentHash");
    assert_eq!(
        pack["contentHash"],
        format!("{:x}", Sha256::digest(content.to_string().as_bytes()))
    );
    let mut report = pack["queryReport"].clone();
    let hash = report["contentHash"].clone();
    report["contentHash"] = json!("");
    assert_eq!(
        hash,
        format!("{:x}", Sha256::digest(report.to_string().as_bytes()))
    );
    assert_eq!(
        call(&app, "GET", &uri, Value::Null, CANDIDATE).await,
        (StatusCode::OK, pack)
    );
    let small = uri.replace("byteBudget=60000", "byteBudget=4000");
    let (status, error) = call(&app, "GET", &small, Value::Null, CANDIDATE).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    let author_uri = format!(
        "/api/projects/{project}/context?projectRevision=0&purpose=planner&queryPlan={encoded}"
    );
    assert_eq!(
        call(&app, "GET", &author_uri, Value::Null, AUTHOR).await.0,
        StatusCode::FORBIDDEN
    );
    let writer = task(
        &app,
        &project,
        "generateChapter",
        json!({"chapterId":"chapter-3","chapterNumber":3,"readingChapter":3,"sceneOrder":2}),
    )
    .await;
    let writer_uri = format!(
        "/api/projects/{project}/context?taskId={writer}&threadId=query-thread&projectRevision=0&purpose=writer&readingChapter=3&sceneOrder=2&queryPlan={encoded}"
    );
    assert_eq!(
        call(&app, "GET", &writer_uri, Value::Null, CANDIDATE)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let mut invalid = plan.clone();
    invalid["uploadedReport"] = json!({"coverage":{"complete":true}});
    assert_eq!(
        call(
            &app,
            "GET",
            &uri.replace(&encoded, &encode_plan(&invalid)),
            Value::Null,
            CANDIDATE
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{uri}&queryReport=forged"),
            Value::Null,
            CANDIDATE
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    repository
        .update_task(
            &project,
            &id,
            "query-thread",
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 10,
            },
        )
        .unwrap();
    assert_ne!(
        call(&app, "GET", &uri, Value::Null, CANDIDATE).await.0,
        StatusCode::OK
    );
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn task(app: &Router, project: &ProjectId, operation: &str, target: Value) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status,value) = call(app,"POST",&format!("/api/projects/{project}/tasks"),
        json!({"operation":operation,"inputRevision":0,"threadId":"query-thread","target":target,
            "authorIntent":"分析林夕","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn advisory_query_combines_bound_evidence_and_reports_whole_package_truncation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("queries.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 4);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let id = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let uri = format!("/api/projects/{project}/tasks/{id}/query");
    let body = json!({"threadId":"query-thread","expectedRevision":0,"plan":{"question":"林夕第一章知道什么，在哪里？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"text","operation":{"type":"text","query":fixture::marker(1),"limit":2}},
            {"id":"state","operation":{"type":"state","entityId":"location-1-0","predicate":"knows","perspective":"character","ownerId":"person-0","epistemicTime":1}},
            {"id":"graph","operation":{"type":"relations","entityId":"person-0","relationType":"locatedAt","direction":"outgoing","worldTime":1,"maxHops":1,"limit":2}}]}});
    let (status, report) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["snapshotId"], snapshot.as_str());
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        "revision-1"
    );
    assert_eq!(report["steps"][1]["results"][0]["sourceChapter"], 1);
    assert_eq!(
        report["steps"][2]["results"][0]["sourceRevisionId"],
        "revision-1"
    );
    assert_eq!(
        report["sourceWatermarks"],
        repository.project_source_watermarks(&project).unwrap()
    );
    assert_eq!(report["coverage"]["semanticCompleteness"], "unknown");
    let dictionary = report["evidence"].as_object().unwrap();
    for (index, pointer) in [
        (0, "/evidenceRef"),
        (1, "/assertion/evidenceRef"),
        (2, "/evidenceRef"),
    ] {
        let row = &report["steps"][index]["results"][0];
        let id = row.pointer(pointer).unwrap().as_str().unwrap();
        assert_eq!(dictionary[id]["sourceRevisionId"], row["sourceRevisionId"]);
        let path = if index == 0 {
            row["sourcePath"].clone()
        } else {
            json!("/body")
        };
        assert_eq!(dictionary[id]["sourcePath"], path);
        let quote = dictionary[id]["quote"].as_str().unwrap();
        assert!(!quote.is_empty());
        assert_eq!(
            dictionary[id]["end"].as_u64().unwrap() - dictionary[id]["start"].as_u64().unwrap(),
            quote.chars().count() as u64
        );
    }
    assert_eq!(report["ranking"]["semantic"], false);
    assert_eq!(
        report["steps"][1]["results"][0]["assertion"]["evidenceRef"],
        report["steps"][2]["results"][0]["evidenceRef"]
    );
    let mut unhashed = report.clone();
    unhashed["contentHash"] = json!("");
    assert_eq!(
        report["contentHash"],
        format!("{:x}", Sha256::digest(unhashed.to_string().as_bytes()))
    );
    assert!(report.to_string().len() <= 9000);
    let (status, repeat) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    assert_eq!((status, repeat), (StatusCode::OK, report));
    let mut wide = body.clone();
    wide["plan"]["byteBudget"] = json!(4000);
    wide["plan"]["steps"] = json!([{"id":"all","operation":{"type":"state","entityId":"person-0","perspective":"author"}}]);
    let (status, bounded) = call(&app, "POST", &uri, wide, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{bounded}");
    assert!(bounded.to_string().len() <= 4000);
    assert_eq!(
        bounded["coverage"],
        json!({"complete":false,"semanticCompleteness":"unknown","responseTruncated":true})
    );
    assert!(
        bounded["steps"][0]["returnedItems"].as_u64()
            < bounded["steps"][0]["originalReturnedItems"].as_u64()
    );
    let (status, _) = call(&app, "POST", &uri, body.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let mut invalid = body.clone();
    invalid["plan"]["steps"][0]["operation"]["sql"] = json!("SELECT *");
    assert_eq!(
        call(&app, "POST", &uri, invalid, CANDIDATE).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut invalid = body.clone();
    invalid["plan"]["steps"][1]["id"] = json!("text");
    assert_eq!(
        call(&app, "POST", &uri, invalid, CANDIDATE).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut invalid = body.clone();
    invalid["plan"]["steps"][2]["operation"]["maxHops"] = json!(4);
    assert_eq!(
        call(&app, "POST", &uri, invalid, CANDIDATE).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut invalid = body.clone();
    invalid["threadId"] = json!("another-thread");
    assert_ne!(
        call(&app, "POST", &uri, invalid, CANDIDATE).await.0,
        StatusCode::OK
    );
    repository
        .update_task(
            &project,
            &id,
            "query-thread",
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 10,
            },
        )
        .unwrap();
    assert_ne!(
        call(&app, "POST", &uri, body, CANDIDATE).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn global_plan_cannot_be_used_by_a_writer_and_stales_after_source_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stale-queries.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = fixture::build(&mut repository, 2);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let body = json!({"threadId":"query-thread","expectedRevision":0,"plan":{"question":"第一章","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"text","operation":{"type":"text","query":fixture::marker(1),"limit":1}}]}});
    let writer = task(&app,&project,"generateChapter",json!({"chapterId":"chapter-1","chapterNumber":1,"readingChapter":1,"povCharacterId":"person-0","storyTime":1})).await;
    let uri = format!("/api/projects/{project}/tasks/{writer}/query");
    let mut global = body.clone();
    global["plan"]["steps"] = json!([{"id":"global","operation":{"type":"state","entityId":"person-0","perspective":"author"}}]);
    assert_eq!(
        call(&app, "POST", &uri, global, CANDIDATE).await.0,
        StatusCode::FORBIDDEN
    );
    let advisory = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let uri = format!("/api/projects/{project}/tasks/{advisory}/query");
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("draft-chapter-1").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("query-source-edit").unwrap(),
            commit_id: CommitId::parse("query-source-edit").unwrap(),
            idempotency_key: IdempotencyKey::parse("query-source-edit").unwrap(),
            request_digest: "query-source-edit".into(),
            content_digest: "query-source-edit".into(),
            content_json: json!({"chapterNumber":1,"title":"改稿","body":"新故事。"}).to_string(),
            expected_project_revision: 0,
            expected_snapshot_id: snapshot,
            next_snapshot_id: SnapshotId::parse("query-source-edit-snapshot").unwrap(),
            expected_candidate_revision_id: Some(RevisionId::parse("revision-1").unwrap()),
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "改稿".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        call(&app, "POST", &uri, body, CANDIDATE).await.0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn name_resolution_keeps_distinct_candidates_without_implicitly_querying_one() {
    use pretty_assertions::assert_eq;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ambiguous-queries.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repository, 2);
    let object = ObjectId::parse("draft-chapter-2").unwrap();
    let revision = RevisionId::parse("revision-2").unwrap();
    let batch = repository
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &object,
            source_revision_id: &revision,
            expected_revision: 0,
            extractor_version: "ambiguity-test",
            now: 5,
        })
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "ambiguity-modeling".into(),
            project_id: project.clone(),
            thread_id: "ambiguity-modeling".into(),
            operation: "modelKnowledge".into(),
            input_revision: 0,
            model: None,
            target_json: json!({"batchId":batch.id}).to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    let name = format!("人物{}", char::from_u32(0x5000).unwrap());
    let quote = format!("{name}在港城并知晓此事");
    let start = fixture::marker(2).chars().count();
    let entities = [
        json!({"id":"different-person","kind":"character","displayName":name,"aliases":[],
        "evidence":{"quote":quote,"start":start,"end":start+quote.chars().count()}}),
    ];
    repository
        .publish_modeling_batch(PublishModelingBatch {
            project_id: &project,
            batch_id: &batch.id,
            task_id: "ambiguity-modeling",
            thread_id: "ambiguity-modeling",
            entities: &entities,
            assertions: &[],
            now: 6,
        })
        .unwrap();
    repository
        .complete_task(
            &project,
            "ambiguity-modeling",
            "ambiguity-modeling",
            "published".into(), None,
            /*now*/ 7,
        )
        .unwrap();
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let id = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let uri = format!("/api/projects/{project}/tasks/{id}/query");
    let body = json!({"threadId":"query-thread","expectedRevision":0,"plan":{"question":"同名人物是谁？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"identity","operation":{"type":"resolveEntity","name":name}}]}});
    let (status, result) = call(&app, "POST", &uri, body.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let ids = result["steps"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["different-person", "person-0"]);
    assert_eq!(
        result["steps"][0]["coverage"]["requiresDisambiguation"],
        true
    );
    assert_eq!(result["coverage"]["complete"], false);
    assert_eq!(result["steps"].as_array().unwrap().len(), 1);

    assert_eq!(
        result["steps"][0]["coverage"]["identityDecisions"],
        json!([])
    );
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let mut last_commit = String::new();
    let mut identity_candidate = Value::Null;
    for (revision, decisions) in [
        (
            0,
            json!([{"kind":"distinct","leftId":"different-person","rightId":"person-0"}]),
        ),
        (
            1,
            json!([{"kind":"same","fromId":"different-person","toId":"person-0"}]),
        ),
    ] {
        let (status, candidate) = call(&app,"POST",&format!("/api/projects/{project}/artifacts/identity"),
            json!({"payload":{"decisions":decisions},"expectedRevision":revision,"expectedCandidateRevisionId":identity_candidate,
                "idempotencyKey":format!("identity-{revision}")}),AUTHOR).await;
        assert_eq!(status, StatusCode::CREATED, "{candidate}");
        identity_candidate = candidate["revisionId"].clone();
        let (status, review) = call(&app,"POST",&format!("/api/projects/{project}/reviews"),
            json!({"artifactKind":"identity","revisionId":candidate["revisionId"],"proposals":[],"selections":{},
                "idempotencyKey":format!("identity-review-{revision}")}),AUTHOR).await;
        assert_eq!(status, StatusCode::CREATED, "{review}");
        let (status, accepted) = call(&app,"POST",&format!("/api/projects/{project}/reviews/{}/accept",review["id"].as_str().unwrap()),
            json!({"expectedRevision":revision,"idempotencyKey":format!("identity-accept-{revision}"),"selections":{}}),AUTHOR).await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        last_commit = accepted["commitId"].as_str().unwrap().to_owned();
        let (status, fresh) = call(&app,"POST",&format!("/api/projects/{project}/tasks"),json!({
            "operation":"advisory","inputRevision":revision+1,"threadId":"query-thread","target":{"type":"project"},
            "authorIntent":"这两个同名人物是不是同一人？只核对，不修改",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires}),AUTHOR).await;
        assert_eq!(status, StatusCode::CREATED, "{fresh}");
        let query_uri = format!(
            "/api/projects/{project}/tasks/{}/query",
            fresh["id"].as_str().unwrap()
        );
        let mut request = body.clone();
        request["expectedRevision"] = json!(revision + 1);
        let before = repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap();
        let (status, answer) = call(&app, "POST", &query_uri, request, CANDIDATE).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let coverage = &answer["steps"][0]["coverage"];
        assert_eq!(coverage["identityDecisions"], decisions);
        assert_eq!(coverage["identityRevisionId"], candidate["revisionId"]);
        if revision == 0 {
            assert_eq!(coverage["ambiguousNames"][0]["authorDecidedDistinct"], true);
        } else {
            assert_eq!(coverage["ambiguousNames"], json!([]));
        }
        assert_eq!(
            repository
                .branch_head(&project, &BranchId::parse("main").unwrap())
                .unwrap(),
            before
        );
        assert_eq!(
            repository
                .task(&project, fresh["id"].as_str().unwrap())
                .unwrap()
                .unwrap()
                .output_refs_json,
            "[]"
        );
        if revision == 1 {
            let (status, writer) = call(&app,"POST",&format!("/api/projects/{project}/tasks"),json!({
                "operation":"generateChapter","inputRevision":2,"threadId":"query-thread",
                "target":{"chapterId":"chapter-2","chapterNumber":2,"readingChapter":2},
                "authorIntent":"只续写第二章", "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires}),AUTHOR).await;
            assert_eq!(status, StatusCode::CREATED, "{writer}");
            let mut bounded = body.clone();
            bounded["expectedRevision"] = json!(2);
            let (status, answer) = call(
                &app,
                "POST",
                &format!(
                    "/api/projects/{project}/tasks/{}/query",
                    writer["id"].as_str().unwrap()
                ),
                bounded,
                CANDIDATE,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{answer}");
            assert_eq!(
                answer["steps"][0]["coverage"]["identityRevisionId"],
                Value::Null
            );
            assert_eq!(
                answer["steps"][0]["coverage"]["identityDecisions"],
                json!([])
            );
            assert!(!answer.to_string().contains("different-person"));
        }
        assert_eq!(
            call(&app, "POST", &uri, body.clone(), CANDIDATE).await.0,
            StatusCode::CONFLICT
        );
    }
    let (status, undone) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/commits/{last_commit}/undo"),
        json!({"expectedRevision":2,"idempotencyKey":"identity-query-undo"}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let (status, fresh) = call(&app,"POST",&format!("/api/projects/{project}/tasks"),json!({
        "operation":"advisory","inputRevision":3,"threadId":"query-thread","target":{"type":"project"},
        "authorIntent":"撤销后核对这两个同名人物", "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{fresh}");
    let mut request = body;
    request["expectedRevision"] = json!(3);
    let (status, answer) = call(
        &app,
        "POST",
        &format!(
            "/api/projects/{project}/tasks/{}/query",
            fresh["id"].as_str().unwrap()
        ),
        request,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(
        answer["steps"][0]["coverage"]["identityDecisions"],
        json!([{"kind":"distinct","leftId":"different-person","rightId":"person-0"}])
    );
    assert_eq!(
        answer["steps"][0]["coverage"]["ambiguousNames"][0]["authorDecidedDistinct"],
        true
    );
}

#[tokio::test]
async fn package_budget_never_returns_a_cursor_that_skips_removed_text_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query-cursor.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repository, 20);
    let raw = repository
        .search_documents(novel_codex_story_core::SearchQuery {
            project_id: &project,
            snapshot_id: None,
            text: "港城",
            kind: Some("draft"),
            scope: novel_codex_story_core::SearchScope::All,
            limit: 20,
            cursor: None,
        })
        .unwrap();
    assert!(raw["nextCursor"].is_string(), "{raw}");
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let id = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let body = json!({"threadId":"query-thread","expectedRevision":0,"plan":{"question":"港城有哪些相关原文？","byteBudget":4000,"ranking":{"type":"literal"},
        "steps":[{"id":"text","operation":{"type":"text","query":"港城","limit":20}}]}});
    let (status, report) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/tasks/{id}/query"),
        body,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert!(report.to_string().len() <= 4000);
    assert_eq!(report["steps"][0]["originalReturnedItems"], 20);
    assert!(report["steps"][0]["returnedItems"].as_u64().unwrap() < 20);
    assert_eq!(report["steps"][0]["nextCursor"], Value::Null);
    assert_eq!(
        report["steps"][0]["coverage"]["cursorWithheldAfterBudget"],
        true
    );
    assert_eq!(report["steps"][0]["coverage"]["requeryRequired"], true);
    assert_eq!(report["coverage"]["complete"], false);
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
