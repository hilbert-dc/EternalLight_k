use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
            Some(json!({"title":"港城"})),
        )
        .await;
        let root = format!(
            "/api/projects/{}",
            created["project"]["id"].as_str().unwrap()
        );
        let payload = json!({"chapterId":"chapter-one","chapterNumber":1,"title":"密函",
            "body":"林夕走进港口。\n\n---\n\n她烧掉密函。\n\n---\n\n白塔熄灭。",
            "sceneGroups":[
                {"id":"opening","sceneId":"plan-opening","body":"林夕走进港口。","locked":false},
                {"id":"letter","sceneId":"plan-letter","body":"她烧掉密函。","locked":false},
                {"id":"ending","sceneId":null,"body":"白塔熄灭。","locked":true}]});
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
            "scope":{"type":"selectedScenes","sourceRevisionId":original["revisionId"],"groupIds":["letter"]}});
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
    assert_eq!(context["taskSources"]["readTool"], "read_task_source");
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
    let (status, action) = call(
        &fixture.app,
        "POST",
        &actions,
        CANDIDATE,
        Some(json!({
        "taskId":source,"threadId":"advisory","expectedRevision":0,"expectedIntentRevision":0,"idempotencyKey":"local",
        "operation":"generateChapter","target":fixture.target,"instruction":"只改密函场景为藏信"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(action["target"], fixture.target);
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
