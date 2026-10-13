use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::{settle_fixture, task_fixture};
use task_fixture::AuthorTaskFixture;

pub(super) const AUTHOR: &str = "outcome-author-token-12345678901234567";
pub(super) const CANDIDATE: &str = "outcome-candidate-token-12345678901234";
const CAPABILITY: &str = "outcome-capability-123456789012345678";
pub(super) const INTENT: &str = "把烧信改为藏信；说明她尚未坦白。保留开场、守卫和结尾。";
pub(super) const NEW_BODY: &str = "🌙她将密函藏进衣襟。她没有向守卫坦白。";

pub(super) fn expected_issue_summaries(
    current_draft: &Value,
    status: &str,
    observation_count: usize,
) -> Value {
    json!([
        {"issueId":"confession-missing","summary":"林夕仍未向守卫坦白密函的存在",
         "status":status,"chapterId":"chapter-one","currentDraftRevisionId":current_draft,
         "observations":[],"observationCount":observation_count,"hasMoreObservations":true},
        {"issueId":"guard-motive-missing","summary":"守卫沉默的动机缺少新稿证据",
         "status":status,"chapterId":"chapter-one","currentDraftRevisionId":current_draft,
         "observations":[],"observationCount":observation_count,"hasMoreObservations":true}
    ])
}

pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub app: Router,
    state: RuntimeState,
    pub root: String,
    pub original: Value,
    pub target: Value,
    pub discussion: Value,
}

pub(super) fn desired_outcomes() -> Value {
    json!([
        {"id":"hide-letter","criterion":"她藏起密函，不再烧信。","groupIds":["letter"]},
        {"id":"confess","criterion":"她向守卫坦白密函的存在。","groupIds":["letter"]},
        {"id":"guard-motive","criterion":"交代守卫保持沉默的动机。","groupIds":["witness"]}
    ])
}

