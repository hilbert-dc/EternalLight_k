use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

const AUTHOR: &str = "evidence-author-token-1234567890123456";
const CANDIDATE: &str = "evidence-candidate-token-123456789012";
const CAPABILITY: &str = "evidence-capability-token-1234567890";
const BODY: &str = "前🙂句。\n\n---\n\n灯亮。灯亮。灯亮。灯亮。灯亮。\n后句。";

async fn call(
    app: &Router,
    uri: &str,
    actor: &str,
    body: Value,
    caller: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if !caller.is_empty() {
        let tool = if uri.ends_with("/source-evidence") {
            "resolve_story_evidence"
        } else {
            "submit_commitment_extraction"
        };
        request = request
            .header("x-novel-tool-name", tool)
            .header("x-novel-root-thread", "evidence-thread")
            .header("x-novel-caller-thread", caller)
            .header("x-novel-turn", "evidence-turn")
            .header(
                "x-novel-tool-call",
                body["idempotencyKey"].as_str().unwrap_or("resolve-call"),
            )
            .header(
                "x-novel-request-hash",
                format!("{:x}", Sha256::digest(body.to_string().as_bytes())),
            );
    }
    let response = app
        .clone()
        .author_fixture_request(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"message":String::from_utf8_lossy(&bytes)})),
    )
}

async fn setup(operation: &str) -> (tempfile::TempDir, std::path::PathBuf, Router, Value) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse("evidence-project").unwrap();
    let branch = BranchId::parse("main").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "合成锚点作品".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch.clone(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    for chapter in 1..=2 {
        repo.save_candidate(SaveCandidateRequest {
            project_id:project.clone(),branch_id:branch.clone(),object_id:ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap(),object_kind:"draft".into(),
            revision_id:RevisionId::parse(format!("source-{chapter}")).unwrap(),commit_id:CommitId::parse(format!("save-{chapter}")).unwrap(),
            idempotency_key:IdempotencyKey::parse(format!("save-{chapter}")).unwrap(),request_digest:BODY.into(),content_digest:BODY.into(),
            content_json:json!({"chapterId":format!("chapter-{chapter}"),"chapterNumber":chapter,"title":"锚点","body":BODY,"sceneGroups":[
                {"id":"first","body":"前🙂句。","locked":false},{"id":"last","body":"灯亮。灯亮。灯亮。灯亮。灯亮。\n后句。","locked":false}]}).to_string(),
            expected_project_revision:0,expected_snapshot_id:repo.branch_head(&project,&branch).unwrap().unwrap().snapshot_id,
            next_snapshot_id:SnapshotId::parse(format!("snapshot-{chapter}")).unwrap(),expected_candidate_revision_id:None,
            chapter_projection:Some(ChapterProjection {chapter_id:format!("chapter-{chapter}"),number:chapter,title:"锚点".into()}),task_output:None,linked_impact_ids:vec![],
        }).unwrap();
    }
    drop(repo);
    let app = novel_codex_story_runtime::router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let target = if operation == "advisory" {
        json!({"type":"project"})
    } else {
        json!({"chapterId":"chapter-1","chapterNumber":1,"sourceRevisionId":"source-1"})
    };
    let (status,task) = call(&app,"/api/projects/evidence-project/tasks",AUTHOR,
        json!({"operation":operation,"inputRevision":0,"threadId":"evidence-thread","target":target,"authorIntent":"提取本章伏笔，不发布或确认",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires}),"").await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    (directory, path, app, task)
}

fn resolve_request(quote: &str) -> Value {
    json!({"threadId":"evidence-thread","expectedRevision":0,"sourceObjectId":"draft-chapter-1","sourceRevisionId":"source-1","quote":quote})
}

