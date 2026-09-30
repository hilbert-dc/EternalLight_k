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

const AUTHOR: &str = "passage-author-token-123456789012345";
const CANDIDATE: &str = "passage-candidate-token-123456789012";
const CAPABILITY: &str = "passage-capability-12345678901234567";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    payload: Value,
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
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"frameworkRejection":String::from_utf8_lossy(&bytes)})),
    )
}

async fn task(app: &Router, project: &ProjectId, operation: &str, target: Value) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (status,value) = call(app,"POST",&format!("/api/projects/{project}/tasks"),json!({"operation":operation,
        "inputRevision":0,"threadId":"passage-thread","target":target,"authorIntent":"按含义寻找行程中断的原文",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    format!(
        "/api/projects/{project}/tasks/{}/query",
        value["id"].as_str().unwrap()
    )
}

fn request() -> Value {
    json!({"threadId":"passage-thread","expectedRevision":0,"plan":{"question":"为什么行程被延误？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"read","operation":{"type":"passages","cursor":null,"maxChars":160,"limit":2}}]}})
}

fn save(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    chapter: usize,
    generation: usize,
) -> Value {
    let branch = BranchId::parse("main").unwrap();
    let head = repo
        .branch_head(project, &branch)
        .unwrap()
        .unwrap()
        .snapshot_id;
    let object = ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap();
    let previous = if generation == 0 {
        None
    } else {
        Some(RevisionId::parse(format!("revision-{chapter}-0")).unwrap())
    };
    // Null and non-BMP characters must not terminate or split the source scan.
    let first = format!(
        "{}🙂\0暴雨淹没了渡口，他只好折返。{}",
        "晨光".repeat(70),
        "潮声".repeat(350)
    );
    let second = format!("第{chapter}章后场景{}", "远方".repeat(200));
    let body = format!("{first}\n\n---\n\n{second}");
    let content = json!({"chapterNumber":chapter,"title":format!("第{chapter}章"),"body":body,
        "sceneGroups":[{"id":format!("scene-{chapter}-a"),"sceneId":format!("scene-{chapter}-a"),"body":first},
            {"id":format!("scene-{chapter}-b"),"sceneId":format!("scene-{chapter}-b"),"body":second}]});
    let serialized = content.to_string();
    let digest = format!("{:x}", Sha256::digest(serialized.as_bytes()));
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: branch,
        object_id: object,
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(format!("revision-{chapter}-{generation}")).unwrap(),
        commit_id: CommitId::parse(format!("commit-{chapter}-{generation}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{chapter}-{generation}")).unwrap(),
        request_digest: digest.clone(),
        content_digest: digest,
        content_json: serialized,
        expected_project_revision: 0,
        expected_snapshot_id: head,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{chapter}-{generation}")).unwrap(),
        expected_candidate_revision_id: previous,
        chapter_projection: Some(ChapterProjection {
            chapter_id: format!("chapter-{chapter}"),
            number: chapter as i64,
            title: format!("第{chapter}章"),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    content
}

fn fixture(repo: &mut SqliteStoryRepository) -> (ProjectId, Vec<Value>) {
    let project = ProjectId::parse("passage-project").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "改述召回测试".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("snapshot-initial").unwrap(),
    )
    .unwrap();
    let contents = (1..=3)
        .map(|chapter| save(repo, &project, chapter, /*generation*/ 0))
        .collect();
    (project, contents)
}

async fn pages(app: &Router, uri: &str, mut body: Value) -> Vec<Value> {
    let mut reports = Vec::new();
    for _ in 0..100 {
        let (status, report) = call(app, "POST", uri, body.clone(), CANDIDATE).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        assert_eq!(report["coverage"]["complete"], false);
        assert_eq!(report["steps"][0]["coverage"]["pageComplete"], true);
        let cursor = report["steps"][0]["nextCursor"].clone();
        reports.push(report);
        if cursor.is_null() {
            return reports;
        }
        body["plan"]["steps"][0]["operation"]["cursor"] = cursor;
    }
    panic!("finite source scan failed to reach the end");
}

#[tokio::test]
async fn public_reading_checkpoint_requires_intact_page_and_bound_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reading-http.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture(&mut repo);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let uri = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let checkpoint = format!("{}/reading-progress", uri.strip_suffix("/query").unwrap());
    let read = format!("{checkpoint}?threadId=passage-thread&expectedRevision=0");
    let (status, empty) = call(&app, "GET", &read, Value::Null, CANDIDATE).await;
    assert_eq!(
        (status, empty),
        (
            StatusCode::OK,
            json!({"checkpoint":null,"semanticCompleteness":"unknown"})
        )
    );
    let mut page = request();
    page["plan"]["byteBudget"] = json!(4000);
    page["plan"]["steps"][0]["operation"]["maxChars"] = json!(400);
    page["plan"]["steps"][0]["operation"]["limit"] = json!(4);
    let (status, clipped) = call(&app, "POST", &uri, page.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{clipped}");
    assert_eq!(clipped["coverage"]["responseTruncated"], true);
    page["reportHash"] = clipped["contentHash"].clone();
    page["expectedVersion"] = json!(0);
    page["summary"] = json!("不能把裁剪页标成已读");
    let (status, _) = call(&app, "POST", &checkpoint, page, CANDIDATE).await;
    assert_ne!(status, StatusCode::OK);
    let mut save = request();
    let (status, report) = call(&app, "POST", &uri, save.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["coverage"]["responseTruncated"], false);
    save["reportHash"] = report["contentHash"].clone();
    save["expectedVersion"] = json!(0);
    save["summary"] = json!("首段候选解释");
    for (name, method, uri, payload) in [
        ("read_reading_progress", "GET", &read, Value::Null),
        ("checkpoint_reading", "POST", &checkpoint, save.clone()),
    ] {
        let response = app
            .clone()
            .author_fixture_request(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {CANDIDATE}"))
                    .header("x-novel-task-capability", CAPABILITY)
                    .header("content-type", "application/json")
                    .header("x-novel-tool-name", name)
                    .header("x-novel-root-thread", "passage-thread")
                    .header("x-novel-caller-thread", "child")
                    .header("x-novel-parent-thread", "passage-thread")
                    .header("x-novel-turn", "reading-turn")
                    .header("x-novel-tool-call", "reading-call")
                    .header("x-novel-request-hash", "a".repeat(64))
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let error: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error["error"]["code"], "invalid_request");
    }
    let mut forged = save.clone();
    forged["threadId"] = json!("another-thread");
    let (status, _) = call(&app, "POST", &checkpoint, forged, CANDIDATE).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, saved) = call(&app, "POST", &checkpoint, save.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["checkpoint"]["version"], 1);
    assert_eq!(
        call(&app, "POST", &checkpoint, save, CANDIDATE).await,
        (status, saved.clone())
    );
    assert_eq!(
        call(&app, "GET", &read, Value::Null, CANDIDATE).await,
        (status, saved)
    );
    let tasks_uri = format!("/api/projects/{project}/work-tasks");
    let (status, work_page) = call(&app, "GET", &tasks_uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{work_page}");
    let work = &work_page["tasks"][0];
    let expected = json!({"status":"current","version":1,"pages":1,"acknowledgedCharacters":
        report["evidence"].as_object().unwrap().values().map(|e|e["end"].as_u64().unwrap()-e["start"].as_u64().unwrap()
            - if e["start"].as_u64().unwrap()>0 {32} else {0}).sum::<u64>(),"traversal":"reading"});
    assert_eq!(work["steps"][0]["readingProgress"], expected);
    let work_uri = format!("{tasks_uri}/{}", work["id"].as_str().unwrap());
    for endpoint in [&tasks_uri, &work_uri] {
        assert_eq!(
            call(&app, "GET", endpoint, Value::Null, CANDIDATE).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let control = format!("{work_uri}/control");
    let (status, paused) = call(
        &app,
        "POST",
        &control,
        json!({"command":"pause","expectedVersion":work["version"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    assert_eq!(paused["status"], "paused");
    assert_eq!(paused["steps"][0]["readingProgress"], expected);
    assert_ne!(
        call(&app, "GET", &read, Value::Null, CANDIDATE).await.0,
        StatusCode::OK
    );
    let (status, cancelled) = call(
        &app,
        "POST",
        &control,
        json!({"command":"cancel","expectedVersion":paused["version"]}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(cancelled["steps"][0]["readingProgress"], expected);
    let writer = task(
        &app,
        &project,
        "generateChapter",
        json!({"type":"chapter","chapterId":"chapter-3","chapterNumber":3,
        "readingChapter":3,"povCharacterId":"linxi"}),
    )
    .await;
    let mut save = request();
    let (status, hidden) = call(&app, "POST", &writer, save.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{hidden}");
    assert_eq!(hidden["steps"][0]["results"], json!([]));
    save["reportHash"] = hidden["contentHash"].clone();
    save["expectedVersion"] = json!(0);
    save["summary"] = json!("没有权限不表示全书读完");
    let checkpoint = format!(
        "{}/reading-progress",
        writer.strip_suffix("/query").unwrap()
    );
    assert_ne!(
        call(&app, "POST", &checkpoint, save, CANDIDATE).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn source_scan_recalls_paraphrases_and_covers_unicode_without_keyword_matching() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("逐页原文.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, contents) = fixture(&mut repo);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let uri = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let body = request();
    let mut literal = body.clone();
    literal["plan"]["steps"][0]["operation"] =
        json!({"type":"text","query":"为什么行程被延误？","limit":2});
    let (status, report) = call(&app, "POST", &uri, literal, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["steps"][0]["results"], json!([]));
    let scanned = pages(&app, &uri, body.clone()).await;
    let mut covered = contents
        .iter()
        .map(|value| vec![false; value["body"].as_str().unwrap().chars().count()])
        .collect::<Vec<_>>();
    let mut saw_cause = false;
    for report in &scanned {
        assert!(report.to_string().len() <= 9000);
        assert_eq!(report["steps"][0]["coverage"]["fullScanCertified"], false);
        for row in report["steps"][0]["results"].as_array().unwrap() {
            let chapter = row["chapterNumber"].as_u64().unwrap() as usize - 1;
            let evidence = &report["evidence"][row["evidenceRef"].as_str().unwrap()];
            assert_eq!(evidence["sourcePath"], "/body");
            assert_eq!(
                evidence["sourceRevisionId"],
                format!("revision-{}-0", chapter + 1)
            );
            let start = evidence["start"].as_u64().unwrap() as usize;
            let end = evidence["end"].as_u64().unwrap() as usize;
            let expected = contents[chapter]["body"]
                .as_str()
                .unwrap()
                .chars()
                .skip(start)
                .take(end - start)
                .collect::<String>();
            assert_eq!(evidence["quote"], expected);
            saw_cause |= expected.contains("暴雨淹没了渡口，他只好折返。");
            covered[chapter][start..end].fill(true);
        }
    }
    assert!(saw_cause);
    assert!(covered.iter().flatten().all(|value| *value));
    assert_eq!(
        scanned[0]["steps"][0]["coverage"]["startsAtBeginning"],
        true
    );
    assert_eq!(
        scanned.last().unwrap()["steps"][0]["coverage"]["endOfScopeFromCursor"],
        true
    );
    assert_eq!(
        scanned.last().unwrap()["steps"][0]["coverage"]["startsAtBeginning"],
        false
    );
    assert_eq!(pages(&app, &uri, body.clone()).await, scanned);
    let baseline = &scanned[0];
    let mut semantic = body;
    semantic["plan"]["ranking"] = json!({"type":"modelSemantic","baseReportHash":baseline["contentHash"],
        "judgments":baseline["steps"][0]["results"].as_array().unwrap().iter().enumerate().map(|(index,row)|json!({"resultId":row["resultId"],"score":index*500,
            "rationale":"工程指定的模型候选评分，不是供应商质量认证"})).collect::<Vec<_>>()});
    let (status, ranked) = call(&app, "POST", &uri, semantic.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{ranked}");
    assert_eq!(
        ranked["steps"][0]["results"][0]["resultId"],
        baseline["steps"][0]["results"][1]["resultId"]
    );
    assert_eq!(ranked["evidence"], baseline["evidence"]);
    assert_eq!(ranked["coverage"], baseline["coverage"]);
    assert_eq!(
        ranked["ranking"]["candidateScope"],
        "authorizedBaselineResults"
    );
    let id = uri
        .strip_suffix("/query")
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap();
    let encoded = semantic["plan"]
        .to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect::<String>();
    let (status,pack) = call(&app,"GET",&format!("/api/projects/{project}/context?taskId={id}&threadId=passage-thread&projectRevision=0&purpose=planner&byteBudget=60000&queryPlan={encoded}"),Value::Null,CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{pack}");
    assert_eq!(pack["queryReport"], ranked);
    assert_eq!(pack["coverage"]["complete"], false);
}

#[tokio::test]
async fn passage_scope_uses_task_position_and_never_exposes_unverified_pov_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("原文范围.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, contents) = fixture(&mut repo);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let writer = task(&app,&project,"generateChapter",json!({"chapterId":"chapter-2","chapterNumber":2,"sceneOrder":1,"scope":{"type":"throughScene","endSceneOrder":1},"storyTime":2})).await;
    let scanned = pages(&app, &writer, request()).await;
    for report in &scanned {
        for row in report["steps"][0]["results"].as_array().unwrap() {
            assert!(row["chapterNumber"] == 1 || row["chapterNumber"] == 2);
            let evidence = &report["evidence"][row["evidenceRef"].as_str().unwrap()];
            let chapter = row["chapterNumber"].as_u64().unwrap() as usize - 1;
            let source = if chapter == 0 {
                assert_eq!(evidence["sourcePath"], "/body");
                &contents[chapter]["body"]
            } else {
                assert_eq!(evidence["sourcePath"], "/sceneGroups/0/body");
                &contents[chapter]["sceneGroups"][0]["body"]
            };
            let start = evidence["start"].as_u64().unwrap() as usize;
            let end = evidence["end"].as_u64().unwrap() as usize;
            assert_eq!(
                evidence["quote"],
                source
                    .as_str()
                    .unwrap()
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>()
            );
        }
    }
    let rolling = task(&app, &project, "generatePlan", json!({"planFromChapter":2})).await;
    for report in pages(&app, &rolling, request()).await {
        assert!(
            report["steps"][0]["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["chapterNumber"] == 1)
        );
    }
    let first = task(&app, &project, "generatePlan", json!({"planFromChapter":1})).await;
    let (status, empty) = call(&app, "POST", &first, request(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["steps"][0]["results"], json!([]));
    let source = task(
        &app,
        &project,
        "extractCommitments",
        json!({"chapterId":"chapter-2","chapterNumber":2,"sourceRevisionId":"revision-2-0"}),
    )
    .await;
    for report in pages(&app, &source, request()).await {
        assert!(
            report["steps"][0]["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["chapterNumber"] == 2)
        );
    }
    let pov = task(&app,&project,"generateChapter",json!({"chapterId":"chapter-2","chapterNumber":2,"sceneOrder":1,"scope":{"type":"throughScene","endSceneOrder":1},"storyTime":2,"povCharacterId":"person-0"})).await;
    let (status, hidden) = call(&app, "POST", &pov, request(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{hidden}");
    assert_eq!(hidden["steps"][0]["results"], json!([]));
    assert_eq!(
        hidden["steps"][0]["coverage"],
        json!({"complete":false,"semanticCompleteness":"unknown","povTextUnverified":true})
    );
    let mut forged = request();
    forged["plan"]["steps"][0]["operation"]["cursor"] =
        scanned[0]["steps"][0]["nextCursor"].clone();
    assert_eq!(
        call(&app, "POST", &pov, forged, CANDIDATE).await.1["steps"][0]["coverage"],
        hidden["steps"][0]["coverage"]
    );
}

#[tokio::test]
async fn passage_cursor_is_source_and_task_bound_and_budget_clipping_cannot_skip_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("原文游标.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture(&mut repo);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let uri = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let (_, baseline) = call(&app, "POST", &uri, request(), CANDIDATE).await;
    let cursor = baseline["steps"][0]["nextCursor"].clone();
    assert!(cursor.is_string());
    let mut next = request();
    next["plan"]["steps"][0]["operation"]["cursor"] = cursor.clone();
    let (status, continued) = call(&app, "POST", &uri, next.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{continued}");
    enum Failure {
        Question,
        Step,
        Binding,
        Position,
        MissingObject,
    }
    for failure in [
        Failure::Question,
        Failure::Step,
        Failure::Binding,
        Failure::Position,
        Failure::MissingObject,
    ] {
        let mut invalid = next.clone();
        let mut decoded: Value = serde_json::from_str(cursor.as_str().unwrap()).unwrap();
        match failure {
            Failure::Question => invalid["plan"]["question"] = json!("不同的问题"),
            Failure::Step => invalid["plan"]["steps"][0]["id"] = json!("another-step"),
            Failure::Binding => decoded["binding"] = json!("b".repeat(64)),
            Failure::Position => decoded["position"]["start"] = json!(8_000_000),
            Failure::MissingObject => decoded["position"]["object"] = json!("draft-unknown"),
        }
        invalid["plan"]["steps"][0]["operation"]["cursor"] = json!(decoded.to_string());
        assert_ne!(
            call(&app, "POST", &uri, invalid, CANDIDATE).await.0,
            StatusCode::OK
        );
    }
    let other = task(&app, &project, "advisory", json!({"type":"project"})).await;
    assert_ne!(
        call(&app, "POST", &other, next.clone(), CANDIDATE).await.0,
        StatusCode::OK
    );
    let mut large = next.clone();
    large["plan"]["byteBudget"] = json!(4000);
    large["plan"]["steps"][0]["operation"]["maxChars"] = json!(400);
    large["plan"]["steps"][0]["operation"]["limit"] = json!(4);
    let (status, clipped) = call(&app, "POST", &uri, large, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{clipped}");
    assert!(clipped.to_string().len() <= 4000);
    assert_eq!(clipped["coverage"]["responseTruncated"], true);
    assert_eq!(clipped["steps"][0]["coverage"]["pageComplete"], false);
    assert_eq!(
        clipped["steps"][0]["coverage"]["endOfScopeFromCursor"],
        false
    );
    assert_eq!(clipped["steps"][0]["nextCursor"], Value::Null);
    let mut smaller = next.clone();
    smaller["plan"]["byteBudget"] = json!(4000);
    smaller["plan"]["steps"][0]["operation"]["maxChars"] = json!(80);
    smaller["plan"]["steps"][0]["operation"]["limit"] = json!(1);
    let (status, retry) = call(&app, "POST", &uri, smaller, CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    let evidence = &retry["evidence"][retry["steps"][0]["results"][0]["evidenceRef"]
        .as_str()
        .unwrap()];
    let decoded: Value = serde_json::from_str(cursor.as_str().unwrap()).unwrap();
    assert_eq!(evidence["start"], decoded["position"]["start"]);
    save(
        &mut repo, &project, /*chapter*/ 1, /*generation*/ 1,
    );
    assert_ne!(
        call(&app, "POST", &uri, next, CANDIDATE).await.0,
        StatusCode::OK
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
