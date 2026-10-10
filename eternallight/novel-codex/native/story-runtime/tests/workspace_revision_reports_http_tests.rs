//! Actual report saves, real successor Attempts, and task-owned full source reads.
//! No SQL-created output, root identity, revision or authorization is used here.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
#[path = "revision_outcome/fixture.rs"]
mod fixture;
#[path = "revision_outcome/flow.rs"]
mod flow;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
const AUTHOR: &str = fixture::AUTHOR;
const CANDIDATE: &str = fixture::CANDIDATE;
const CAPABILITY: &str = "outcome-capability-123456789012345678";
const FUTURE: &str = "第二章尚未开放的秘密🙂";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if method == "GET"
        && actor == CANDIDATE
        && (uri.contains("/task-source?") || uri.contains("/task-source-content?"))
    {
        let thread = uri
            .split("threadId=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap();
        request = request
            .header(
                "x-novel-tool-name",
                if uri.contains("/task-source-content?") {
                    "read_source_content"
                } else {
                    "read_task_source"
                },
            )
            .header("x-novel-root-thread", thread)
            .header("x-novel-caller-thread", thread)
            .header("x-novel-turn", "workspace-report-read")
            .header("x-novel-tool-call", "workspace-report-source");
    }
    if method == "POST" && actor == CANDIDATE && uri.ends_with("/commitment-extractions") {
        let thread = body["threadId"].as_str().unwrap();
        request = request
            .header("x-novel-tool-name", "submit_commitment_extraction")
            .header("x-novel-root-thread", thread)
            .header("x-novel-caller-thread", thread)
            .header("x-novel-turn", "workspace-extraction-save")
            .header(
                "x-novel-tool-call",
                format!("workspace-{}", body["idempotencyKey"].as_str().unwrap()),
            )
            .header(
                "x-novel-request-hash",
                format!("{:x}", Sha256::digest(body.to_string().as_bytes())),
            );
    }
    let response = app
        .clone()
        .author_fixture_request(
            request
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
    let raw = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&raw).unwrap_or_else(|e| {
        panic!(
            "{method} {uri}: {status}: {e}: {}",
            String::from_utf8_lossy(&raw)
        )
    });
    (status, value)
}
async fn ok(app: &Router, method: &str, uri: &str, actor: &str, body: Value) -> Value {
    let (s, p) = call(app, method, uri, actor, body).await;
    assert!(s.is_success(), "{method} {uri}: {s}: {p}");
    p
}
async fn setup() -> (tempfile::TempDir, Router, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = novel_codex_story_runtime::router(task_fixture::state(
        dir.path().join("reports.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let project = ok(&app, "POST", "/api/projects", AUTHOR, json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"报告的实际保存与回读"})).await;
    (
        dir,
        app,
        format!(
            "/api/projects/{}",
            project["project"]["id"].as_str().unwrap()
        ),
    )
}
async fn workspace(app: &Router, base: &str) -> Value {
    ok(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        Value::Null,
    )
    .await
}
fn task_request(op: &str, target: Value, thread: &str, revision: Value) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"operation":op,"target":target,"inputRevision":revision,"threadId":thread,
        "authorIntent":"在现有范围保存候选报告并核对真实原文；不采用、不发布。",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600})
}
async fn task(app: &Router, base: &str, op: &str, target: Value, thread: &str) -> Value {
    let current = workspace(app, base).await;
    ok(
        app,
        "POST",
        &format!("{base}/tasks"),
        AUTHOR,
        task_request(op, target, thread, current["revision"].clone()),
    )
    .await
}
fn binding(t: &Value) -> String {
    format!(
        "threadId={}&expectedRevision={}",
        t["threadId"].as_str().unwrap(),
        t["inputRevision"]
    )
}
async fn context(app: &Router, base: &str, t: &Value) -> Value {
    let p = ok(
        app,
        "GET",
        &format!(
            "{base}/context?taskId={}&threadId={}&projectRevision={}&purpose={}&byteBudget=60000",
            t["id"].as_str().unwrap(),
            t["threadId"].as_str().unwrap(),
            t["inputRevision"],
            t["purpose"].as_str().unwrap()
        ),
        CANDIDATE,
        Value::Null,
    )
    .await;
    bounded(&p);
    p
}
fn bounded(p: &Value) {
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":json!({"content":[{"type":"text","text":p.to_string()}]}).to_string()});
    assert!(
        eternallight_engine_core::dispatch("tokens.count", json!(item.to_string()))
            .unwrap()
            .as_u64()
            .unwrap()
            <= 9000
    );
}
async fn catalogue(app: &Router, base: &str, t: &Value, pack: &Value, filter: &str) -> Vec<Value> {
    let mut all = Vec::new();
    let mut cursor = String::new();
    loop {
        let p = ok(
            app,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source-content?{}&sourceId={}&view=workspace{filter}{cursor}",
                t["id"].as_str().unwrap(),
                binding(t),
                pack["taskWorkspace"]["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert!(p.to_string().len() <= 6500);
        bounded(&p);
        assert_eq!(p["contentRead"], false);
        all.extend(p["items"].as_array().unwrap().clone());
        if let Some(next) = p["nextCursor"].as_str() {
            let next = format!("&cursor={next}");
            assert_ne!(cursor, next);
            cursor = next;
        } else {
            return all;
        }
    }
}
async fn raw(app: &Router, base: &str, t: &Value, source: &str) -> (String, Value, String) {
    let mut text = String::new();
    let mut cursor = String::new();
    let mut hash = None;
    let mut path = None;
    loop {
        let p = ok(
            app,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source?{}&sourceId={source}{cursor}",
                t["id"].as_str().unwrap(),
                binding(t)
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert!(p.to_string().len() <= 6500);
        bounded(&p);
        assert_eq!(p["sourceId"], source);
        assert_eq!(p["byteRange"]["start"], text.len());
        let part = p["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        text.push_str(part);
        assert_eq!(p["byteRange"]["end"], text.len());
        assert_eq!(
            hash.get_or_insert_with(|| p["contentHash"].clone())
                .as_str()
                .unwrap(),
            p["contentHash"].as_str().unwrap()
        );
        assert_eq!(
            path.get_or_insert_with(|| p["sectionPath"].clone())
                .as_str()
                .unwrap(),
            p["sectionPath"].as_str().unwrap()
        );
        if let Some(next) = p["nextCursor"].as_str() {
            assert!(!part.is_empty());
            assert_eq!(p["endOfInputFromCursor"], false);
            let next = format!("&cursor={next}");
            assert_ne!(cursor, next);
            cursor = next;
        } else {
            assert_eq!(p["endOfInputFromCursor"], true);
            assert_eq!(p["byteRange"]["total"], text.len());
            break;
        }
    }
    let hash = hash.unwrap().as_str().unwrap().to_owned();
    assert_eq!(hash, format!("{:x}", Sha256::digest(text.as_bytes())));
    let parsed = serde_json::from_str(&text).unwrap();
    (text, parsed, hash)
}
fn set_field(value: &mut Value, pointer: &str, field: Value) {
    let parts: Vec<String> = pointer.strip_prefix('/').map_or_else(Vec::new, |p| {
        p.split('/')
            .map(|s| s.replace("~1", "/").replace("~0", "~"))
            .collect()
    });
    fn put(v: &mut Value, parts: &[String], field: Value) {
        let Some((part, rest)) = parts.split_first() else {
            *v = field;
            return;
        };
        if v.is_null() {
            *v = if part.parse::<usize>().is_ok() {
                json!([])
            } else {
                json!({})
            };
        }
        match v {
            Value::Array(a) => {
                let n = part.parse::<usize>().unwrap();
                if a.len() <= n {
                    a.resize(n + 1, Value::Null);
                }
                put(&mut a[n], rest, field);
            }
            Value::Object(o) => put(o.entry(part.clone()).or_insert(Value::Null), rest, field),
            _ => panic!("non-container source field {parts:?}"),
        }
    }
    put(value, &parts, field);
}
async fn parsed(app: &Router, base: &str, t: &Value, source: &str, hash: &str) -> Value {
    let mut result = Value::Null;
    let mut cursor = String::new();
    let mut chunks = BTreeMap::<String, (usize, usize, String)>::new();
    let mut from = 0;
    let mut total = None;
    loop {
        let p = ok(
            app,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source-content?{}&sourceId={source}{cursor}",
                t["id"].as_str().unwrap(),
                binding(t)
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert!(p.to_string().len() <= 6500);
        bounded(&p);
        assert_eq!(p["sourceId"], source);
        assert_eq!(p["contentHash"], hash);
        assert_eq!(p["representation"], "parsedFields");
        assert_eq!(p["contentRead"], true);
        assert_eq!(p["coverage"]["fromEntry"], from);
        let entries = p["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        from += entries.len();
        assert_eq!(p["coverage"]["toEntry"], from);
        let count = p["coverage"]["totalEntries"].as_u64().unwrap() as usize;
        assert_eq!(*total.get_or_insert(count), count);
        assert!(from <= count);
        for e in entries {
            let pointer = e["path"].as_str().unwrap();
            if let Some(s) = e["text"].as_str() {
                let range = &e["characterRange"];
                let start = range["start"].as_u64().unwrap() as usize;
                let end = range["end"].as_u64().unwrap() as usize;
                let n = range["total"].as_u64().unwrap() as usize;
                let chunk = chunks
                    .entry(pointer.into())
                    .or_insert((0, n, String::new()));
                assert_eq!((start, n), (chunk.0, chunk.1));
                assert_eq!(end, start + s.chars().count());
                assert!(end <= n);
                chunk.0 = end;
                chunk.2.push_str(s);
            } else {
                assert!(e.get("value").is_some());
                set_field(&mut result, pointer, e["value"].clone());
            }
        }
        if let Some(next) = p["nextCursor"].as_str() {
            assert!(from < count);
            let next = format!("&cursor={next}");
            assert_ne!(cursor, next);
            cursor = next;
        } else {
            assert_eq!(from, count);
            break;
        }
    }
    for (pointer, (end, n, s)) in chunks {
        assert_eq!(end, n);
        set_field(&mut result, &pointer, json!(s));
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(result.to_string().as_bytes())),
        hash
    );
    result
}
async fn full(app: &Router, base: &str, t: &Value, source: &str) -> (String, Value, String) {
    let (text, v, hash) = raw(app, base, t, source).await;
    assert_eq!(parsed(app, base, t, source, &hash).await, v);
    (text, v, hash)
}
async fn frozen_draft(
    app: &Router,
    base: &str,
    t: &Value,
    chapter: &str,
    revision: &Value,
    body: &str,
) -> (String, Value, String) {
    let pack = context(app, base, t).await;
    let items = catalogue(app, base, t, &pack, "&objectKind=draft").await;
    let item = items
        .iter()
        .find(|i| {
            i["objectId"] == format!("draft-{chapter}")
                && i["revisionId"] == *revision
                && i["purpose"] != "savedCheckpoint"
        })
        .unwrap_or_else(|| panic!("{items:?}"));
    let got = full(app, base, t, item["sourceId"].as_str().unwrap()).await;
    assert_eq!(got.1["sourceRevisionId"], *revision);
    assert_eq!(got.1["content"]["body"], body);
    got
}
async fn successor(app: &Router, base: &str, path: &Path, producer: &Value) -> (Value, Value) {
    let finished=ok(app,"POST",&format!("{base}/tasks/{}/complete",producer["id"].as_str().unwrap()),CANDIDATE,json!({"threadId":producer["threadId"],"expectedRevision":producer["inputRevision"],"summary":"报告已真实保存，交后续模型核对。"})).await;
    assert_eq!(finished["status"], "completed");
    settle_fixture::settle(
        path,
        base.trim_start_matches("/api/projects/"),
        producer["id"].as_str().unwrap(),
    );
    let work = ok(
        app,
        "GET",
        &format!(
            "{base}/tasks/{}/work-task",
            producer["id"].as_str().unwrap()
        ),
        AUTHOR,
        Value::Null,
    )
    .await;
    let before = work.clone();
    let authorization_uri = format!(
        "{base}/work-tasks/{}/budget-authorization",
        work["id"].as_str().unwrap()
    );
    let authorization = ok(app, "GET", &authorization_uri, AUTHOR, Value::Null).await;
    assert_eq!(authorization["authorization"]["maxRequests"], 16);
    assert_eq!(authorization["authorization"]["maxTokens"], 200000);
    assert_eq!(
        before["steps"][0]["attempts"][0]["threadId"],
        before["threadId"]
    );
    let source_step = before["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["currentAttemptId"] == producer["id"])
        .unwrap();
    assert_eq!(source_step["status"], "completed");
    assert!(
        source_step["outputRefs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == producer["requiredOutputKind"] && r["revisionId"].is_string())
    );
    let ws = workspace(app, base).await;
    let next=ok(app,"POST",&format!("{base}/work-tasks/{}/decision",work["id"].as_str().unwrap()),AUTHOR,json!({"expectedWorkVersion":work["version"],"expectedRevision":ws["revision"],"expectedSourceDigest":ws["sourceDigest"]})).await;
    assert_eq!(next["stopReason"], Value::Null, "{next}");
    assert_eq!(next["workTaskId"], work["id"]);
    assert_eq!(next["threadId"], work["threadId"]);
    let mut target = next["step"]["target"].clone();
    target["resumeStepId"] = next["step"]["id"].clone();
    let mut request = task_request(
        "advisory",
        target,
        next["threadId"].as_str().unwrap(),
        next["step"]["inputRevision"].clone(),
    );
    request["authorIntent"] = producer["inputManifest"]["authorIntent"].clone();
    let reader = ok(app, "POST", &format!("{base}/tasks"), AUTHOR, request).await;
    let current = ok(
        app,
        "GET",
        &format!("{base}/tasks/{}/work-task", reader["id"].as_str().unwrap()),
        AUTHOR,
        Value::Null,
    )
    .await;
    assert_eq!(current["id"], before["id"]);
    assert_eq!(current["threadId"], before["threadId"]);
    assert_eq!(
        ok(app, "GET", &authorization_uri, AUTHOR, Value::Null).await,
        authorization
    );
    assert_eq!(
        current["steps"][0]["attempts"][0]["threadId"],
        current["threadId"]
    );
    let reader_step = current["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["currentAttemptId"] == reader["id"])
        .unwrap();
    assert_eq!(reader_step["id"], next["step"]["id"]);
    assert_eq!(
        reader["inputManifest"]["authorIntent"],
        producer["inputManifest"]["authorIntent"]
    );
    assert_ne!(reader["id"], producer["id"]);
    assert_eq!(reader["threadId"], before["threadId"]);
    (reader, current)
}
async fn report(
    app: &Router,
    base: &str,
    reader: &Value,
    producer: &Value,
    kind: &str,
    object: &str,
    revision: &Value,
    expected: &Value,
) -> Value {
    let pack = context(app, base, reader).await;
    let items = catalogue(app, base, reader, &pack, &format!("&objectKind={kind}")).await;
    let item = items
        .iter()
        .find(|i| i["producerTaskId"] == producer["id"] && i["revisionId"] == *revision)
        .unwrap_or_else(|| panic!("missing saved {kind}: {items:?}"));
    assert_eq!(item["objectId"], object);
    assert_eq!(item["outputKind"], kind);
    assert_eq!(item["outputOrdinal"], 1);
    assert_eq!(item["purpose"], "savedReport");
    assert_eq!(item["authority"], "nativeSavedCheckpoint");
    assert_eq!(item["producerStatusAtListing"], "completed");
    assert_eq!(
        item["version"],
        json!({"revisionId":revision,"outputOrdinal":1})
    );
    assert_eq!(item["contentRead"], false);
    assert_eq!(item["contentHash"], Value::Null);
    let (text, got, _) = full(app, base, reader, item["sourceId"].as_str().unwrap()).await;
    assert!(
        text.len() > 2000,
        "every report exercises multiple actual raw pages"
    );
    assert_eq!(got["sourceRevisionId"], *revision);
    assert_eq!(got["producerTaskId"], producer["id"]);
    assert_eq!(got["outputOrdinal"], 1);
    assert_eq!(got["purpose"], "savedReport");
    assert_eq!(got["authority"], "nativeSavedCheckpoint");
    assert_eq!(got["content"], *expected);
    assert!(got.get("isCurrentAtListing").is_none());
    item.clone()
}
async fn foreign_denied(app: &Router, base: &str, item: &Value) {
    let foreign = task(
        app,
        base,
        "advisory",
        json!({"type":"project"}),
        "foreign-reader",
    )
    .await;
    let pack = context(app, base, &foreign).await;
    assert!(
        !catalogue(app, base, &foreign, &pack, "")
            .await
            .iter()
            .any(|i| i["producerTaskId"] == item["producerTaskId"])
    );
    for route in ["task-source", "task-source-content"] {
        let (s, p) = call(
            app,
            "GET",
            &format!(
                "{base}/tasks/{}/{route}?{}&sourceId={}",
                foreign["id"].as_str().unwrap(),
                binding(&foreign),
                item["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            Value::Null,
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT, "{p}");
        assert_eq!(p["error"]["code"], "task_source_not_in_catalog");
    }
}
async fn author_draft(app: &Router, base: &str, chapter: &str, n: i64, body: &str) -> Value {
    let ws = workspace(app, base).await;
    ok(app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"payload":{"chapterId":chapter,"chapterNumber":n,"title":format!("第{n}章"),"body":body,"completeness":"chapterComplete"},"expectedRevision":ws["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":format!("original-{chapter}")})).await
}

#[tokio::test]
async fn real_revision_outcome_is_readable_by_later_same_work_attempt() {
    let f = flow::Flow::queued().await;
    let producer = f.assessment().await;
    let pack = context(&f.fixture.app, &f.fixture.root, &producer).await;
    let initial = catalogue(&f.fixture.app, &f.fixture.root, &producer, &pack, "").await;
    let before = initial
        .iter()
        .find(|i| i["purpose"] == "beforeDraft")
        .unwrap();
    let original = full(
        &f.fixture.app,
        &f.fixture.root,
        &producer,
        before["sourceId"].as_str().unwrap(),
    )
    .await;
    assert_eq!(
        original.1["sourceRevisionId"],
        f.fixture.original["revisionId"]
    );
    assert_eq!(original.1["content"]["body"], f.fixture.original["body"]);
    let mut expected = f.report();
    expected["outcomes"][0]["reason"] = json!("改稿原文明确藏信，依据实际正文。🙂\n".repeat(45));
    let receipt = ok(
        &f.fixture.app,
        "POST",
        &format!("{}{}", f.fixture.root, f.path()),
        CANDIDATE,
        f.submission(&producer, expected.clone(), "workspace-assessment"),
    )
    .await;
    assert_eq!(
        full(
            &f.fixture.app,
            &f.fixture.root,
            &producer,
            before["sourceId"].as_str().unwrap()
        )
        .await,
        original
    );
    let (reader, _) = successor(
        &f.fixture.app,
        &f.fixture.root,
        &f.fixture.path(),
        &producer,
    )
    .await;
    let item = report(
        &f.fixture.app,
        &f.fixture.root,
        &reader,
        &producer,
        "revisionOutcome",
        &format!("revision-outcome-{}", f.writer["id"].as_str().unwrap()),
        &receipt["revisionId"],
        &expected,
    )
    .await;
    // A report does not grant this ordinary Work a new writer action. The
    // later same-Work writer report exclusion is proved after the real model
    // repair choice in support/authoring_selected_chapters.rs, where the
    // original generation Run already supplies that execution authority.
    foreign_denied(&f.fixture.app, &f.fixture.root, &item).await;
}

#[tokio::test]
async fn real_scene_assessment_preserves_source_scope_then_later_same_work_reader_reads_full_report()
 {
    let (dir, app, base) = setup().await;
    let plan=ok(&app,"POST",&format!("{base}/artifacts/plan"),AUTHOR,json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"plan","payload":{"chapters":[{"chapterPlanId":"planned-one","chapterNumber":1,"goal":"密函","scenes":[{"id":"scene-one","goal":"收到密函","beats":[],"expectedEffects":[{"id":"effect-one","dimension":"plot","description":"发现秘密","commitmentRefs":[]}]}]}]}})).await;
    let review=ok(&app,"POST",&format!("{base}/reviews"),AUTHOR,json!({"artifactKind":"plan","revisionId":plan["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"review"})).await;
    ok(
        &app,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        AUTHOR,
        json!({"expectedRevision":0,"selections":{},"idempotencyKey":"accept"}),
    )
    .await;
    let ws = workspace(&app, &base).await;
    let materialized=ok(&app,"POST",&format!("{base}/chapters/materialize-from-plan"),AUTHOR,json!({"chapterPlanId":"planned-one","expectedRevision":ws["revision"],"idempotencyKey":"materialize"})).await;
    let chapter = materialized["chapterId"].as_str().unwrap();
    let ws = workspace(&app, &base).await;
    let body = "林夕拾起密函。🌊";
    let draft=ok(&app,"POST",&format!("{base}/drafts/checkpoints"),AUTHOR,json!({"expectedRevision":ws["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":"original-one","payload":{"chapterId":chapter,"chapterNumber":1,"title":"密函","body":body,"completeness":"chapterComplete","sourcePlanRefs":{"chapterPlanId":"planned-one","planRevisionId":plan["revisionId"],"localOnly":false,"lineageStatus":"current","outlineRevisionId":null}}})).await;
    author_draft(&app, &base, "future-two", 2, FUTURE).await;
    let ws = workspace(&app, &base).await;
    let batch=ok(&app,"POST",&format!("{base}/modeling-batches"),AUTHOR,json!({"objectId":format!("draft-{chapter}"),"sourceRevisionId":draft["revisionId"],"expectedRevision":ws["revision"],"extractorVersion":"workspace-scene"})).await;
    let model = task(
        &app,
        &base,
        "modelKnowledge",
        json!({"batchId":batch["id"]}),
        "scene-model",
    )
    .await;
    ok(&app,"POST",&format!("{base}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()),CANDIDATE,json!({"taskId":model["id"],"threadId":model["threadId"],"expectedRevision":model["inputRevision"],"entities":[{"id":"letter-event","kind":"event","displayName":"拾起密函","evidence":{"quote":"密函"}},{"id":"linxi","kind":"character","displayName":"林夕","evidence":{"quote":"林夕"}}],"assertions":[{"kind":"event","subjectId":"letter-event","predicate":"pickedUp","storyTime":null,"value":{"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null},"evidence":{"quote":"密函"}}]})).await;
    let completed = ok(
        &app,
        "POST",
        &format!("{base}/tasks/{}/complete", model["id"].as_str().unwrap()),
        CANDIDATE,
        json!({"threadId":model["threadId"],"summary":"原文事件建模，没有规划场景归属。"}),
    )
    .await;
    assert_eq!(completed["status"], "completed");
    settle_fixture::settle(
        &dir.path().join("reports.sqlite3"),
        base.trim_start_matches("/api/projects/"),
        model["id"].as_str().unwrap(),
    );
    let ws = workspace(&app, &base).await;
    let target = json!({"chapterId":chapter,"chapterNumber":1,"draftRevisionId":draft["revisionId"],"planRevisionId":plan["revisionId"],"knowledgeBatchId":batch["id"],"inputSnapshotId":ws["headSnapshotId"],"intentGeneration":0});
    let producer = task(&app, &base, "assessSceneOutcome", target, "scene-report").await;
    let frozen = frozen_draft(&app, &base, &producer, chapter, &draft["revisionId"], body).await;
    let pack = context(&app, &base, &producer).await;
    let items = catalogue(&app, &base, &producer, &pack, "&objectKind=draft").await;
    assert!(items.iter().all(|i| i["chapterNumber"] == 1));
    assert!(!pack.to_string().contains(FUTURE));
    assert!(!frozen.0.contains(FUTURE));
    let reason = "原文没有绑定规划场景，不能借用整章事件。🙂\n".repeat(35);
    let payload = json!({"chapterId":chapter,"planRevisionId":plan["revisionId"],"draftRevisionId":draft["revisionId"],"knowledgeBatchId":batch["id"],"inputSnapshotId":ws["headSnapshotId"],"intentGeneration":0,"assessments":[{"sceneId":"scene-one","verdict":"insufficientEvidence","rationale":reason,"recommendation":"repairDraft","effectAssessments":[{"effectId":"effect-one","verdict":"insufficientEvidence","rationale":reason,"actualEventRefs":[],"commitmentAssessments":[]}]}]});
    let receipt=ok(&app,"POST",&format!("{base}/chapters/{chapter}/scene-assessments"),CANDIDATE,json!({"taskId":producer["id"],"threadId":producer["threadId"],"expectedRevision":producer["inputRevision"],"expectedCandidateRevisionId":null,"idempotencyKey":"saved-scene","payload":payload})).await;
    assert_eq!(
        frozen_draft(&app, &base, &producer, chapter, &draft["revisionId"], body).await,
        frozen
    );
    let (reader, _) = successor(&app, &base, &dir.path().join("reports.sqlite3"), &producer).await;
    let item = report(
        &app,
        &base,
        &reader,
        &producer,
        "sceneAssessment",
        &format!("scene-assessment-{chapter}"),
        &receipt["revisionId"],
        &payload,
    )
    .await;
    foreign_denied(&app, &base, &item).await;
}

#[tokio::test]
async fn real_commitment_extraction_keeps_original_source_and_scoped_future_exclusion_before_successor_read()
 {
    let (dir, app, base) = setup().await;
    let body = "白塔的灯将再亮。";
    let draft = author_draft(&app, &base, "chapter-one", 1, body).await;
    author_draft(&app, &base, "chapter-two", 2, FUTURE).await;
    let producer = task(
        &app,
        &base,
        "extractCommitments",
        json!({"chapterId":"chapter-one","chapterNumber":1,"sourceRevisionId":draft["revisionId"]}),
        "extraction-report",
    )
    .await;
    let frozen = frozen_draft(
        &app,
        &base,
        &producer,
        "chapter-one",
        &draft["revisionId"],
        body,
    )
    .await;
    let pack = context(&app, &base, &producer).await;
    let entry = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["sectionPath"] == "/relevantText")
        .unwrap();
    let original = full(&app, &base, &producer, entry["sourceId"].as_str().unwrap()).await;
    assert_eq!(original.1[0]["sourceRevisionId"], draft["revisionId"]);
    assert!(!original.0.contains(FUTURE));
    assert!(
        catalogue(&app, &base, &producer, &pack, "&objectKind=draft")
            .await
            .iter()
            .all(|i| i["chapterNumber"] == 1)
    );
    let candidate = json!({"id":"lamp","kind":"foreshadowing","summary":"灯会再亮","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,"setupEvidenceRefs":[{"objectId":"draft-chapter-one","revisionId":draft["revisionId"],"quote":"白塔的灯","start":0,"end":4}],"fulfillmentCriteria":"后续正文出现再次亮灯的实际证据。🙂\n".repeat(40),"targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null});
    let payload = json!({"chapterId":"chapter-one","sourceRevisionId":draft["revisionId"],"examinedRanges":[{"start":0,"end":body.chars().count()}],"candidates":[candidate]});
    let receipt=ok(&app,"POST",&format!("{base}/chapters/chapter-one/commitment-extractions"),CANDIDATE,json!({"taskId":producer["id"],"threadId":producer["threadId"],"expectedRevision":producer["inputRevision"],"expectedCandidateRevisionId":null,"idempotencyKey":"saved-extraction","payload":payload})).await;
    assert_eq!(
        full(&app, &base, &producer, entry["sourceId"].as_str().unwrap()).await,
        original
    );
    assert_eq!(
        frozen_draft(
            &app,
            &base,
            &producer,
            "chapter-one",
            &draft["revisionId"],
            body
        )
        .await,
        frozen
    );
    let (reader, _) = successor(&app, &base, &dir.path().join("reports.sqlite3"), &producer).await;
    let item = report(
        &app,
        &base,
        &reader,
        &producer,
        "commitmentExtraction",
        "commitment-extraction-chapter-one",
        &receipt["revisionId"],
        &json!({"payload":payload}),
    )
    .await;
    foreign_denied(&app, &base, &item).await;
}

#[tokio::test]
async fn reachable_revise_commitment_saves_real_candidate_then_same_work_reader_reads_exact_origin_and_body()
 {
    let (dir, app, base) = setup().await;
    let draft = author_draft(&app, &base, "chapter-one", 1, "灯将熄灭。灯火归来。").await;
    let resource = format!("{base}/commitments/lamp");
    let mut payload = json!({"id":"lamp","kind":"suspense","summary":"灯能否归来","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,"setupEvidenceRefs":[{"objectId":"draft-chapter-one","revisionId":draft["revisionId"],"quote":"灯将熄灭","start":0,"end":4}],"fulfillmentCriteria":"后文实际正文应明确灯火归来。🙂\n".repeat(45),"targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null});
    let ws = workspace(&app, &base).await;
    let initial=ok(&app,"POST",&resource,AUTHOR,json!({"payload":payload,"extractionOrigin":null,"expectedRevision":ws["revision"],"baseSnapshotId":ws["headSnapshotId"],"expectedCandidateRevisionId":null,"idempotencyKey":"author-commitment"})).await;
    let producer = task(
        &app,
        &base,
        "reviseCommitment",
        json!({"type":"commitment","commitmentId":"lamp","revisionId":initial["revisionId"]}),
        "commitment-report",
    )
    .await;
    assert_eq!(producer["requiredOutputKind"], "commitment");
    let input_uri = format!(
        "{resource}/task-input?taskId={}&{}",
        producer["id"].as_str().unwrap(),
        binding(&producer)
    );
    let original = ok(&app, "GET", &input_uri, CANDIDATE, Value::Null).await;
    assert_eq!(original["revisionId"], initial["revisionId"]);
    assert_eq!(original["payload"], payload);
    // Initial commitment bases remain on the actual domain task-input API;
    // only real TaskOutput commitments enter the workspace saved-output branch.
    context(&app, &base, &producer).await;
    payload["status"] = json!("fulfilled");
    payload["fulfillmentEvidenceRefs"] = json!([{"objectId":"draft-chapter-one","revisionId":draft["revisionId"],"quote":"灯火归来","start":5,"end":9}]);
    let receipt=ok(&app,"POST",&format!("{resource}/task-candidate"),CANDIDATE,json!({"taskId":producer["id"],"threadId":producer["threadId"],"expectedRevision":producer["inputRevision"],"payload":payload,"expectedCandidateRevisionId":initial["revisionId"],"idempotencyKey":"saved-commitment"})).await;
    assert_eq!(
        ok(&app, "GET", &input_uri, CANDIDATE, Value::Null).await,
        original
    );
    let (reader, _) = successor(&app, &base, &dir.path().join("reports.sqlite3"), &producer).await;
    let item = report(
        &app,
        &base,
        &reader,
        &producer,
        "commitment",
        "commitment-lamp",
        &receipt["revisionId"],
        &json!({"payload":payload,"extractionOrigin":null}),
    )
    .await;
    foreign_denied(&app, &base, &item).await;
}