#[tokio::test]
async fn exact_unicode_evidence_handles_duplicate_text_without_repair_or_write() {
    let (_directory, path, app, task) = setup("extractCommitments").await;
    let uri = format!(
        "/api/projects/evidence-project/tasks/{}/source-evidence",
        task["id"].as_str().unwrap()
    );
    let start = BODY[..BODY.find("后句。").unwrap()].chars().count();
    let expected = json!({"status":"unique","source":{"objectId":"draft-chapter-1","revisionId":"source-1","sourcePath":"/body","unicodeLength":BODY.chars().count()},
        "quote":"后句。","matches":[{"start":start,"end":start+3}],"truncated":false,"authority":"exactSourceText"});
    assert_eq!(
        call(
            &app,
            &uri,
            CANDIDATE,
            resolve_request("后句。"),
            "evidence-thread"
        )
        .await,
        (StatusCode::OK, expected.clone())
    );
    let (_, missing) = call(
        &app,
        &uri,
        CANDIDATE,
        resolve_request("后匀。"),
        "evidence-thread",
    )
    .await;
    assert_eq!(
        missing,
        json!({"status":"notFound","source":expected["source"],"quote":"后匀。","matches":[],"truncated":false,"authority":"exactSourceText"})
    );
    let (_, ambiguous) = call(
        &app,
        &uri,
        CANDIDATE,
        resolve_request("灯亮。"),
        "evidence-thread",
    )
    .await;
    let first = BODY[..BODY.find("灯亮。").unwrap()].chars().count();
    assert_eq!(
        ambiguous,
        json!({"status":"ambiguous","source":expected["source"],"quote":"灯亮。",
        "matches":[{"start":first,"end":first+3},{"start":first+3,"end":first+6},{"start":first+6,"end":first+9},{"start":first+9,"end":first+12}],"truncated":true,"authority":"exactSourceText"})
    );
    let escaped_quote = "\0".repeat(2_000);
    let escaped = json!({"status":"notFound","source":expected["source"],"quote":escaped_quote,"matches":[],"truncated":false,"authority":"exactSourceText"});
    let envelope = json!({"content":[{"type":"text","text":escaped.to_string()}]});
    let model_item = json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":envelope.to_string()});
    let count = eternallight_engine_core::dispatch("tokens.count", json!(model_item.to_string()))
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(
        count <= 9_000,
        "legal escaped quote must fit its delivered model item: {count}"
    );
    let (status, budget) = call(
        &app,
        &uri,
        CANDIDATE,
        resolve_request(&escaped_quote),
        "evidence-thread",
    )
    .await;
    assert_eq!((status, budget), (StatusCode::OK, escaped));
    let save_uri = "/api/projects/evidence-project/chapters/chapter-1/commitment-extractions";
    let payload = json!({"chapterId":"chapter-1","sourceRevisionId":"source-1","examinedRanges":[{"start":0,"end":BODY.chars().count()}],
        "candidates":[{"id":"promise","kind":"foreshadowing","summary":"合成待兑现线索","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
            "setupEvidenceRefs":[{"objectId":expected["source"]["objectId"],"revisionId":expected["source"]["revisionId"],"quote":expected["quote"],"start":start,"end":start+3}],
            "fulfillmentCriteria":"后续原文解释","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null}]});
    let mut save = json!({"taskId":task["id"],"threadId":"evidence-thread","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"bad-anchor","payload":payload});
    save["payload"]["candidates"][0]["setupEvidenceRefs"][0]["start"] = json!(0);
    assert!(
        !call(&app, save_uri, CANDIDATE, save.clone(), "evidence-thread")
            .await
            .0
            .is_success()
    );
    assert!(
        SqliteStoryRepository::open(&path)
            .unwrap()
            .commitment_extraction(
                &ProjectId::parse("evidence-project").unwrap(),
                "chapter-1",
                None
            )
            .unwrap()
            .is_none()
    );
    save["payload"]["candidates"][0]["setupEvidenceRefs"][0]["start"] =
        expected["matches"][0]["start"].clone();
    save["idempotencyKey"] = json!("canonical-anchor");
    let (status, receipt) = call(&app, save_uri, CANDIDATE, save, "evidence-thread").await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
    assert_eq!(receipt["candidateCount"], json!(1));
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id='evidence-project'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn evidence_reads_reject_foreign_sources_expired_inputs_and_delegated_callers() {
    let (_directory, path, app, task) = setup("extractCommitments").await;
    let uri = format!(
        "/api/projects/evidence-project/tasks/{}/source-evidence",
        task["id"].as_str().unwrap()
    );
    for (field, value) in [
        ("sourceObjectId", json!("draft-chapter-2")),
        ("sourceRevisionId", json!("foreign")),
        ("expectedRevision", json!(1)),
        ("quote", json!("字".repeat(2001))),
        ("snapshotId", json!("snapshot-1")),
    ] {
        let mut request = resolve_request("后句。");
        request[field] = value;
        assert!(
            !call(&app, &uri, CANDIDATE, request, "evidence-thread")
                .await
                .0
                .is_success()
        );
    }
    let mut foreign = resolve_request("后句。");
    foreign["sourceObjectId"] = json!("draft-chapter-2");
    foreign["sourceRevisionId"] = json!("source-2");
    assert!(
        !call(&app, &uri, CANDIDATE, foreign, "evidence-thread")
            .await
            .0
            .is_success()
    );
    for caller in ["", "child"] {
        assert!(
            !call(&app, &uri, CANDIDATE, resolve_request("后句。"), caller)
                .await
                .0
                .is_success()
        );
    }
    assert!(
        !call(
            &app,
            &uri.replace("evidence-project", "another-project"),
            CANDIDATE,
            resolve_request("后句。"),
            "evidence-thread"
        )
        .await
        .0
        .is_success()
    );
    rusqlite::Connection::open(path).unwrap().execute("UPDATE story_workspaces_v2 SET revision=revision+1 WHERE project_id='evidence-project'",[]).unwrap();
    assert!(
        !call(
            &app,
            &uri,
            CANDIDATE,
            resolve_request("后句。"),
            "evidence-thread"
        )
        .await
        .0
        .is_success()
    );
}

use novel_codex_story_core::{CommitmentExtractionRepository, ProjectRevisionRepository};
