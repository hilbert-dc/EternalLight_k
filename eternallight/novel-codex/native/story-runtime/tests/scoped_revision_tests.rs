use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{
    ProjectId, RecoveryOperationBinding, SqliteStoryRepository, TaskUpdate,
};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

#[path = "../../../shared/draft-checkpoint-identity.rs"]
mod draft_identity;
#[path = "../../../shared/novel-operation-digest.rs"]
mod operation_digest;

const AUTHOR: &str = "scene-author-token-123456789012345678";
const CANDIDATE: &str = "scene-candidate-token-123456789012345";
const CAPABILITY: &str = "scene-capability-token-12345678901234";

struct Fixture {
    _directory: tempfile::TempDir,
    app: Router,
    root: String,
    original: Value,
    target: Value,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_single_group(false).await
    }

    async fn with_single_group(single_group: bool) -> Self {
        Self::with_source(single_group, false).await
    }

    async fn with_body_only_source() -> Self {
        Self::with_source(true, true).await
    }

    async fn with_source(single_group: bool, source_body_only: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let app = router(task_fixture::state(
            directory.path().join("scenes.sqlite3"),
            AUTHOR.into(),
            CANDIDATE.into(),
        ));
        let (_, created) = call(
            &app,
            "POST",
            "/api/projects",
            AUTHOR,
            Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
        )
        .await;
        let root = format!(
            "/api/projects/{}",
            created["project"]["id"].as_str().unwrap()
        );
        let mut payload = json!({"chapterId":"chapter-one","chapterNumber":1,"title":"密函",
            "body":"林夕走进港口。\n\n---\n\n她烧掉密函。\n\n---\n\n白塔熄灭。",
            "sceneGroups":[
                {"id":"opening","sceneId":"plan-opening","body":"林夕走进港口。","locked":false},
                {"id":"letter","sceneId":"plan-letter","body":"她烧掉密函。","locked":false},
                {"id":"ending","sceneId":null,"body":"白塔熄灭。","locked":true}]});
        if single_group {
            payload["sceneGroups"] = json!([{"id":"scene-group-chapter-one","sceneId":null,
                "body":payload["body"],"locked":false}]);
        }
        if source_body_only {
            // Whole-chapter Author saves accept raw body whitespace when its
            // trimmed text agrees with Runtime's canonical generated group.
            payload.as_object_mut().unwrap().remove("sceneGroups");
            payload["body"] = json!(format!("{}\n", payload["body"].as_str().unwrap()));
        }
        let (status, original) = call(&app,"POST",&format!("{root}/drafts/checkpoints"),AUTHOR,
            Some(json!({"payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"original"}))).await;
        assert_eq!(status, StatusCode::CREATED, "{original}");
        let (_, workspace) = call(
            &app,
            "GET",
            &format!("{root}/creative-workspace"),
            AUTHOR,
            /*body*/ None,
        )
        .await;
        let mut baseline = workspace["chapters"][0]["candidate"].clone();
        baseline["revisionId"] = original["revisionId"].clone();
        baseline["title"] = payload["title"].clone();
        baseline["chapterNumber"] = payload["chapterNumber"].clone();
        let target = json!({"type":"chapter","chapterId":"chapter-one","chapterNumber":1,"readingChapter":1,"sceneOrder":1,
            "scope":{"type":"selectedScenes","sourceRevisionId":original["revisionId"],
                "groupIds":[if single_group { "scene-group-chapter-one" } else { "letter" }]}});
        Self {
            _directory: directory,
            app,
            root,
            original: baseline,
            target,
        }
    }

    async fn create_task(&self, operation: &str, target: Value) -> (StatusCode, Value) {
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        call(&self.app,"POST",&format!("{}/tasks",self.root),AUTHOR,Some(json!({
            "operation":operation,"target":target,"inputRevision":0,"threadId":operation,
            "authorIntent":"只把密函场景的烧信改成藏信，保留其他内容",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires}))).await
    }

    async fn save(
        &self,
        task: &Value,
        previous: &Value,
        payload: Value,
        key: &str,
    ) -> (StatusCode, Value) {
        call(&self.app,"POST",&format!("{}/drafts/checkpoints",self.root),CANDIDATE,Some(json!({
            "taskId":task["id"],"threadId":task["threadId"],"expectedRevision":0,
            "expectedCandidateRevisionId":previous["revisionId"],"idempotencyKey":key,"payload":payload}))).await
    }

    async fn workspace(&self) -> Value {
        call(
            &self.app,
            "GET",
            &format!("{}/creative-workspace", self.root),
            AUTHOR,
            /*body*/ None,
        )
        .await
        .1
    }

    async fn model(&self, draft: &Value, quote: &str, value: &str) {
        let (status,batch) = call(&self.app,"POST",&format!("{}/modeling-batches",self.root),AUTHOR,
            Some(json!({"objectId":"draft-chapter-one","sourceRevisionId":draft["revisionId"],"expectedRevision":0,"extractorVersion":"scoped-test"}))).await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let (status, task) = self
            .create_task("modelKnowledge", json!({"batchId":batch["id"]}))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let body = draft["body"].as_str().unwrap();
        let start = body[..body.find(quote).unwrap()].chars().count();
        let evidence = json!({"quote":quote,"start":start,"end":start+quote.chars().count()});
        let (status,published) = call(&self.app,"POST",&format!("{}/modeling-batches/{}/publish",self.root,batch["id"].as_str().unwrap()),CANDIDATE,
            Some(json!({"taskId":task["id"],"threadId":"modelKnowledge","expectedRevision":0,
                "entities":[{"id":"letter","kind":"item","displayName":"密函","aliases":[],"evidence":evidence}],
                "assertions":[{"kind":"assertion","subjectId":"letter","predicate":"action","value":value,"evidence":evidence}]}))).await;
        assert_eq!(status, StatusCode::OK, "{published}");
        let (status, complete) = call(
            &self.app,
            "POST",
            &format!(
                "{}/tasks/{}/complete",
                self.root,
                task["id"].as_str().unwrap()
            ),
            CANDIDATE,
            Some(json!({"threadId":"modelKnowledge","summary":"已按原文建模"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{complete}");
    }

    fn path(&self) -> std::path::PathBuf {
        self._directory.path().join("scenes.sqlite3")
    }

    fn project(&self) -> ProjectId {
        ProjectId::parse(self.root.trim_start_matches("/api/projects/")).unwrap()
    }

    fn repo(&self) -> SqliteStoryRepository {
        SqliteStoryRepository::open(&self.path()).unwrap()
    }

    /// Current Author/Native contracts establish the same Work as the original
    /// conversation; fixture transport grants no extra model permissions.
    async fn scoped_writer(&self) -> Value {
        let (status, parent) = self
            .create_task("advisory", json!({"type":"project"}))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{parent}");
        let id = parent["id"].as_str().unwrap();
        let (status, plan) = call(&self.app, "POST", &format!("{}/tasks/{id}/work-plan", self.root), CANDIDATE,
            Some(json!({"taskId":id,"threadId":"advisory","expectedRevision":0,"expectedPlanVersion":0,
                "idempotencyKey":"scoped-recovery-plan","plan":{"intentMode":"executeGoals","goals":[{
                    "id":"local-revision","title":"只改密函场景","acceptance":"保留其他正文与锁定结尾",
                    "kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"实际改稿是否保留保护内容",
                    "checkScope":"选定场景","approach":"局部修改后独立核对","assessment":"","sourceIds":[],
                    "sourceChecks":[],"evidence":[],"continuation":null}]}}))).await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let mut target = self.target.clone();
        target["chapterAcceptance"] = json!({"requirements":[{"id":"protected-text","text":"密函改为藏信，保留其他正文"}],
            "length":{"requirement":"作者未规定长度","minCharacters":null,"maxCharacters":null}});
        let actions = format!("{}/tasks/{id}/actions", self.root);
        let (status, action) = call(&self.app, "POST", &actions, CANDIDATE,
            Some(json!({"taskId":id,"threadId":"advisory","expectedRevision":0,"expectedIntentRevision":0,
                "idempotencyKey":"scoped-recovery-action","operation":"generateChapter","target":target,
                "instruction":"只改密函场景为藏信"}))).await;
        assert_eq!(status, StatusCode::CREATED, "{action}");
        let (status, completed) = call(
            &self.app,
            "POST",
            &format!("{}/tasks/{id}/complete", self.root),
            CANDIDATE,
            Some(json!({"threadId":"advisory","summary":"已登记局部修订，正文尚未生成"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{completed}");
        settle_fixture::settle(&self.path(), self.project().as_str(), id);
        let (status, approved) = call(
            &self.app,
            "POST",
            &format!("{actions}/{}/approve", action["id"].as_str().unwrap()),
            AUTHOR,
            Some(json!({"expectedIntentRevision":0})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{approved}");
        target["requestedByTaskId"] = parent["id"].clone();
        target["requestedActionId"] = action["id"].clone();
        let (status, writer) = self.create_task("generateChapter", target).await;
        assert_eq!(status, StatusCode::CREATED, "{writer}");
        self.repo()
            .bind_task_turn(
                &self.project(),
                writer["id"].as_str().unwrap(),
                "generateChapter",
                "writer-turn",
            )
            .unwrap();
        writer
    }

    fn arguments(&self, writer: &Value, payload: Value, key: &str) -> Value {
        json!({"projectId":self.project().as_str(),"taskId":writer["id"],"threadId":"generateChapter",
            "expectedRevision":0,"expectedCandidateRevisionId":self.original["revisionId"],
            "idempotencyKey":key,"payload":payload})
    }

    fn semantic_arguments(arguments: &Value) -> Value {
        let mut semantic = arguments.clone();
        for field in ["projectId", "taskId", "threadId"] { semantic.as_object_mut().unwrap().remove(field); }
        semantic
    }

    fn evidence(&self, arguments: Value, call_id: &str) -> Value {
        json!({"taskId":arguments["taskId"],"threadId":"generateChapter","turnId":"writer-turn","callId":call_id,
            "toolName":"save_draft_checkpoint","requestHash":operation_digest::operation_digest("save_draft_checkpoint", &Self::semantic_arguments(&arguments)).unwrap(),
            "arguments":Self::semantic_arguments(&arguments)})
    }

    /// Models an interruption after actual Native admission, not a rejected row
    /// edited back to accepted. Only isolated current-schema databases use this.
    fn admit(&self, arguments: Value, call_id: &str) -> Value {
        let evidence = self.evidence(arguments, call_id);
        self.repo()
            .bind_recovery_operation(RecoveryOperationBinding {
                project_id: self.project(),
                task_id: evidence["taskId"].as_str().unwrap().into(),
                thread_id: "generateChapter".into(),
                turn_id: "writer-turn".into(),
                call_id: call_id.into(),
                tool_name: "save_draft_checkpoint".into(),
                request_hash: evidence["requestHash"].as_str().unwrap().into(),
                idempotency_ref: Some(format!(
                    "sha256:{:x}",
                    Sha256::digest(
                        evidence["arguments"]["idempotencyKey"]
                            .to_string()
                            .as_bytes()
                    )
                )),
                now: 1,
            })
            .unwrap();
        evidence
    }

    async fn save_exact(&self, arguments: &Value, call_id: &str) -> (StatusCode, Value) {
        let mut body = arguments.clone();
        body.as_object_mut().unwrap().remove("projectId");
        // The public MCP request may be sceneGroups-only. Its HTTP conversion
        // provides the canonical body; identity hashes retain the raw arguments.
        if body["payload"].get("body").is_none() {
            join_body(&mut body["payload"]);
        }
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("{}/drafts/checkpoints", self.root))
                    .header("authorization", format!("Bearer {CANDIDATE}"))
                    .header("content-type", "application/json")
                    .header("x-novel-task-capability", CAPABILITY)
                    .header("x-novel-tool-name", "save_draft_checkpoint")
                    .header("x-novel-root-thread", "generateChapter")
                    .header("x-novel-caller-thread", "generateChapter")
                    .header("x-novel-turn", "writer-turn")
                    .header("x-novel-tool-call", call_id)
                    .header(
                        "x-novel-request-hash",
                        operation_digest::operation_digest("save_draft_checkpoint", &Self::semantic_arguments(arguments))
                            .unwrap(),
                    )
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
        )
    }

    async fn exact(&self, evidence: &Value) -> Value {
        let (status, result) = call(&self.app, "GET", &format!(
            "{}/tasks/recovery-operations?taskId={}&threadId=generateChapter&turnId=writer-turn&callId={}&toolName=save_draft_checkpoint&requestHash={}",
            self.root,evidence["taskId"].as_str().unwrap(),evidence["callId"].as_str().unwrap(),evidence["requestHash"].as_str().unwrap()), AUTHOR,None).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        result
    }

    async fn reconcile(&self, actor: &str, evidence: Value) -> (StatusCode, Value) {
        call(
            &self.app,
            "POST",
            &format!("{}/tasks/recovery-operations/reconcile", self.root),
            actor,
            Some(evidence),
        )
        .await
    }

    async fn finish_writer(&self, writer: &Value) {
        let (status, result) = call(
            &self.app,
            "POST",
            &format!(
                "{}/tasks/{}/complete",
                self.root,
                writer["id"].as_str().unwrap()
            ),
            CANDIDATE,
            Some(json!({"threadId":"generateChapter","summary":"已保存局部工作稿，仍待独立检查"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        settle_fixture::settle(
            &self.path(),
            self.project().as_str(),
            writer["id"].as_str().unwrap(),
        );
    }

    async fn completed_same_key_calls(&self) -> (Value, Value, Value, Value) {
        let writer = self.scoped_writer().await;
        let original_payload = body_only_with_newline(self);
        let original_body = original_payload["body"].clone();
        let original = self.admit(
            self.arguments(&writer, original_payload, "shared-save-key"),
            "original-same-key-call",
        );
        let mut corrected = local_payload(self);
        corrected["sceneGroups"][0]["body"] = original_body.clone();
        corrected.as_object_mut().unwrap().remove("body");
        let corrected_arguments = self.arguments(&writer, corrected, "shared-save-key");
        let corrected = self.evidence(corrected_arguments.clone(), "corrected-same-key-call");
        let (status, saved) = self
            .save_exact(&corrected_arguments, "corrected-same-key-call")
            .await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        assert_eq!(saved["sceneGroups"][0]["body"], original_body);
        assert_eq!(saved["body"], json!(original_body.as_str().unwrap().trim()));
        assert_eq!(
            original["arguments"]["idempotencyKey"],
            corrected["arguments"]["idempotencyKey"]
        );
        assert_ne!(original["requestHash"], corrected["requestHash"]);
        assert_eq!(
            self.exact(&original).await["operation"]["state"],
            "accepted"
        );
        assert_eq!(
            self.exact(&corrected).await["operation"]["state"],
            "committed"
        );
        self.finish_writer(&writer).await;
        (writer, original, saved, corrected)
    }

    async fn writer_event(&self, writer: &Value) -> Value {
        let (status, events) = call(
            &self.app,
            "GET",
            &format!("{}/work-execution-events", self.root),
            AUTHOR,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{events}");
        events["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["sourceAttemptId"] == writer["id"])
            .unwrap()
            .clone()
    }

    async fn claim_event(&self, event: &Value) -> (StatusCode, Value) {
        let workspace = self.workspace().await;
        let (status, work) = call(
            &self.app,
            "GET",
            &format!(
                "{}/work-tasks/{}",
                self.root,
                event["workTaskId"].as_str().unwrap()
            ),
            AUTHOR,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{work}");
        call(&self.app, "POST", &format!("{}/work-execution-events/{}/claim", self.root,event["eventId"].as_str().unwrap()), AUTHOR,
            Some(json!({"hostSessionId":"scoped-recovery-host","expectedWorkVersion":work["version"],
                "expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"]}))).await
    }

    /// All persisted domain/author/Work/budget facts, including complete row
    /// values. Exclude only this precise operation's verdict and timestamp.
    fn facts_except_verdict(&self, call_id: &str) -> Value {
        json!({"native":database_rows(&self.path(), Some(call_id)),
            "engine":database_rows(&self.path().with_extension("engine.sqlite3"), None)})
    }
}

fn database_rows(path: &std::path::Path, except_call: Option<&str>) -> Value {
    let db = rusqlite::Connection::open(path).unwrap();
    let mut tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    let names = tables
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut result = serde_json::Map::new();
    for name in names {
        let mut statement = db.prepare(&format!("SELECT * FROM \"{name}\"")).unwrap();
        let columns = statement
            .column_names()
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>();
        let mut values = statement
            .query_map([], |row| {
                let mut object = serde_json::Map::new();
                for (index, column) in columns.iter().enumerate() {
                    let value = match row.get_ref(index)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(v) => json!(v),
                        rusqlite::types::ValueRef::Real(v) => json!(v),
                        rusqlite::types::ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
                        rusqlite::types::ValueRef::Blob(v) => json!(v),
                    };
                    object.insert(column.clone(), value);
                }
                if name == "story_tool_operations_v1"
                    && except_call.is_some_and(|id| object["call_id"] == id)
                {
                    object.remove("state");
                    object.remove("updated_at");
                }
                Ok(Value::Object(object))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        values.sort_by_key(Value::to_string);
        result.insert(name, json!(values));
    }
    Value::Object(result)
}

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

async fn read_context_source(
    fixture: &Fixture,
    task: &Value,
    context: &Value,
    path: &str,
) -> Value {
    assert_eq!(context["taskSources"]["readTool"], "read_source_content");
    assert_eq!(context["taskSources"]["textDelivered"], false);
    let source = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["sectionPath"] == path)
        .expect("registered frozen source");
    assert_eq!(
        context.pointer(path).unwrap(),
        &json!({"sourceId":source["sourceId"],"textDelivered":false})
    );
    let mut cursor = Value::Null;
    let mut text = String::new();
    loop {
        let suffix = cursor
            .as_str()
            .map(|cursor| format!("&cursor={cursor}"))
            .unwrap_or_default();
        let (status, page) = call(
            &fixture.app,
            "GET",
            &format!(
                "{}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{suffix}",
                fixture.root,
                task["id"].as_str().unwrap(),
                task["threadId"].as_str().unwrap(),
                task["inputRevision"],
                source["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source["sourceId"]);
        assert_eq!(page["sectionPath"], path);
        assert_eq!(page["contentHash"], source["contentHash"]);
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
        assert!(text.len() < source["payloadBytes"].as_u64().unwrap() as usize);
    }
    assert_eq!(source["payloadBytes"], text.len());
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        source["contentHash"].as_str().unwrap()
    );
    serde_json::from_str(&text).unwrap()
}

fn join_body(payload: &mut Value) {
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

fn local_payload(fixture: &Fixture) -> Value {
    let mut payload = fixture.original.clone();
    if payload["sceneGroups"].as_array().unwrap().len() == 1 {
        payload["sceneGroups"][0]["body"] = json!(
            payload["sceneGroups"][0]["body"]
                .as_str()
                .unwrap()
                .replace("她烧掉密函。", "她将密函藏进衣襟。")
        );
    } else {
        payload["sceneGroups"][1]["body"] = json!("她将密函藏进衣襟。");
    }
    join_body(&mut payload);
    json!({"chapterId":"chapter-one","chapterNumber":1,"title":payload["title"],"body":payload["body"],
        "sceneGroups":payload["sceneGroups"],"sourcePlanRefs":payload["sourcePlanRefs"],"completeness":"chapterComplete"})
}

fn body_only_with_newline(fixture: &Fixture) -> Value {
    let mut payload = local_payload(fixture);
    payload.as_object_mut().unwrap().remove("sceneGroups");
    payload["body"] = json!(format!("{}\n", payload["body"].as_str().unwrap()));
    payload
}

#[tokio::test]
async fn text_range_authority_rejects_outside_punctuation_and_stale_or_invalid_ranges() {
    let mut f=Fixture::with_single_group(true).await;
    let body=f.original["sceneGroups"][0]["body"].as_str().unwrap();
    let start=body[..body.find("她烧掉密函。").unwrap()].chars().count();
    let end=start+"她烧掉密函。".chars().count();
    f.target["scope"]["bodyRanges"]=json!([{"groupId":"scene-group-chapter-one","start":start,"end":end}]);
    for change in [json!([]),json!([{"groupId":"unknown","start":start,"end":end}]),json!([{"groupId":"scene-group-chapter-one","start":start,"end":10000}])] {
        let mut invalid=f.target.clone();invalid["scope"]["bodyRanges"]=change;
        let (status,_)=f.create_task("generateChapter",invalid).await;
        assert!(status.is_client_error());
    }
    let task=f.scoped_writer().await;
    let mut outside=local_payload(&f);
    outside["sceneGroups"][0]["body"]=json!(outside["sceneGroups"][0]["body"].as_str().unwrap().replace("林夕走进港口。","林夕走进港口！"));
    join_body(&mut outside);
    let (status,_)=f.save(&task,&f.original,outside,"outside-punctuation").await;
    assert!(status.is_client_error());
    let (status,saved)=f.save(&task,&f.original,local_payload(&f),"only-authorized-text").await;
    assert!(status.is_success(),"{saved}");
    let (status,_)=f.create_task("generateChapter",f.target.clone()).await;
    assert!(status.is_client_error(),"old coordinates must not bind a new head");
}

#[tokio::test]
async fn body_only_single_selected_group_newline_is_rejected_but_identical_group_body_can_save() {
    let f = Fixture::with_single_group(true).await;
    let writer = f.scoped_writer().await;
    let payload = body_only_with_newline(&f);
    let original_body = payload["body"].clone();
    let arguments = f.arguments(&writer, payload, "only-body-newline");
    let evidence = f.evidence(arguments.clone(), "only-body-call");
    let before = f.workspace().await;
    let (status, rejection) = f.save_exact(&arguments, "only-body-call").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejection}");
    assert_eq!(f.exact(&evidence).await["operation"]["state"], "rejected");
    assert_eq!(f.workspace().await, before);
    // Every other scope/metadata constraint is valid. Runtime's body-only
    // conversion retains the newline, so exact selectedScenes body alignment
    // is the original failure; the correction changes representation only.
    let mut corrected = local_payload(&f);
    corrected["sceneGroups"][0]["body"] = original_body.clone();
    corrected.as_object_mut().unwrap().remove("body");
    let args = f.arguments(&writer, corrected, "identical-scene-group-body");
    let (status, saved) = f.save_exact(&args, "scene-group-call").await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["sceneGroups"][0]["body"], original_body);
    assert_eq!(saved["body"], json!(original_body.as_str().unwrap().trim()));
    assert_eq!(
        f.exact(&f.evidence(args, "scene-group-call")).await["operation"]["state"],
        "committed"
    );
}

#[tokio::test]
async fn scoped_prewrite_rejections_are_exact_and_same_writer_can_save_complete_and_handoff() {
    let f = Fixture::new().await;
    let writer = f.scoped_writer().await;
    let first = f.arguments(
        &writer,
        body_only_with_newline(&f),
        "body-only-trailing-newline",
    );
    let evidence = f.evidence(first.clone(), "body-only-call");
    let before = f.workspace().await;
    let (status, rejected) = f.save_exact(&first, "body-only-call").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let exact = f.exact(&evidence).await;
    assert_eq!(exact["operation"]["state"], "rejected");
    assert!(exact["operation"]["receipt_ref"].is_null());
    assert_eq!(f.workspace().await, before);

    // Same constructor-owned, zero-write stage: protection failures must not
    // become uncertain writes and block an otherwise corrected task.
    for index in 0..5 {
        let mut payload = local_payload(&f);
        match index {
            0 => payload["sceneGroups"][0]["body"] = json!("擅自改开场。"),
            1 => payload["sceneGroups"][1]["sceneId"] = json!("different-plan"),
            2 => payload["sceneGroups"].as_array_mut().unwrap().swap(0, 1),
            3 => payload["title"] = json!("擅自改标题"),
            4 => payload["sceneGroups"][2]["locked"] = json!(false),
            _ => unreachable!(),
        }
        join_body(&mut payload);
        let call_id = format!("protection-{index}");
        let args = f.arguments(&writer, payload, &format!("protection-key-{index}"));
        let (status, rejection) = f.save_exact(&args, &call_id).await;
        assert!(status.is_client_error(), "{rejection}");
        assert_eq!(
            f.exact(&f.evidence(args, &call_id)).await["operation"]["state"],
            "rejected",
            "protection-{index}"
        );
        assert_eq!(f.workspace().await, before);
    }

    let mut groups_only = local_payload(&f);
    groups_only.as_object_mut().unwrap().remove("body");
    let corrected = f.arguments(&writer, groups_only, "corrected-scene-groups");
    let (status, saved) = f.save_exact(&corrected, "corrected-call").await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let proof = f.exact(&f.evidence(corrected, "corrected-call")).await;
    assert_eq!(proof["operation"]["state"], "committed");
    assert_eq!(
        proof["operation"]["receipt_ref"],
        format!(
            "runtime:candidate:{}",
            saved["revisionId"].as_str().unwrap()
        )
    );
    assert_eq!(f.exact(&evidence).await, exact);
    f.finish_writer(&writer).await;
    let event = f.writer_event(&writer).await;
    let (status, claim) = f.claim_event(&event).await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    assert_eq!(claim["admission"], "claimed", "{claim}");
    assert_eq!(claim["event"]["sourceAttemptId"], writer["id"]);
    assert_eq!(claim["event"]["rootThreadId"], "advisory");
    assert_eq!(claim["work"]["id"], event["workTaskId"]);
    assert_eq!(f.workspace().await["chapters"][0]["accepted"], Value::Null);
}

#[tokio::test]
async fn completed_scoped_writer_reconciles_only_original_alignment_call_and_original_event() {
    let f = Fixture::with_body_only_source().await;
    let source_body = f.original["body"].as_str().unwrap();
    let source_group = f.original["sceneGroups"][0]["body"].as_str().unwrap();
    assert_eq!(source_body, source_group);
    assert_ne!(source_body, source_group.trim());
    assert_eq!(source_body.trim(), source_group.trim());
    let writer = f.scoped_writer().await;
    let original_payload = body_only_with_newline(&f);
    let original_body = original_payload["body"].clone();
    let evidence = f.admit(
        f.arguments(&writer, original_payload, "interrupted-alignment-key"),
        "interrupted-alignment-call",
    );
    let mut corrected = local_payload(&f);
    corrected["sceneGroups"][0]["body"] = original_body.clone();
    corrected.as_object_mut().unwrap().remove("body");
    let (status, saved) = f
        .save_exact(
            &f.arguments(&writer, corrected, "another-valid-key"),
            "another-valid-call",
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["sceneGroups"][0]["body"], original_body);
    assert_eq!(saved["body"], json!(original_body.as_str().unwrap().trim()));
    f.finish_writer(&writer).await;
    assert_eq!(
        f.repo()
            .task(&f.project(), writer["id"].as_str().unwrap())
            .unwrap()
            .unwrap()
            .status,
        "completed"
    );
    let event = f.writer_event(&writer).await;
    assert_eq!(
        f.claim_event(&event).await.1["blockedReason"],
        "unknownWrite"
    );
    let original = f.exact(&evidence).await;
    assert_eq!(original["operation"]["state"], "accepted");
    let before = f.facts_except_verdict("interrupted-alignment-call");
    assert_eq!(
        f.reconcile(CANDIDATE, evidence.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.facts_except_verdict("interrupted-alignment-call"), before);
    let (status, proof) = f.reconcile(AUTHOR, evidence.clone()).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operation"]["state"], "rejected");
    assert!(proof["operation"]["receipt_ref"].is_null());
    assert_eq!(f.facts_except_verdict("interrupted-alignment-call"), before);
    assert_eq!(f.reconcile(AUTHOR, evidence).await, (StatusCode::OK, proof));
    assert_eq!(f.facts_except_verdict("interrupted-alignment-call"), before);
    let (status, claim) = f.claim_event(&event).await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    assert_eq!(claim["admission"], "claimed", "{claim}");
    assert_eq!(claim["event"]["sourceAttemptId"], writer["id"]);
    assert_eq!(claim["work"]["id"], event["workTaskId"]);
    assert_eq!(
        f.workspace().await["chapters"][0]["candidate"]["id"],
        saved["revisionId"]
    );
}

#[tokio::test]
async fn corrected_same_key_receipt_proves_only_original_scoped_call_rejected_and_hands_off() {
    let f = Fixture::with_body_only_source().await;
    let (writer, original, saved, corrected) = f.completed_same_key_calls().await;
    let event = f.writer_event(&writer).await;
    assert_eq!(
        f.claim_event(&event).await.1["blockedReason"],
        "unknownWrite"
    );
    let other_proof = f.exact(&corrected).await;
    let before = f.facts_except_verdict("original-same-key-call");
    let (status, proof) = f.reconcile(AUTHOR, original.clone()).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operation"]["state"], "rejected");
    assert!(proof["operation"]["receipt_ref"].is_null());
    assert_eq!(f.facts_except_verdict("original-same-key-call"), before);
    assert_eq!(f.exact(&corrected).await, other_proof);
    assert_eq!(f.reconcile(AUTHOR, original).await, (StatusCode::OK, proof));
    assert_eq!(f.facts_except_verdict("original-same-key-call"), before);
    let (status, claim) = f.claim_event(&event).await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    assert_eq!(claim["admission"], "claimed", "{claim}");
    assert_eq!(claim["event"]["sourceAttemptId"], writer["id"]);
    assert_eq!(claim["work"]["id"], event["workTaskId"]);
    assert_eq!(
        f.workspace().await["chapters"][0]["candidate"]["id"],
        saved["revisionId"]
    );
}

#[tokio::test]
async fn same_key_receipts_without_exact_corrected_call_proof_keep_original_unknown() {
    for edge in [
        "matching-original-domain-digest",
        "missing-candidate",
        "missing-commit",
        "digest-pair-mismatch",
        "snapshot-pair-mismatch",
        "project-revision-mismatch",
        "foreign-branch",
        "extra-foreign-receipt",
        "other-op-uncommitted",
        "other-op-wrong-task",
        "other-receipt-not-own-output",
        "unapplied-commit",
        "snapshot-missing-output-member",
    ] {
        let f = Fixture::with_body_only_source().await;
        let (writer, original, _, _) = f.completed_same_key_calls().await;
        // Current-schema, isolated contradiction fixtures. None is a receipt
        // authorizing this original raw body-only request to be called saved.
        let db = rusqlite::Connection::open(f.path()).unwrap();
        let project = f.project();
        let pair = [project.as_str(), "shared-save-key"];
        match edge {
            "matching-original-domain-digest" => {
                let normalized = operation_digest::host_json(&original["arguments"]).unwrap();
                let domain = draft_identity::draft_request_digest(
                    &normalized["payload"],
                    normalized["expectedRevision"].as_i64().unwrap(),
                    normalized["expectedCandidateRevisionId"].as_str(),
                );
                assert_ne!(domain, original["requestHash"].as_str().unwrap());
                let saved_domain: String = db.query_row(
                    "SELECT request_digest FROM story_candidate_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2",
                    pair, |row| row.get(0),
                ).unwrap();
                assert_ne!(domain, saved_domain);
                assert_eq!(db.execute("UPDATE story_candidate_receipts_v2 SET request_digest=?3 WHERE project_id=?1 AND idempotency_key=?2", rusqlite::params![project.as_str(),"shared-save-key",domain]).unwrap(), 1);
                assert_eq!(db.execute("UPDATE story_receipts_v2 SET request_digest=?3 WHERE project_id=?1 AND idempotency_key=?2", rusqlite::params![project.as_str(),"shared-save-key",domain]).unwrap(), 1);
            }
            "missing-candidate" => {
                assert_eq!(db.execute("DELETE FROM story_candidate_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
            }
            "missing-commit" => {
                assert_eq!(
                    db.execute(
                        "DELETE FROM story_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2",
                        pair
                    )
                    .unwrap(),
                    1
                );
            }
            "digest-pair-mismatch" => {
                assert_eq!(db.execute("UPDATE story_candidate_receipts_v2 SET request_digest='different-domain-digest' WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
            }
            "snapshot-pair-mismatch" => {
                assert_eq!(db.execute("UPDATE story_receipts_v2 SET result_snapshot_id='different-result-snapshot' WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
            }
            "project-revision-mismatch" => {
                assert_eq!(db.execute("UPDATE story_candidate_receipts_v2 SET project_revision=project_revision+1 WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
            }
            "foreign-branch" => {
                assert_eq!(db.execute("UPDATE story_candidate_receipts_v2 SET branch_id='other-branch' WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
                assert_eq!(db.execute("UPDATE story_receipts_v2 SET branch_id='other-branch' WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
            }
            "extra-foreign-receipt" => {
                assert_eq!(db.execute("INSERT INTO story_candidate_receipts_v2 SELECT project_id,'other-branch',idempotency_key,request_digest,revision_id,result_snapshot_id,project_revision FROM story_candidate_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2", pair).unwrap(), 1);
            }
            "other-op-uncommitted" => {
                assert_eq!(db.execute("UPDATE story_tool_operations_v1 SET state='accepted',receipt_ref=NULL WHERE call_id='corrected-same-key-call'", []).unwrap(), 1);
            }
            "other-op-wrong-task" => {
                assert_eq!(db.execute("UPDATE story_tool_operations_v1 SET task_id=(SELECT task_id FROM story_tasks_v2 WHERE operation='advisory' LIMIT 1) WHERE call_id='corrected-same-key-call'", []).unwrap(), 1);
            }
            "other-receipt-not-own-output" => {
                assert_eq!(db.execute("UPDATE story_candidate_receipts_v2 SET revision_id=?3 WHERE project_id=?1 AND idempotency_key=?2", rusqlite::params![project.as_str(),"shared-save-key",f.original["revisionId"].as_str().unwrap()]).unwrap(), 1);
            }
            "unapplied-commit" => {
                assert_eq!(db.execute("UPDATE story_commits_v2 SET status='undone' WHERE commit_id=(SELECT commit_id FROM story_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2)", pair).unwrap(), 1);
            }
            "snapshot-missing-output-member" => {
                assert_eq!(db.execute("DELETE FROM story_snapshot_members_v2 WHERE snapshot_id=(SELECT result_snapshot_id FROM story_candidate_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2) AND object_id='draft-chapter-one'", pair).unwrap(), 1);
            }
            _ => unreachable!(),
        }
        drop(db);
        let before = f.facts_except_verdict("different-call");
        let original_proof = f.exact(&original).await;
        assert_eq!(original_proof["operation"]["state"], "accepted");
        assert_eq!(
            f.reconcile(AUTHOR, original).await,
            (StatusCode::OK, original_proof),
            "{edge}"
        );
        assert_eq!(f.facts_except_verdict("different-call"), before, "{edge}");
        assert_eq!(
            f.repo()
                .task(&project, writer["id"].as_str().unwrap())
                .unwrap()
                .unwrap()
                .status,
            "completed"
        );
    }
}

#[tokio::test]
async fn scoped_alignment_reconciliation_never_infers_from_later_output_or_receipts() {
    for edge in [
        "legal-body",
        "running",
        "unsealed",
        "completed-no-output",
        "candidate-receipt",
        "commit-receipt",
        "prefix",
        "valid-groups",
        "delegated",
        "wrong-cas-with-output",
    ] {
        let f = Fixture::new().await;
        let writer = if edge == "prefix" {
            let mut target = f.target.clone();
            target["scope"] = json!({"type":"throughScene","endSceneOrder":1});
            target["sceneOrder"] = json!(1);
            let (status, writer) = f.create_task("generateChapter", target).await;
            assert_eq!(status, StatusCode::CREATED, "{writer}");
            f.repo()
                .bind_task_turn(
                    &f.project(),
                    writer["id"].as_str().unwrap(),
                    "generateChapter",
                    "writer-turn",
                )
                .unwrap();
            writer
        } else {
            f.scoped_writer().await
        };
        let mut payload = body_only_with_newline(&f);
        if edge == "legal-body" {
            payload["body"] = local_payload(&f)["body"].clone();
        }
        if edge == "valid-groups" {
            payload = local_payload(&f);
        }
        let mut arguments = f.arguments(&writer, payload, "uncertain-key");
        if edge == "delegated" {
            arguments["delegatedCandidateId"] = json!("isolated-candidate");
        }
        if edge == "wrong-cas-with-output" {
            arguments["payload"] = local_payload(&f);
            arguments["expectedCandidateRevisionId"] = json!("wrong-frozen-head");
        }
        let evidence = f.admit(arguments, "uncertain-call");
        if matches!(
            edge,
            "legal-body" | "valid-groups" | "delegated" | "wrong-cas-with-output"
        ) {
            let (status, saved) = f
                .save_exact(
                    &f.arguments(&writer, local_payload(&f), "own-valid-key"),
                    "own-valid-call",
                )
                .await;
            assert_eq!(status, StatusCode::CREATED, "{saved}");
            f.finish_writer(&writer).await;
        } else if edge != "running" {
            f.repo()
                .update_task(
                    &f.project(),
                    writer["id"].as_str().unwrap(),
                    "generateChapter",
                    TaskUpdate {
                        status: "failed".into(),
                        error: None,
                        result_summary: None,
                        now: 2,
                    },
                )
                .unwrap();
        }
        let db = rusqlite::Connection::open(f.path()).unwrap();
        match edge {
            "unsealed" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET capability_revoked_at=NULL WHERE task_id=?1",
                    [writer["id"].as_str().unwrap()],
                )
                .unwrap();
            }
            // Explicit current-schema contradiction; the public complete
            // contract cannot produce completed without a candidate output.
            "completed-no-output" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET status='completed' WHERE task_id=?1",
                    [writer["id"].as_str().unwrap()],
                )
                .unwrap();
            }
            "candidate-receipt" => {
                db.execute("INSERT INTO story_candidate_receipts_v2 VALUES (?1,'another-branch','uncertain-key','domain-digest','retained-revision','retained-snapshot',0)", [f.project().as_str()]).unwrap();
            }
            "commit-receipt" => {
                db.execute("INSERT INTO story_receipts_v2 VALUES (?1,'another-branch','uncertain-key','domain-digest','retained-commit','retained-snapshot','applied')", [f.project().as_str()]).unwrap();
            }
            _ => {}
        }
        drop(db);
        let before = f.facts_except_verdict("different-call");
        let original = f.exact(&evidence).await;
        assert_eq!(original["operation"]["state"], "accepted");
        assert_eq!(
            f.reconcile(AUTHOR, evidence).await,
            (StatusCode::OK, original),
            "{edge}"
        );
        assert_eq!(f.facts_except_verdict("different-call"), before, "{edge}");
    }
}

#[tokio::test]
async fn scoped_completed_alignment_proof_rejects_inexact_input_and_inconsistent_own_outputs() {
    let f = Fixture::new().await;
    let writer = f.scoped_writer().await;
    let evidence = f.admit(
        f.arguments(&writer, body_only_with_newline(&f), "original-key"),
        "original-call",
    );
    let (status, saved) = f
        .save_exact(
            &f.arguments(&writer, local_payload(&f), "valid-other-key"),
            "valid-other-call",
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    f.finish_writer(&writer).await;
    let before = f.facts_except_verdict("different-call");
    let mut variants = Vec::new();
    for (field, value) in [
        ("projectId", json!("foreign-project")),
        ("taskId", json!("foreign-task")),
        ("threadId", json!("foreign-thread")),
        ("expectedRevision", json!("0")),
        ("expectedCandidateRevisionId", json!(false)),
        ("idempotencyKey", json!("different-key")),
    ] {
        let mut variant = evidence.clone();
        variant["arguments"][field] = value;
        variants.push(variant);
    }
    for (field, value) in [
        ("callId", json!("different-call")),
        ("toolName", json!("save_brief")),
        ("requestHash", json!("f".repeat(64))),
    ] {
        let mut variant = evidence.clone();
        variant[field] = value;
        variants.push(variant);
    }
    let mut missing = evidence.clone();
    missing["arguments"]
        .as_object_mut()
        .unwrap()
        .remove("expectedCandidateRevisionId");
    variants.push(missing);
    let mut oversized = evidence.clone();
    oversized["arguments"]["payload"]["body"] = json!("x".repeat(129 * 1024));
    variants.push(oversized);
    for variant in variants {
        let (status, result) = f.reconcile(AUTHOR, variant).await;
        assert!(
            !status.is_success() || result["operation"].is_null(),
            "{result}"
        );
        assert_eq!(f.facts_except_verdict("different-call"), before);
        assert_eq!(f.exact(&evidence).await["operation"]["state"], "accepted");
    }
    // The later successful call is not enough. Internal output/refs must agree
    // before completed can participate in this call-local, immutable proof.
    let db = rusqlite::Connection::open(f.path()).unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET output_refs_json='[]' WHERE task_id=?1",
        [writer["id"].as_str().unwrap()],
    )
    .unwrap();
    drop(db);
    let before = f.facts_except_verdict("different-call");
    let (status, proof) = f.reconcile(AUTHOR, evidence.clone()).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operation"]["state"], "accepted");
    assert_eq!(f.exact(&evidence).await["operation"]["state"], "accepted");
    assert_eq!(f.facts_except_verdict("different-call"), before);

    for edge in ["captured-head-mismatch", "output-parent-mismatch"] {
        let f = Fixture::with_single_group(true).await;
        let writer = f.scoped_writer().await;
        let evidence = f.admit(
            f.arguments(&writer, body_only_with_newline(&f), "sealed-original-key"),
            "sealed-original-call",
        );
        let (status, saved) = f
            .save_exact(
                &f.arguments(&writer, local_payload(&f), "sealed-valid-other-key"),
                "sealed-valid-other-call",
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        f.finish_writer(&writer).await;
        // Isolated current-schema corruption fixtures exercise the two new
        // authority checks. The successful later save is never their proof.
        let db = rusqlite::Connection::open(f.path()).unwrap();
        if edge == "captured-head-mismatch" {
            assert_eq!(db.execute(
                "UPDATE story_task_source_heads_v2 SET working_selection_revision=COALESCE(working_selection_revision,0)+1 WHERE task_id=?1 AND object_id='draft-chapter-one'",
                [writer["id"].as_str().unwrap()],
            ).unwrap(), 1);
        } else {
            assert_eq!(
                db.execute(
                    "UPDATE story_revisions_v2 SET parent_revision_id=NULL WHERE revision_id=?1",
                    [saved["revisionId"].as_str().unwrap()],
                )
                .unwrap(),
                1
            );
        }
        drop(db);
        let before = f.facts_except_verdict("different-call");
        let (status, response) = f.reconcile(AUTHOR, evidence.clone()).await;
        if edge == "captured-head-mismatch" {
            assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        } else {
            assert_eq!(status, StatusCode::OK, "{response}");
            assert_eq!(response["operation"]["state"], "accepted");
        }
        assert_eq!(
            f.exact(&evidence).await["operation"]["state"],
            "accepted",
            "{edge}"
        );
        assert_eq!(f.facts_except_verdict("different-call"), before, "{edge}");
    }
}

#[tokio::test]
async fn scoped_storage_corruption_and_rejection_proof_failure_remain_unknown_without_partial_facts()
 {
    let f = Fixture::new().await;
    let writer = f.scoped_writer().await;
    let args = f.arguments(&writer, local_payload(&f), "real-storage-key");
    let evidence = f.evidence(args.clone(), "real-storage-call");
    let db = rusqlite::Connection::open(f.path()).unwrap();
    db.execute_batch("CREATE TRIGGER scoped_storage_fail BEFORE INSERT ON story_task_outputs_v2 WHEN NEW.output_kind='draft' BEGIN SELECT RAISE(ABORT,'real scoped output storage failure'); END;").unwrap();
    let workspace = f.workspace().await;
    let (status, error) = f.save_exact(&args, "real-storage-call").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    assert_eq!(f.workspace().await, workspace);
    assert_eq!(f.exact(&evidence).await["operation"]["state"], "accepted");
    db.execute_batch("DROP TRIGGER scoped_storage_fail;")
        .unwrap();
    f.repo()
        .update_task(
            &f.project(),
            writer["id"].as_str().unwrap(),
            "generateChapter",
            TaskUpdate {
                status: "failed".into(),
                error: None,
                result_summary: None,
                now: 2,
            },
        )
        .unwrap();
    let before = f.facts_except_verdict("different-call");
    let (status, proof) = f.reconcile(AUTHOR, evidence).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operation"]["state"], "accepted");
    assert_eq!(f.facts_except_verdict("different-call"), before);

    for edge in [
        "corrupt-base",
        "source-body-mismatch",
        "corrupt-output-refs",
        "proof-storage",
    ] {
        let f = Fixture::new().await;
        let writer = f.scoped_writer().await;
        let evidence = f.admit(
            f.arguments(&writer, body_only_with_newline(&f), "proof-key"),
            "proof-call",
        );
        f.repo()
            .update_task(
                &f.project(),
                writer["id"].as_str().unwrap(),
                "generateChapter",
                TaskUpdate {
                    status: "failed".into(),
                    error: None,
                    result_summary: None,
                    now: 2,
                },
            )
            .unwrap();
        let db = rusqlite::Connection::open(f.path()).unwrap();
        match edge {
            "corrupt-base" => {
                db.execute(
                    "UPDATE story_revisions_v2 SET content_json='{' WHERE revision_id=?1",
                    [f.original["revisionId"].as_str().unwrap()],
                )
                .unwrap();
            }
            "source-body-mismatch" => {
                assert_eq!(db.execute(
                    "UPDATE story_revisions_v2 SET content_json=json_set(content_json,'$.body','正文与场景分组真正不相等') WHERE revision_id=?1",
                    [f.original["revisionId"].as_str().unwrap()],
                ).unwrap(), 1);
            }
            "corrupt-output-refs" => {
                db.execute(
                    "UPDATE story_tasks_v2 SET output_refs_json='{}' WHERE task_id=?1",
                    [writer["id"].as_str().unwrap()],
                )
                .unwrap();
            }
            _ => {
                db.execute_batch("CREATE TRIGGER scoped_proof_fail BEFORE UPDATE OF state ON story_tool_operations_v1 WHEN OLD.call_id='proof-call' BEGIN SELECT RAISE(ABORT,'rejection proof write failure'); END;").unwrap();
            }
        }
        drop(db);
        let before = f.facts_except_verdict("different-call");
        let (status, _) = f.reconcile(AUTHOR, evidence.clone()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{edge}");
        assert_eq!(f.exact(&evidence).await["operation"]["state"], "accepted");
        assert_eq!(f.facts_except_verdict("different-call"), before, "{edge}");
    }
}

#[tokio::test]
async fn modeling_reports_evidence_shape_and_allows_corrected_submission() {
    let fixture = Fixture::new().await;
    let (_, batch) = call(
        &fixture.app,
        "POST",
        &format!("{}/modeling-batches", fixture.root),
        AUTHOR,
        Some(
            json!({"objectId":"draft-chapter-one","sourceRevisionId":fixture.original["revisionId"],
            "expectedRevision":0,"extractorVersion":"evidence-feedback"}),
        ),
    )
    .await;
    let (status, task) = fixture
        .create_task("modelKnowledge", json!({"batchId":batch["id"]}))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let uri = format!(
        "{}/modeling-batches/{}/publish",
        fixture.root,
        batch["id"].as_str().unwrap()
    );
    let mut payload = json!({"taskId":task["id"],"threadId":"modelKnowledge","expectedRevision":0,
        "entities":[{"id":"linxi","kind":"character","displayName":"林夕","aliases":[],
            "evidence":{"quote":"林夕","start":0,"end":2,"sourceRevisionId":fixture.original["revisionId"]}}],
        "assertions":[]});
    let (status, error) = call(&fixture.app, "POST", &uri, CANDIDATE, Some(payload.clone())).await;
    assert!(status.is_client_error());
    assert_eq!(
        error["error"],
        json!({"code":"invalid_source_evidence", "message":"entities[0].evidence: evidence_shape_invalid",
            "details":{"field":"entities[0].evidence","reason":"evidence_shape_invalid","publicationState":"rejected"},
            "recovery":{"nextAction":"correct_evidence","preserveValidEntries":true,"retryUnchanged":false,
                "instruction":"Evidence requires a nonempty verbatim quote of at most 2000 Unicode characters. Only quote and optional paired start/end fields are allowed."}})
    );
    payload["entities"][0]["evidence"]
        .as_object_mut()
        .unwrap()
        .remove("sourceRevisionId");
    let source = json!({"quote":"林夕走进港口。","start":0,"end":7});
    payload["entities"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"arrival","kind":"event","displayName":"林夕走进港口","aliases":[],"evidence":source}));
    payload["assertions"] = json!([{"kind":"event","subjectId":"arrival",
        "predicate":"arrives","storyTime":null,
        "value":{"participants":[{"role":"actor","entityId":"linxi"}],
            "preconditions":[],"effects":[],"untilExclusive":null}}]);
    let (status, missing_evidence) =
        call(&fixture.app, "POST", &uri, CANDIDATE, Some(payload.clone())).await;
    assert!(status.is_client_error(), "{missing_evidence}");
    assert!(
        missing_evidence["error"]["message"]
            .as_str()
            .unwrap()
            .contains("{quote,start,end}")
    );
    payload["assertions"][0]["evidence"] = source;
    payload["assertions"][0]["value"]
        .as_object_mut()
        .unwrap()
        .remove("preconditions");
    let (status, missing_links) =
        call(&fixture.app, "POST", &uri, CANDIDATE, Some(payload.clone())).await;
    assert!(status.is_client_error(), "{missing_links}");
    assert!(
        missing_links["error"]["message"]
            .as_str()
            .unwrap()
            .contains("participants, preconditions, effects and untilExclusive")
    );
    payload["assertions"][0]["value"]["preconditions"] = json!([]);
    let (status, published) =
        call(&fixture.app, "POST", &uri, CANDIDATE, Some(payload.clone())).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["status"], "ready");
    assert_eq!(published["publicationState"], "committed");
    assert_eq!(published["nextAction"], "complete_task");
    let (status, knowledge) = call(
        &fixture.app,
        "GET",
        &format!("{}/knowledge", fixture.root),
        AUTHOR,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(knowledge["sources"][0]["entities"], payload["entities"]);
}

#[tokio::test]
async fn every_writer_scope_preserves_author_locks_and_rejects_model_created_locks() {
    for scope in [
        json!({"type":"wholeChapter"}),
        json!({"type":"throughScene","endSceneOrder":3}),
    ] {
        let fixture = Fixture::new().await;
        let mut target = fixture.target.clone();
        target["sceneOrder"] = if scope["type"] == "throughScene" {
            json!(3)
        } else {
            json!(1)
        };
        target["scope"] = scope;
        let (status, task) = fixture.create_task("generateChapter", target).await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let before = fixture.workspace().await;
        for mutation in 0..7 {
            let mut output = fixture.original.clone();
            match mutation {
                0 => output["sceneGroups"][2]["body"] = json!("擅改锁定结尾"),
                1 => output["sceneGroups"][2]["locked"] = json!(false),
                2 => {
                    output["sceneGroups"].as_array_mut().unwrap().remove(2);
                }
                3 => output["sceneGroups"].as_array_mut().unwrap().swap(1, 2),
                4 => output["sceneGroups"][0]["locked"] = json!(true),
                5 => output["sceneGroups"][0]["id"] = json!("ending"),
                6 => {}
                _ => unreachable!(),
            }
            join_body(&mut output);
            if mutation == 6 {
                output["body"] = json!("聚合正文绕过锁定");
            }
            let (status, error) = fixture
                .save(
                    &task,
                    &fixture.original,
                    output,
                    &format!("forbidden-{mutation}"),
                )
                .await;
            assert!(status.is_client_error(), "{error}");
            assert_eq!(fixture.workspace().await, before);
        }
        let mut output = fixture.original.clone();
        output["sceneGroups"][1]["body"] = json!("她藏好密函。");
        join_body(&mut output);
        let (status, first) = fixture
            .save(&task, &fixture.original, output.clone(), "first")
            .await;
        assert_eq!(status, StatusCode::CREATED, "{first}");
        output["sceneGroups"][1]["body"] = json!("她藏好密函，走进白塔。");
        join_body(&mut output);
        let (status, second) = fixture.save(&task, &first, output.clone(), "second").await;
        assert_eq!(status, StatusCode::CREATED, "{second}");
        assert_eq!(second["sceneGroups"], output["sceneGroups"]);
    }
}

#[tokio::test]
async fn protected_scenes_follow_selected_working_revision_not_the_unselected_candidate() {
    for end in [None, Some(1)] {
        let fixture = Fixture::new().await;
        let mut newer = fixture.original.clone();
        newer["sceneGroups"][2]["body"] = json!("作者试写另一个结尾。");
        newer["sceneGroups"][2]["locked"] = json!(false);
        join_body(&mut newer);
        let (status, newer) = call(&fixture.app,"POST",&format!("{}/drafts/checkpoints",fixture.root),AUTHOR,
        Some(json!({"payload":newer,"expectedRevision":0,"expectedCandidateRevisionId":fixture.original["revisionId"],"idempotencyKey":"author-newer"}))).await;
        assert_eq!(status, StatusCode::CREATED, "{newer}");
        let (status, selected) = call(&fixture.app,"PUT",&format!("{}/working-baselines/draft",fixture.root),AUTHOR,
        Some(json!({"objectId":"chapter-one","revisionId":fixture.original["revisionId"],"expectedSelectionRevision":0}))).await;
        assert_eq!(status, StatusCode::OK, "{selected}");
        let mut target = fixture.target.clone();
        target["scope"] = end.map_or_else(
            || json!({"type":"wholeChapter"}),
            |end| json!({"type":"throughScene","endSceneOrder":end}),
        );
        let (status, task) = fixture.create_task("generateChapter", target).await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        if end.is_none() {
            let (status, rejected) = fixture
                .save(&task, &newer, newer.clone(), "wrong-base")
                .await;
            assert!(status.is_client_error(), "{rejected}");
        }
        let mut output = fixture.original.clone();
        output["sceneGroups"][0]["body"] = json!("林夕藏好密函走进港口。");
        join_body(&mut output);
        let mut visible = output.clone();
        if let Some(end) = end {
            visible["sceneGroups"].as_array_mut().unwrap().truncate(end);
            join_body(&mut visible);
        }
        let (status, saved) = fixture.save(&task, &newer, visible, "selected-base").await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        assert_eq!(
            fixture.workspace().await["chapters"][0]["candidate"]["sceneGroups"],
            output["sceneGroups"]
        );
        if end.is_some() {
            assert_eq!(saved["sceneGroups"].as_array().unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn conversation_local_revision_preserves_other_content_and_requests_modeling() {
    let fixture = Fixture::new().await;
    fixture
        .model(&fixture.original, "她烧掉密函。", "burned")
        .await;
    let (status, discussion) = fixture
        .create_task("advisory", json!({"type":"project"}))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{discussion}");
    let source = discussion["id"].as_str().unwrap();
    let actions = format!("{}/tasks/{source}/actions", fixture.root);
    let (status, plan) = call(&fixture.app,"POST",&format!("{}/tasks/{source}/work-plan",fixture.root),CANDIDATE,Some(json!({"taskId":source,"threadId":"advisory","expectedRevision":0,"expectedPlanVersion":0,"idempotencyKey":"scoped-execution-plan","plan":{"intentMode":"executeGoals","goals":[{"id":"local-revision","title":"完成作者批准的密函场景修订","acceptance":"烧信改为藏信，保留其他正文与锁定场景","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"实际改稿是否保留作者指定内容","checkScope":"选定场景与保护内容","approach":"局部修改后独立核对保存成果","assessment":"","sourceIds":[],"sourceChecks":[],"evidence":[],"continuation":null}]}}))).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    let mut writer_target = fixture.target.clone();
    writer_target["chapterAcceptance"] = json!({"requirements":[{"id":"whole-chapter","text":"密函场景改为藏信，开场与锁定结尾保留"}],"length":{"requirement":"作者未规定改稿字符数","minCharacters":null,"maxCharacters":null}});
    let (status, action) = call(
        &fixture.app,
        "POST",
        &actions,
        CANDIDATE,
        Some(json!({
        "taskId":source,"threadId":"advisory","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"local",
        "operation":"generateChapter","target":writer_target,"instruction":"只改密函场景为藏信"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(action["target"], writer_target);
    let (status, result) = call(
        &fixture.app,
        "POST",
        &format!("{}/tasks/{source}/complete", fixture.root),
        AUTHOR,
        Some(json!({"threadId":"advisory","summary":"已登记局部改稿"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    settle_fixture::settle(
        &fixture._directory.path().join("scenes.sqlite3"),
        fixture.root.trim_start_matches("/api/projects/"),
        source,
    );
    let (status, approved) = call(
        &fixture.app,
        "POST",
        &format!("{actions}/{}/approve", action["id"].as_str().unwrap()),
        AUTHOR,
        Some(json!({"expectedIntentRevision":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let mut target = action["target"].clone();
    target["requestedByTaskId"] = json!(source);
    target["requestedActionId"] = action["id"].clone();
    let (status, task) = fixture.create_task("generateChapter", target).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let uri = format!(
        "{}/context?taskId={task_id}&threadId=generateChapter&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000",
        fixture.root
    );
    let (status, context) = call(&fixture.app, "GET", &uri, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let relevant_text = read_context_source(&fixture, &task, &context, "/relevantText").await;
    assert_eq!(
        relevant_text[0]["content"]["sceneGroups"],
        fixture.original["sceneGroups"]
    );
    let before = fixture.workspace().await;
    let mut payload = fixture.original.clone();
    payload["sceneGroups"][1]["body"] = json!("她将密函藏进衣襟。");
    join_body(&mut payload);
    for index in 0..5 {
        let mut invalid = payload.clone();
        match index {
            0 => invalid["sceneGroups"][0]["body"] = json!("擅自改变开场。"),
            1 => invalid["sceneGroups"][1]["sceneId"] = json!("different-plan"),
            2 => invalid["sceneGroups"].as_array_mut().unwrap().swap(0, 1),
            3 => invalid["title"] = json!("擅自修改标题"),
            4 => invalid["sceneGroups"][2]["locked"] = json!(false),
            _ => unreachable!(),
        }
        join_body(&mut invalid);
        let (status, error) = fixture
            .save(
                &task,
                &fixture.original,
                invalid,
                &format!("invalid-{index}"),
            )
            .await;
        assert!(status.is_client_error(), "{error}");
        assert_eq!(fixture.workspace().await, before);
    }
    let (status, saved) = fixture
        .save(&task, &fixture.original, payload.clone(), "valid")
        .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["sceneGroups"], payload["sceneGroups"]);
    payload["sceneGroups"][1]["body"] = json!("她将密函藏进衣襟，转身离开。");
    join_body(&mut payload);
    let (status, final_draft) = fixture
        .save(&task, &saved, payload.clone(), "checkpoint-two")
        .await;
    assert_eq!(status, StatusCode::CREATED, "{final_draft}");
    let (status, complete) = call(
        &fixture.app,
        "POST",
        &format!("{}/tasks/{task_id}/complete", fixture.root),
        CANDIDATE,
        Some(json!({"threadId":"generateChapter","summary":"已生成局部改稿候选"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{complete}");
    let (status, followup) = call(
        &fixture.app,
        "GET",
        &format!("{}/chapters/chapter-one/next-step", fixture.root),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{followup}");
    assert_eq!(
        followup,
        json!({"action":{"kind":"modelKnowledge","chapterId":"chapter-one",
        "sourceRevisionId":final_draft["revisionId"],"sourceTaskId":task_id}})
    );
    let workspace = fixture.workspace().await;
    assert_eq!(
        workspace["chapters"][0]["candidate"]["sceneGroups"],
        payload["sceneGroups"]
    );
    assert_eq!(workspace["chapters"][0]["accepted"], Value::Null);
    let state_uri = format!(
        "{}/knowledge/state?perspective=world&entityId=letter&predicate=action",
        fixture.root
    );
    let (status, invalidated) = call(&fixture.app, "GET", &state_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{invalidated}");
    assert_eq!(invalidated["entries"], json!([]));
    fixture
        .model(&final_draft, "她将密函藏进衣襟，转身离开。", "hidden")
        .await;
    let (status, updated) = call(&fixture.app, "GET", &state_uri, AUTHOR, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let entries = updated["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{updated}");
    assert_eq!(entries[0]["sourceRevisionId"], final_draft["revisionId"]);
    assert_eq!(entries[0]["assertion"]["value"], json!("hidden"));
    let (status, followup) = call(
        &fixture.app,
        "GET",
        &format!("{}/chapters/chapter-one/next-step", fixture.root),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{followup}");
    assert_eq!(followup, json!({"action":null}));
}

#[tokio::test]
async fn local_revision_rejects_unknown_locked_duplicate_and_stale_selections() {
    let fixture = Fixture::new().await;
    for ids in [
        json!([]),
        json!(["missing"]),
        json!(["ending"]),
        json!(["letter", "letter"]),
    ] {
        let mut target = fixture.target.clone();
        target["scope"]["groupIds"] = ids;
        let (status, error) = fixture.create_task("generateChapter", target).await;
        assert!(status.is_client_error(), "{error}");
    }
    let mut target = fixture.target.clone();
    target["scope"]["sourceRevisionId"] = json!("unknown-revision");
    let (status, error) = fixture.create_task("generateChapter", target).await;
    assert!(status.is_client_error(), "{error}");
    let (status, task) = fixture
        .create_task("generateChapter", fixture.target.clone())
        .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let (status, discussion) = fixture
        .create_task("advisory", json!({"type":"project"}))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{discussion}");
    let mut edited = fixture.original.clone();
    edited["sceneGroups"][0]["body"] = json!("作者更改开场。");
    join_body(&mut edited);
    let (status,saved) = call(&fixture.app,"POST",&format!("{}/drafts/checkpoints",fixture.root),AUTHOR,
        Some(json!({"payload":edited,"expectedRevision":0,"expectedCandidateRevisionId":fixture.original["revisionId"],"idempotencyKey":"external"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let before = fixture.workspace().await;
    let (status, error) = fixture
        .save(&task, &saved, fixture.original.clone(), "late")
        .await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(fixture.workspace().await, before);
    let (status, error) = fixture
        .create_task("generateChapter", fixture.target.clone())
        .await;
    assert!(status.is_client_error(), "{error}");
    let mut changed_target = fixture.target.clone();
    changed_target["scope"]["sourceRevisionId"] = saved["revisionId"].clone();
    let actions = format!(
        "{}/tasks/{}/actions",
        fixture.root,
        discussion["id"].as_str().unwrap()
    );
    let (status,error) = call(&fixture.app,"POST",&actions,CANDIDATE,Some(json!({
        "taskId":discussion["id"],"threadId":"advisory","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"stale-discussion",
        "operation":"generateChapter","target":changed_target,"instruction":"不能借新版本绕过旧对话输入"}))).await;
    assert!(status.is_client_error(), "{error}");
    assert_eq!(
        call(&fixture.app, "GET", &actions, AUTHOR, /*body*/ None)
            .await
            .1,
        json!({"actions":[]})
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;

use novel_codex_story_core::{RecoveryOperationsRepository, TasksRepository};