impl Fixture {
    pub async fn new() -> Self {
        Self::with_letter("她烧掉密函。").await
    }
    pub async fn with_letter(letter: &str) -> Self {
        Self::with_book(letter, 1).await
    }
    pub async fn with_long_book(chapters: i64) -> Self {
        Self::with_book("她烧掉密函。", chapters).await
    }
    async fn with_book(letter: &str, number: i64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let state = task_fixture::state(
            directory.path().join("outcomes.sqlite3"),
            AUTHOR.into(),
            CANDIDATE.into(),
        );
        let app = router(state.clone());
        let (status, created) = call(
            &app,
            "POST",
            "/api/projects",
            AUTHOR,
            json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"改稿证据"}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let root = format!(
            "/api/projects/{}",
            created["project"]["id"].as_str().unwrap()
        );
        if number > 1 {
            // Immutable domain seeds before any author task; real HTTP reads below.
            let mut db =
                rusqlite::Connection::open(directory.path().join("outcomes.sqlite3")).unwrap();
            let tx = db.transaction().unwrap();
            let project = created["project"]["id"].as_str().unwrap();
            for n in 1..number {
                let chapter = format!("prior-{n}");
                let object = format!("draft-{chapter}");
                let revision = format!("prior-revision-{n}");
                let body = if n == 7 {
                    format!("远章独有的实际证据。{}", "前文章节内容。".repeat(1000))
                } else {
                    "前文章节内容。".repeat(1000)
                };
                tx.execute(
                    "INSERT INTO story_chapters_v2 VALUES(?1,?2,?3,?4,'candidate','')",
                    rusqlite::params![chapter, project, n, format!("第{n}章")],
                )
                .unwrap();
                tx.execute("INSERT INTO story_revisions_v2 VALUES(?1,?2,?3,'draft',NULL,'seed-digest',?4)",rusqlite::params![revision,project,object,json!({"chapterId":chapter,"chapterNumber":n,"title":format!("第{n}章"),"body":body,"completeness":"chapterComplete","sceneGroups":[{"id":"prior-scene","body":body}]}).to_string()]).unwrap();
                tx.execute(
                    "INSERT INTO story_object_heads_v2 VALUES(?1,?2,'draft',?3,NULL,0)",
                    rusqlite::params![project, object, revision],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let mut payload = json!({"chapterId":"chapter-one","chapterNumber":number,"title":"密函",
            "sceneGroups":[
                {"id":"opening","sceneId":null,"body":"林夕走进港口。","locked":false},
                {"id":"letter","sceneId":null,"body":letter,"locked":false},
                {"id":"witness","sceneId":null,"body":"守卫沉默。","locked":false},
                {"id":"ending","sceneId":null,"body":"白塔熄灭。","locked":true}]});
        join_body(&mut payload);
        let (status, checkpoint) = call(
            &app,
            "POST",
            &format!("{root}/drafts/checkpoints"),
            AUTHOR,
            json!({"payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":null,
                "idempotencyKey":"original"}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{checkpoint}");
        let (status, workspace) = call(
            &app,
            "GET",
            &format!("{root}/creative-workspace"),
            AUTHOR,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{workspace}");
        let mut original = workspace["chapters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|chapter| chapter["id"] == "chapter-one")
            .unwrap()["candidate"]
            .clone();
        assert_eq!(original["id"], checkpoint["revisionId"]);
        original["revisionId"] = checkpoint["revisionId"].clone();
        original["title"] = payload["title"].clone();
        original["chapterNumber"] = payload["chapterNumber"].clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (status, discussion) = call(&app, "POST", &format!("{root}/tasks"), AUTHOR,
            json!({"operation":"advisory","target":{"type":"project"},"inputRevision":0,
                "threadId":"discussion","authorIntent":INTENT,
                "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
                "capabilityExpiresAt":now+900,
                "initialBudget":{"authorizationId":"outcome-budget","authorMessageId":"outcome-root",
                    "expectedBaseSnapshotId":workspace["headSnapshotId"],
                    "expectedSourceDigest":workspace["sourceDigest"],
                    "maxRequests":16,"maxTokens":200_000,"deadlineAt":now+900}})).await;
        assert_eq!(status, StatusCode::CREATED, "{discussion}");
        let target = json!({"type":"chapter","chapterId":"chapter-one","chapterNumber":number,
            "readingChapter":number,"sceneOrder":1,
            "chapterAcceptance":{"requirements":[{"id":"whole-chapter","text":"保留开场、守卫和结尾"}],"length":{"requirement":"作者未要求调整章节篇幅","minCharacters":null,"maxCharacters":null}},
            "scope":{"type":"selectedScenes","sourceRevisionId":original["revisionId"],
                "groupIds":["letter","witness"]},"desiredOutcomes":desired_outcomes()});
        let fixture = Self {
            directory,
            app,
            state,
            root,
            original,
            target,
            discussion,
        };
        fixture
            .save_execution_plan(&fixture.discussion, "author-revision-plan", INTENT)
            .await;
        fixture
    }

    pub fn path(&self) -> std::path::PathBuf {
        self.directory.path().join("outcomes.sqlite3")
    }
    pub fn project(&self) -> ProjectId {
        ProjectId::parse(self.root.trim_start_matches("/api/projects/")).unwrap()
    }
    pub fn repo(&self) -> SqliteStoryRepository {
        self.state.database().repository().unwrap()
    }
    pub fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.path()).unwrap()
    }
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        actor: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        call(
            &self.app,
            method,
            &format!("{}{path}", self.root),
            actor,
            body,
        )
        .await
    }
    pub async fn save_execution_plan(&self, task: &Value, key: &str, acceptance: &str) {
        let (status, saved) = self
            .request(
                "POST",
                &format!("/tasks/{}/work-plan", task["id"].as_str().unwrap()),
                CANDIDATE,
                json!({"taskId":task["id"],"threadId":task["threadId"],
                    "expectedRevision":task["inputRevision"],"expectedPlanVersion":0,
                    "idempotencyKey":key,"plan":{"intentMode":"executeGoals","goals":[{
                        "id":"revision-goal","title":"完成作者批准的选定场景修订及评估",
                        "acceptance":acceptance,"kind":"deliverable","status":"inProgress",
                        "dependsOn":[],"readingQuestion":"核对原稿与选定场景实际改动",
                        "checkScope":"作者批准的修订来源与同章场景","approach":"修订后评估真实成果",
                        "assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],
                        "continuation":null}]}}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(saved["activeGoalId"], "revision-goal");
        assert_eq!(saved["grantsExecution"], false);
    }
    pub fn actions_path(&self) -> String {
        format!("/tasks/{}/actions", self.discussion["id"].as_str().unwrap())
    }
    pub async fn propose(&self, target: Value, key: &str) -> (StatusCode, Value) {
        self.request(
            "POST",
            &self.actions_path(),
            CANDIDATE,
            json!({"taskId":self.discussion["id"],"threadId":"discussion","expectedRevision":0,
                "expectedIntentRevision":0,"idempotencyKey":key,"operation":"generateChapter",
                "target":target,"instruction":"局部改稿，并逐项评估作者批准的目标"}),
        )
        .await
    }
    pub fn task_request(&self, operation: &str, target: Value) -> Value {
        json!({"operation":operation,"target":target,"inputRevision":0,"threadId":operation,
            "authorIntent":"宿主包装语句不能替换作者原话",
            "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600})
    }
    pub async fn create_task(&self, operation: &str, target: Value) -> (StatusCode, Value) {
        self.request(
            "POST",
            "/tasks",
            AUTHOR,
            self.task_request(operation, target),
        )
        .await
    }
    pub async fn complete(&self, task: &Value) -> (StatusCode, Value) {
        let mut body = json!({"threadId":task["threadId"],"summary":"完成当前步骤"});
        if task["inputManifest"]["workDecision"].is_object() {
            let status_uri = format!(
                "/tasks/{}/work-decision?threadId={}&expectedRevision={}",
                task["id"].as_str().unwrap(),
                task["threadId"].as_str().unwrap(),
                task["inputRevision"]
            );
            let (code, first) = self
                .request("GET", &status_uri, CANDIDATE, Value::Null)
                .await;
            assert_eq!(code, StatusCode::OK, "{first}");
            // Mandatory work remains incomplete: do not fabricate a review to
            // bypass the expected TaskOutputMissing rejection in negative tests.
            if first["deliveryReviewRequired"] == true {
                self.read_author_requirements(task).await;
                let mut page = first.clone();
                let mut evidence = Vec::new();
                loop {
                    for receipt in page["receipts"]["items"].as_array().unwrap() {
                        if receipt["status"] != "completed" {
                            continue;
                        }
                        for output in receipt["outputs"].as_array().unwrap() {
                            if output["revisionId"].is_string() {
                                evidence.push(json!({"taskId":receipt["taskId"],"revisionId":output["revisionId"]}));
                            }
                        }
                    }
                    let Some(cursor) = page["pagination"]["nextCursor"].as_str() else {
                        break;
                    };
                    let (code, next) = self
                        .request(
                            "GET",
                            &format!("{status_uri}&cursor={}", cursor.replace(':', "%3A")),
                            CANDIDATE,
                            Value::Null,
                        )
                        .await;
                    assert_eq!(code, StatusCode::OK, "{next}");
                    assert_eq!(next["version"], first["version"]);
                    assert_eq!(next["sourceDigest"], first["sourceDigest"]);
                    page = next;
                }
                body["deliveryReview"] = json!({"expectedWorkVersion":first["version"],"expectedSourceDigest":first["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"已核对本次授权改稿和实际保存成果","checkedScope":"本Work已保存回执，不宣称额外文学质量检查","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":evidence});
            }
        }
        self.request(
            "POST",
            &format!("/tasks/{}/complete", task["id"].as_str().unwrap()),
            CANDIDATE,
            body,
        )
        .await
    }
    pub async fn read_author_requirements(&self, task: &Value) {
        let (status, context) = self
            .request(
                "GET",
                &format!(
                    "/context?taskId={}&threadId={}&projectRevision={}&purpose={}",
                    task["id"].as_str().unwrap(),
                    task["threadId"].as_str().unwrap(),
                    task["inputRevision"],
                    task["purpose"].as_str().unwrap()
                ),
                CANDIDATE,
                Value::Null,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{context}");
        let source = context["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["sectionPath"] == "/authorRequirementsSource")
            .unwrap();
        let mut cursor = None;
        loop {
            let (status,page)=self.request("GET",&format!("/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={}{}",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"],source["sourceId"].as_str().unwrap(),cursor.as_ref().map(|c:&String|format!("&cursor={}",c.replace(':',"%3A"))).unwrap_or_default()),CANDIDATE,Value::Null).await;
            assert_eq!(status, StatusCode::OK, "{page}");
            assert_eq!(page["contentHash"], source["contentHash"]);
            assert_eq!(page["contentRead"], true);
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
    }
    pub async fn issue_observations(&self, task: &Value, issue_id: &str) -> Vec<Value> {
        let mut cursor = 0;
        let mut observations = Vec::new();
        loop {
            let (status, page) = self
                .request(
                    "GET",
                    &format!(
                        "/tasks/{}/work-issues?threadId={}&expectedRevision={}&issueId={issue_id}&cursor={cursor}&limit=16",
                        task["id"].as_str().unwrap(),
                        task["threadId"].as_str().unwrap(),
                        task["inputRevision"]
                    ),
                    CANDIDATE,
                    Value::Null,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{page}");
            assert_eq!(page["issueId"], issue_id);
            observations.extend(page["entries"].as_array().unwrap().iter().cloned());
            if page["nextCursor"].is_null() {
                assert_eq!(
                    observations.len() as u64,
                    page["totalCount"].as_u64().unwrap()
                );
                break;
            }
            cursor = page["nextCursor"].as_u64().unwrap();
        }
        observations
    }
    pub fn settle(&self, task: &Value) {
        settle_fixture::settle(
            &self.path(),
            self.project().as_str(),
            task["id"].as_str().unwrap(),
        );
    }
    pub async fn writer(&self, target: Value) -> Value {
        let (status, action) = self.propose(target.clone(), "selected-revision").await;
        assert_eq!(status, StatusCode::CREATED, "{action}");
        assert_eq!(action["target"], target);
        let (status, completed) = self.complete(&self.discussion).await;
        assert_eq!(status, StatusCode::OK, "{completed}");
        self.settle(&self.discussion);
        let approval = format!(
            "{}/{}/approve",
            self.actions_path(),
            action["id"].as_str().unwrap()
        );
        let (status, approved) = self
            .request(
                "POST",
                &approval,
                AUTHOR,
                json!({"expectedIntentRevision":0}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{approved}");
        assert_eq!(approved["target"], target);
        let (_, permission) = self
            .request(
                "GET",
                &approval.replace("/approve", "/permission"),
                AUTHOR,
                Value::Null,
            )
            .await;
        assert_eq!(permission["permission"]["evidence"]["target"], target);
        let mut target = target;
        target["requestedByTaskId"] = self.discussion["id"].clone();
        target["requestedActionId"] = action["id"].clone();
        let (status, writer) = self.create_task("generateChapter", target.clone()).await;
        assert_eq!(status, StatusCode::CREATED, "{writer}");
        assert_eq!(writer["target"], target);
        assert_eq!(writer["inputManifest"]["authorIntent"], INTENT);
        writer
    }
    pub async fn decision(&self, source: &Value) -> (Value, Value) {
        let (code, work) = self
            .request(
                "GET",
                &format!("/tasks/{}/work-task", source["id"].as_str().unwrap()),
                AUTHOR,
                Value::Null,
            )
            .await;
        assert_eq!(code, StatusCode::OK, "{work}");
        let (code, workspace) = self
            .request("GET", "/creative-workspace", AUTHOR, Value::Null)
            .await;
        assert_eq!(code, StatusCode::OK, "{workspace}");
        let (code, continuation) = self.request("POST",&format!("/work-tasks/{}/decision",work["id"].as_str().unwrap()),AUTHOR,json!({"expectedWorkVersion":work["version"],"expectedRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]})).await;
        assert_eq!(code, StatusCode::OK, "{continuation}");
        let queued = &continuation["step"];
        let mut target = queued["target"].clone();
        target["resumeStepId"] = queued["id"].clone();
        let mut request = self.task_request("advisory", target);
        request["threadId"] = work["threadId"].clone();
        request["inputRevision"] = queued["inputRevision"].clone();
        let (code, parent) = self.request("POST", "/tasks", AUTHOR, request).await;
        assert_eq!(code, StatusCode::CREATED, "{parent}");
        let (code, mut status) = self
            .request(
                "GET",
                &format!(
                    "/tasks/{}/work-decision?threadId={}&expectedRevision={}",
                    parent["id"].as_str().unwrap(),
                    parent["threadId"].as_str().unwrap(),
                    parent["inputRevision"]
                ),
                CANDIDATE,
                Value::Null,
            )
            .await;
        assert_eq!(code, StatusCode::OK, "{status}");
        let mut cursor = status["pagination"]["nextCursor"].clone();
        while let Some(next) = cursor.as_str() {
            let (code, page) = self
                .request(
                    "GET",
                    &format!(
                        "/tasks/{}/work-decision?threadId={}&expectedRevision={}&cursor={next}",
                        parent["id"].as_str().unwrap(),
                        parent["threadId"].as_str().unwrap(),
                        parent["inputRevision"]
                    ),
                    CANDIDATE,
                    Value::Null,
                )
                .await;
            assert_eq!(code, StatusCode::OK, "{page}");
            assert_eq!(page["version"], status["version"]);
            for field in ["requirements", "readyActions"] {
                status[field]
                    .as_array_mut()
                    .unwrap()
                    .extend(page[field].as_array().unwrap().iter().cloned());
            }
            cursor = page["pagination"]["nextCursor"].clone();
        }
        if let Some(source) = status["chapterAcceptanceSource"]["sourceId"].as_str() {
            let mut cursor = None::<String>;
            let mut text = String::new();
            loop {
                let suffix = cursor
                    .as_ref()
                    .map(|c| format!("&cursor={c}"))
                    .unwrap_or_default();
                let (code, page) = self.request("GET", &format!(
                    "/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={source}{suffix}",
                    parent["id"].as_str().unwrap(), parent["threadId"].as_str().unwrap(), parent["inputRevision"]),
                    CANDIDATE, Value::Null).await;
                assert_eq!(code, StatusCode::OK, "{page}");
                assert_eq!(
                    page["contentHash"],
                    status["chapterAcceptanceSource"]["contentHash"]
                );
                text.push_str(page["payloadPart"].as_str().unwrap());
                cursor = page["nextCursor"].as_str().map(str::to_owned);
                if cursor.is_none() {
                    break;
                }
            }
            // Test-side aggregate; production status pages contain only the reference.
            status["chapterAcceptance"] = serde_json::from_str(&text).unwrap();
        }
        (parent, status)
    }
    pub async fn choose(&self, source: &Value, operation: &str) -> Value {
        let (parent, status) = self.decision(source).await;
        let action = status["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["operation"] == operation)
            .unwrap_or_else(|| panic!("missing {operation}: {status}"));
        let (code,selected)=self.request("POST",&format!("/tasks/{}/work-decision/actions",parent["id"].as_str().unwrap()),CANDIDATE,json!({"taskId":parent["id"],"threadId":parent["threadId"],"expectedRevision":parent["inputRevision"],"expectedWorkVersion":status["version"],"expectedSourceDigest":status["sourceDigest"],"actionKey":action["actionKey"],"instruction":INTENT,"idempotencyKey":"select-follow-up"})).await;
        assert_eq!(code, StatusCode::OK, "{selected}");
        assert_eq!(self.complete(&parent).await.0, StatusCode::OK);
        self.settle(&parent);
        selected["step"].clone()
    }
    pub async fn checkpoint(
        &self,
        task: Option<&Value>,
        previous: &Value,
        payload: Value,
        key: &str,
    ) -> (StatusCode, Value) {
        let mut request = json!({"expectedRevision":0,"expectedCandidateRevisionId":previous["revisionId"],
            "idempotencyKey":key,"payload":payload});
        let actor = if let Some(task) = task {
            request["taskId"] = task["id"].clone();
            request["threadId"] = task["threadId"].clone();
            CANDIDATE
        } else {
            AUTHOR
        };
        self.request("POST", "/drafts/checkpoints", actor, request)
            .await
    }
    pub async fn work(&self) -> Value {
        let (status, work) = self
            .request(
                "GET",
                &format!(
                    "/tasks/{}/work-task",
                    self.discussion["id"].as_str().unwrap()
                ),
                AUTHOR,
                Value::Null,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{work}");
        work
    }
    pub async fn grant(&self, task: &Value) -> Value {
        let (status, grant) = self
            .request(
                "GET",
                &format!("/tasks/{}/provider-budget", task["id"].as_str().unwrap()),
                AUTHOR,
                Value::Null,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{grant}");
        grant
    }
    pub fn ledger_counts(&self) -> Value {
        let db = self.db();
        let tables = [
            "story_tasks_v2",
            "story_work_tasks_v1",
            "story_work_steps_v1",
            "story_work_attempts_v1",
            "story_work_budgets_v1",
            "story_task_provider_budgets_v1",
            "story_revisions_v2",
            "story_candidate_receipts_v2",
            "story_task_outputs_v2",
            "story_snapshots_v2",
            "story_commits_v2",
            "story_reviews_v2",
            "story_acceptances_v2",
            "story_task_action_requests_v1",
            "story_action_permits_v1",
        ];
        json!(tables.map(|table| {
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
        }))
    }
}

pub(super) fn join_body(payload: &mut Value) {
    payload["body"] = json!(
        payload["sceneGroups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["body"].as_str().unwrap().trim())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    );
}
pub(super) fn step(work: &Value, operation: &str) -> Value {
    let matching = work["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["operation"] == operation)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1, "{work}");
    matching[0].clone()
}
pub(super) async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Value,
) -> (StatusCode, Value) {
    let explicitly_budgeted = body.get("initialBudget").is_some();
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json")
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    // Explicit budgets pass through unchanged; unfunded and derived claims stay raw.
    let response = if explicitly_budgeted {
        app.clone().author_fixture_request(request).await.unwrap()
    } else {
        app.clone().oneshot(request).await.unwrap()
    };
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        task_fixture::work_control_response(
            uri,
            status,
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
        ),
    )
}

/// Reassemble the immutable task source through the same HTTP route used by read_source_content(view=raw).
pub(super) async fn read_context_source(
    f: &Fixture,
    task: &Value,
    pack: &Value,
    path: &str,
) -> Value {
    assert_eq!(pack["taskSources"]["readTool"], "read_source_content");
    assert_eq!(pack["taskSources"]["textDelivered"], false);
    let entry = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == path)
        .expect("registered source");
    assert_eq!(
        pack.pointer(path).unwrap(),
        &json!({"sourceId":entry["sourceId"],"textDelivered":false})
    );
    let mut cursor = Value::Null;
    let mut text = String::new();
    loop {
        let suffix = cursor
            .as_str()
            .map(|value| format!("&cursor={value}"))
            .unwrap_or_default();
        let (status, page) = f
            .request(
                "GET",
                &format!(
                    "/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{suffix}",
                    task["id"].as_str().unwrap(),
                    task["threadId"].as_str().unwrap(),
                    task["inputRevision"],
                    entry["sourceId"].as_str().unwrap()
                ),
                CANDIDATE,
                Value::Null,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], entry["sourceId"]);
        assert_eq!(page["sectionPath"], path);
        assert_eq!(page["contentHash"], entry["contentHash"]);
        assert_eq!(page["byteRange"]["start"], text.len());
        let part = page["payloadPart"].as_str().unwrap();
        assert!(!part.is_empty() && part.len() <= 2000);
        assert!(page.to_string().len() <= 6500);
        text.push_str(part);
        assert_eq!(page["byteRange"]["end"], text.len());
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(page["byteRange"]["total"], text.len());
            break;
        }
        assert_eq!(page["endOfInputFromCursor"], false);
        assert!(text.len() < entry["payloadBytes"].as_u64().unwrap() as usize);
    }
    assert_eq!(entry["payloadBytes"], text.len());
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        entry["contentHash"].as_str().unwrap()
    );
    serde_json::from_str(&text).unwrap()
}
