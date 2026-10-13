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

const AUTHOR: &str = "delegation-author-token-12345678901234";
const CANDIDATE: &str = "delegation-candidate-token-1234567890";
const CAPABILITY: &str = "delegation-capability-123456789012345";
const PROJECT: &str = "delegation-http";

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

enum Actor {
    Author,
    Candidate,
}
struct Fixture {
    directory: tempfile::TempDir,
    app: Router,
}

impl Fixture {
    fn new(chapters: &[(&str, &str)]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("delegation.sqlite3");
        let mut repo = SqliteStoryRepository::open(&path).unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: ProjectId::parse(PROJECT).unwrap(),
                title: "午夜邮局".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        for (index, (chapter, body)) in chapters.iter().enumerate() {
            save(
                &mut repo,
                chapter,
                i64::try_from(index + 1).unwrap(),
                body,
                &format!("source-{chapter}"),
            );
        }
        Self {
            app: router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into())),
            directory,
        }
    }

    fn repo(&self) -> SqliteStoryRepository {
        SqliteStoryRepository::open(self.directory.path().join("delegation.sqlite3")).unwrap()
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        actor: Actor,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let response = self
            .app
            .clone()
            .author_fixture_request(
                Request::builder()
                    .method(method)
                    .uri(format!("/api/projects/{PROJECT}{path}"))
                    .header(
                        "authorization",
                        format!(
                            "Bearer {}",
                            match actor {
                                Actor::Author => AUTHOR,
                                Actor::Candidate => CANDIDATE,
                            }
                        ),
                    )
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

    async fn task(&self, operation: &str, target: Value) -> Value {
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let revision = self
            .repo()
            .project_revision(&ProjectId::parse(PROJECT).unwrap())
            .unwrap();
        let (status, task) = self.call("POST", "/tasks", Actor::Author, Some(json!({
            "operation":operation,"inputRevision":revision,"threadId":"delegation-root","target":target,
            "authorIntent":"请提取本章伏笔并分析事实，保存候选，不发布正文或确认设定。",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires
        }))).await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        task
    }

    async fn context(&self, task: &Value, entries: usize) -> (StatusCode, Value) {
        self.call("GET", &format!("/context?taskId={}&threadId=delegation-root&projectRevision={}&purpose={}&budget={entries}&byteBudget=60000{}",
            task["id"].as_str().unwrap(), task["inputRevision"], task["purpose"].as_str().unwrap(),
            if task["purpose"] == "writer" { "&readingChapter=1&sceneOrder=1" } else { "" }),
            Actor::Candidate, /*body*/ None).await
    }

    async fn selected_context(&self, task: &Value, number: i64) -> (StatusCode, Value) {
        self.call("GET", &format!("/context?taskId={}&threadId=delegation-root&projectRevision={}&purpose=planner&budget=1&byteBudget=60000&readingChapter={number}",
            task["id"].as_str().unwrap(), task["inputRevision"]), Actor::Candidate, /*body*/ None).await
    }

    async fn read_relevant_text(&self, task: &Value, context: &Value) -> Value {
        assert_eq!(context["relevantText"]["textDelivered"], false);
        assert_eq!(context["coverage"]["complete"], false);
        let source = context["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["sectionPath"] == "/relevantText")
            .unwrap();
        assert_eq!(source["sourceId"], context["relevantText"]["sourceId"]);
        let mut cursor = Value::Null;
        let mut text = String::new();
        let mut pages = 0;
        loop {
            let suffix = cursor
                .as_str()
                .map(|cursor| format!("&cursor={cursor}"))
                .unwrap_or_default();
            let (status, page) = self
                .call(
                    "GET",
                    &format!(
                        "/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{suffix}",
                        task["id"].as_str().unwrap(),
                        task["threadId"].as_str().unwrap(),
                        task["inputRevision"],
                        source["sourceId"].as_str().unwrap()
                    ),
                    Actor::Candidate,
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{page}");
            assert_eq!(page["sourceId"], source["sourceId"]);
            assert_eq!(page["sectionPath"], "/relevantText");
            assert_eq!(page["contentHash"], source["contentHash"]);
            assert_eq!(page["byteRange"]["start"], text.len());
            let part = page["payloadPart"].as_str().unwrap();
            assert!(part.len() <= 2_000);
            assert!(page.to_string().len() <= 6_500);
            text.push_str(part);
            pages += 1;
            assert_eq!(page["byteRange"]["end"], text.len());
            assert_eq!(page["byteRange"]["total"], source["payloadBytes"]);
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                assert_eq!(page["endOfInputFromCursor"], true);
                assert_eq!(page["byteRange"]["total"], text.len());
                break;
            }
            assert_eq!(page["endOfInputFromCursor"], false);
        }
        if text.len() > 2_000 {
            assert!(pages > 1);
        }
        assert_eq!(
            format!("{:x}", Sha256::digest(text.as_bytes())),
            source["contentHash"].as_str().unwrap()
        );
        serde_json::from_str(&text).unwrap()
    }
}

fn save(repo: &mut SqliteStoryRepository, chapter: &str, number: i64, body: &str, source: &str) {
    let project = ProjectId::parse(PROJECT).unwrap();
    let branch = BranchId::parse("main").unwrap();
    let object = ObjectId::parse(format!("draft-{chapter}")).unwrap();
    let previous = repo
        .object_heads(&project)
        .unwrap()
        .into_iter()
        .find(|head| head.object_id == object)
        .and_then(|head| head.candidate_revision_id);
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: branch.clone(),
        object_id: object,
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(source).unwrap(),
        commit_id: CommitId::parse(format!("save-{source}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{source}")).unwrap(),
        request_digest: source.into(),
        content_digest: source.into(),
        content_json:
            json!({"chapterId":chapter,"chapterNumber":number,"title":"来信","body":body,"sceneGroups":[]})
                .to_string(),
        expected_project_revision: repo.project_revision(&project).unwrap(),
        expected_snapshot_id: repo
            .branch_head(&project, &branch)
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{source}")).unwrap(),
        expected_candidate_revision_id: previous,
        chapter_projection: Some(ChapterProjection {
            chapter_id: chapter.into(),
            number,
            title: "来信".into(),
        }),
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
}

#[tokio::test]
async fn focused_target_is_independent_of_lazy_prose_and_registers_both_operations_without_story_writes()
 {
    let fixture = Fixture::new(&[
        ("first", "前章正文。"),
        ("second", &"本章很长。".repeat(2000)),
    ]);
    let before = fixture
        .call(
            "GET",
            "/creative-workspace",
            Actor::Author,
            /*body*/ None,
        )
        .await
        .1;
    let task = fixture.task("advisory", json!({"type":"project","conversationFocus":{"view":"chapters","chapterId":"second","chapterNumber":2}})).await;
    let (status, context) = fixture.context(&task, /*entries*/ 1).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert!(!context.to_string().contains("本章很长。"));
    // Entry/transport budgets no longer discard admitted frozen source text.
    let prose = fixture.read_relevant_text(&task, &context).await;
    let prose = prose.as_array().unwrap();
    assert_eq!(prose.len(), 2);
    for (chapter, number, body) in [
        ("first", 1, "前章正文。".to_owned()),
        ("second", 2, "本章很长。".repeat(2_000)),
    ] {
        let saved = prose
            .iter()
            .find(|item| item["objectId"] == format!("draft-{chapter}"))
            .unwrap();
        assert_eq!(saved["sourceRevisionId"], format!("source-{chapter}"));
        assert_eq!(
            saved["content"],
            json!({"chapterId":chapter,"chapterNumber":number,"title":"来信","body":body,"sceneGroups":[]})
        );
    }
    assert_eq!(
        context["operationContext"]["delegationTargets"],
        json!({"authority":"sourceIdentityOnly","scope":"focused","status":"available",
        "entries":[{"objectId":"draft-second","target":{"chapterId":"second","chapterNumber":2,"sourceRevisionId":"source-second"},
            "operations":["extractCommitments","modelKnowledge"]}],"coverage":{"total":1,"included":1,"truncated":false}})
    );
    assert!(
        context["operationContext"]["delegationTargets"]
            .to_string()
            .len()
            <= 768
    );
    for operation in ["extractCommitments", "modelKnowledge"] {
        let (status, action) = fixture.call("POST", &format!("/tasks/{}/actions", task["id"].as_str().unwrap()), Actor::Candidate,
            Some(json!({"taskId":task["id"],"threadId":"delegation-root","expectedRevision":task["inputRevision"],"expectedIntentRevision":0,
                "operation":operation,"target":context["operationContext"]["delegationTargets"]["entries"][0]["target"],
                "instruction":"按原作者要求保存本章分析候选","idempotencyKey":operation}))).await;
        assert_eq!(status, StatusCode::CREATED, "{action}");
        let mut expected =
            context["operationContext"]["delegationTargets"]["entries"][0]["target"].clone();
        expected["type"] = json!("chapter");
        assert_eq!(action["target"], expected);
    }
    let after = fixture
        .call(
            "GET",
            "/creative-workspace",
            Actor::Author,
            /*body*/ None,
        )
        .await
        .1;
    for field in ["revision", "headSnapshotId", "sourceDigest", "chapters"] {
        assert_eq!(after[field], before[field]);
    }
}

#[tokio::test]
async fn project_index_is_byte_bounded_and_never_implies_complete_prose_or_writer_authority() {
    let fixture = Fixture::new(&[
        ("one", "一"),
        ("two", "二"),
        ("three", "三"),
        ("four", "四"),
        ("five", "五"),
        ("six", "六"),
    ]);
    let task = fixture.task("advisory", json!({"type":"project"})).await;
    let (status, context) = fixture.context(&task, /*entries*/ 1).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let targets = &context["operationContext"]["delegationTargets"];
    let included = targets["entries"].as_array().unwrap().len();
    assert!((1..=4).contains(&included));
    assert!(targets.to_string().len() <= 768);
    assert_eq!(
        targets["coverage"],
        json!({"total":6,"included":included,"truncated":true})
    );
    let prose = fixture.read_relevant_text(&task, &context).await;
    let prose = prose.as_array().unwrap();
    assert_eq!(prose.len(), 6);
    for (index, (chapter, body)) in [
        ("one", "一"),
        ("two", "二"),
        ("three", "三"),
        ("four", "四"),
        ("five", "五"),
        ("six", "六"),
    ]
    .iter()
    .enumerate()
    {
        let saved = prose
            .iter()
            .find(|item| item["objectId"] == format!("draft-{chapter}"))
            .unwrap();
        assert_eq!(saved["sourceRevisionId"], format!("source-{chapter}"));
        assert_eq!(
            saved["content"],
            json!({"chapterId":chapter,"chapterNumber":index + 1,"title":"来信","body":body,"sceneGroups":[]})
        );
    }
    let writer = fixture
        .task(
            "generateChapter",
            json!({"chapterId":"one","chapterNumber":1,"readingChapter":1,"sceneOrder":1}),
        )
        .await;
    let (status, writer_context) = fixture.context(&writer, /*entries*/ 24).await;
    assert_eq!(status, StatusCode::OK, "{writer_context}");
    assert!(
        writer_context["operationContext"]
            .get("delegationTargets")
            .is_none()
    );
    let writer_prose = fixture.read_relevant_text(&writer, &writer_context).await;
    assert_eq!(writer_prose.as_array().unwrap().len(), 1);
    assert_eq!(writer_prose[0]["objectId"], "draft-one");
    assert_eq!(writer_prose[0]["sourceRevisionId"], "source-one");
    assert_eq!(
        writer_prose[0]["content"],
        json!({"chapterId":"one","chapterNumber":1,"title":"来信","body":"一","sceneGroups":[]})
    );
    let (status, rejected) = fixture
        .call(
            "GET",
            &format!(
                "/tasks/{}/task-source?threadId=delegation-root&expectedRevision={}&sourceId={}",
                writer["id"].as_str().unwrap(),
                writer["inputRevision"],
                context["relevantText"]["sourceId"].as_str().unwrap()
            ),
            Actor::Candidate,
            None,
        )
        .await;
    assert!(status.is_client_error(), "{rejected}");
    assert_eq!(rejected["error"]["code"], "task_source_not_in_catalog");
    assert!(rejected.get("payloadPart").is_none());
    let (status, author_context) = fixture
        .call(
            "GET",
            "/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
            Actor::Author,
            /*body*/ None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{author_context}");
    assert!(
        author_context["operationContext"]
            .get("delegationTargets")
            .is_none()
    );
    // Unbound Author transport still respects its real one-entry selection.
    let (status, author_one) = fixture
        .call(
            "GET",
            &format!(
                "/context?projectRevision={}&purpose=planner&budget=1&byteBudget=60000",
                task["inputRevision"]
            ),
            Actor::Author,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{author_one}");
    assert_eq!(author_one["relevantText"].as_array().unwrap().len(), 1);
    assert_eq!(author_one["relevantText"][0]["objectId"], "draft-one");
    assert_eq!(author_one["coverage"]["complete"], false);
    assert!(
        author_one["operationContext"]
            .get("delegationTargets")
            .is_none()
    );
}

#[tokio::test]
async fn historical_reference_is_not_rebased_and_current_source_mutation_rejects_old_registration()
{
    let fixture = Fixture::new(&[("first", "原来正文。")]);
    save(
        &mut fixture.repo(),
        "first",
        /*number*/ 1,
        "修改正文。",
        "new-source",
    );
    let snapshot = fixture
        .repo()
        .branch_head(
            &ProjectId::parse(PROJECT).unwrap(),
            &BranchId::parse("main").unwrap(),
        )
        .unwrap()
        .unwrap()
        .snapshot_id;
    let historical = fixture.task("advisory", json!({"type":"project","conversationFocus":{"view":"chapters","chapterId":"first","chapterNumber":1,
        "reference":{"kind":"scene","projectId":PROJECT,"snapshotId":snapshot.as_str(),"objectId":"draft-first","revisionId":"source-first","sceneGroupId":"original"}}})).await;
    let (status, context) = fixture.context(&historical, /*entries*/ 24).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["operationContext"]["delegationTargets"],
        json!({"authority":"sourceIdentityOnly","scope":"focused","status":"referencedSourceNotCurrent",
        "entries":[],"coverage":{"total":0,"included":0,"truncated":false}})
    );
    let task = fixture.task("advisory", json!({"type":"project","conversationFocus":{"view":"chapters","chapterId":"first","chapterNumber":1}})).await;
    let (status, context) = fixture.context(&task, /*entries*/ 24).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["operationContext"]["delegationTargets"]["entries"][0]["target"]["sourceRevisionId"],
        "new-source"
    );
    let target = context["operationContext"]["delegationTargets"]["entries"][0]["target"].clone();
    save(
        &mut fixture.repo(),
        "first",
        /*number*/ 1,
        "再次修改。",
        "newer-source",
    );
    let (status, _) = fixture.context(&task, /*entries*/ 24).await;
    assert!(status.is_client_error());
    let (status, _) = fixture.call("POST", &format!("/tasks/{}/actions", task["id"].as_str().unwrap()), Actor::Candidate,
        Some(json!({"taskId":task["id"],"threadId":"delegation-root","expectedRevision":task["inputRevision"],"expectedIntentRevision":0,"operation":"extractCommitments",
            "target":target,"instruction":"保存伏笔候选","idempotencyKey":"stale"}))).await;
    assert!(status.is_client_error());
}

#[tokio::test]
async fn escaped_valid_identities_enforce_the_whole_metadata_cap() {
    let chapter = "\"".repeat(80);
    let fixture = Fixture::new(&[(&chapter, "原文。")]);
    save(
        &mut fixture.repo(),
        &chapter,
        /*number*/ 1,
        "当前原文。",
        &"\"".repeat(140),
    );
    let task = fixture.task("advisory", json!({"type":"project","conversationFocus":{"view":"chapters","chapterId":chapter,"chapterNumber":1}})).await;
    let (status, error) = fixture.context(&task, /*entries*/ 1).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert!(error.to_string().contains("768-byte metadata budget"));
    let project_task = fixture.task("advisory", json!({"type":"project"})).await;
    let (status, _) = fixture.selected_context(&project_task, /*number*/ 1).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, context) = fixture.context(&project_task, /*entries*/ 1).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let metadata = &context["operationContext"]["delegationTargets"];
    assert_eq!(
        metadata["coverage"],
        json!({"total":1,"included":0,"truncated":true})
    );
    assert_eq!(metadata["entries"], json!([]));
    assert!(metadata.to_string().len() <= 768);
}

#[tokio::test]
async fn explicit_late_chapter_selector_is_not_limited_by_the_first_project_page() {
    let names = (1..=30)
        .map(|number| format!("chapter-{number}"))
        .collect::<Vec<_>>();
    let chapters = names
        .iter()
        .map(|name| (name.as_str(), "已保存正文。"))
        .collect::<Vec<_>>();
    let fixture = Fixture::new(&chapters);
    let task = fixture.task("advisory", json!({"type":"project"})).await;
    let (status, context) = fixture.selected_context(&task, /*number*/ 30).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let target =
        json!({"chapterId":"chapter-30","chapterNumber":30,"sourceRevisionId":"source-chapter-30"});
    assert_eq!(
        context["operationContext"]["delegationTargets"],
        json!({"authority":"sourceIdentityOnly","scope":"requestedChapter","status":"available",
        "entries":[{"objectId":"draft-chapter-30","target":target,"operations":["extractCommitments","modelKnowledge"]}],
        "coverage":{"total":1,"included":1,"truncated":false}})
    );
    let (status, action) = fixture.call("POST", &format!("/tasks/{}/actions", task["id"].as_str().unwrap()), Actor::Candidate,
        Some(json!({"taskId":task["id"],"threadId":"delegation-root","expectedRevision":task["inputRevision"],"expectedIntentRevision":0,
            "operation":"extractCommitments","target":target,"instruction":"按明确指令提取第30章伏笔候选","idempotencyKey":"chapter30"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let (status, missing) = fixture.selected_context(&task, /*number*/ 31).await;
    assert_eq!(status, StatusCode::OK, "{missing}");
    assert_eq!(
        missing["operationContext"]["delegationTargets"],
        json!({"authority":"sourceIdentityOnly","scope":"requestedChapter","status":"noSavedSource",
        "entries":[],"coverage":{"total":0,"included":0,"truncated":false}})
    );
    let (status, _) = fixture.selected_context(&task, /*number*/ 0).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let focused = fixture.task("advisory", json!({"type":"project","conversationFocus":{"view":"chapters","chapterId":"chapter-2","chapterNumber":2}})).await;
    let (status, _) = fixture.selected_context(&focused, /*number*/ 30).await;
    assert!(status.is_client_error());
    let (status, same) = fixture.selected_context(&focused, /*number*/ 2).await;
    assert_eq!(status, StatusCode::OK, "{same}");
    assert_eq!(
        same["operationContext"]["delegationTargets"]["scope"],
        "focused"
    );
    assert_eq!(
        same["operationContext"]["delegationTargets"]["entries"][0]["target"]["chapterId"],
        "chapter-2"
    );
}

use novel_codex_story_core::{ProjectRevisionRepository};
