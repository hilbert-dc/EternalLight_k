use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
const AUTHOR: &str = "hypothesis-author-token-123456789012345";
const CANDIDATE: &str = "hypothesis-candidate-token-12345678901";
const CAP: &str = "hypothesis-capability-token-123456789";

async fn call(
    app: &Router,
    uri: &str,
    actor: &str,
    body: Value,
    caller: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {actor}"))
        .header("x-novel-task-capability", CAP)
        .header("content-type", "application/json");
    if !caller.is_empty() {
        builder = builder
            .header("x-novel-tool-name", "simulate_story_hypothesis")
            .header("x-novel-root-thread", "hypothesis-thread")
            .header("x-novel-caller-thread", caller);
    }
    let response = app
        .clone()
        .author_fixture_request(builder.body(Body::from(body.to_string())).unwrap())
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
async fn setup(source_quote: &str) -> (tempfile::TempDir, std::path::PathBuf, Router, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hypothesis.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let project = ProjectId::parse("hypothesis-story").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let snapshot = SnapshotId::parse("saved-baseline").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "合成反事实".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch,
        snapshot.clone(),
    )
    .unwrap();
    // Persist a finite historical baseline. The HTTP tool still performs real task/capability/engine validation.
    let db = rusqlite::Connection::open(&path).unwrap();
    for i in 0..18 {
        let body = format!("{source_quote}第{i}次现身。🙂");
        let object = format!("draft-chapter-{i}");
        let revision = format!("revision-{i}");
        let batch = format!("batch-{i}");
        let evidence = json!({"quote":body,"start":0,"end":body.chars().count()});
        db.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES (?1,'hypothesis-story',?2,'draft','digest',?3)",params![revision,object,json!({"chapterNumber":i+1,"title":"合成","body":body}).to_string()]).unwrap();
        db.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,accepted_revision_id) VALUES ('hypothesis-story',?1,'draft',?2)",params![object,revision]).unwrap();
        db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) VALUES ('saved-baseline',?1,?2)",params![object,revision]).unwrap();
        db.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,entities_json,assertions_json,created_at,published_at) VALUES (?1,'hypothesis-story',?2,?3,0,'test','ready',?4,?5,1,2)",params![batch,object,revision,json!([{"id":"master","kind":"character","displayName":"师父","aliases":[],"evidence":evidence}]).to_string(),json!([{"kind":"assertion","subjectId":"master","predicate":"alive","value":true,"storyTime":1,"evidence":evidence}]).to_string()]).unwrap();
        db.execute("UPDATE story_snapshot_knowledge_v2 SET batch_id=?2 WHERE snapshot_id='saved-baseline' AND object_id=?1",params![object,batch]).unwrap();
        db.execute("INSERT INTO story_entity_claims_v2(project_id,batch_id,entity_id,kind,display_name,aliases_json,evidence_json) VALUES ('hypothesis-story',?1,'master','character','师父','[]',?2)",params![batch,evidence.to_string()]).unwrap();
    }
    drop(db);
    drop(repo);
    let app = novel_codex_story_runtime::router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let id = create_task(&app, "advisory", json!({"type":"project"})).await;
    (dir, path, app, id)
}
async fn create_task(app: &Router, operation: &str, target: Value) -> String {
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status,task)=call(app,"/api/projects/hypothesis-story/tasks",AUTHOR,json!({"operation":operation,"inputRevision":0,"threadId":"hypothesis-thread","target":target,
        "authorIntent":"假设师父已经死去，检查已有证据冲突，不改变正文与正式设定","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),"capabilityExpiresAt":expires}),"").await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    task["id"].as_str().unwrap().to_owned()
}
fn base() -> Value {
    json!({"threadId":"hypothesis-thread","expectedRevision":0,"action":"readBaseline","entityId":"master","limit":8})
}
fn simulation(token: &Value, key: &str) -> Value {
    json!({"threadId":"hypothesis-thread","expectedRevision":0,"action":"simulate","baselineToken":token,"intent":"如果师父已经死去","logicalKey":key,
    "assumptions":[{"kind":"assumeClaim","subjectId":"master","predicate":"alive","value":false,"storyTime":1}]})
}
fn story_state(path: &std::path::Path) -> Value {
    let db = rusqlite::Connection::open(path).unwrap();
    let mut result = serde_json::Map::new();
    for table in [
        "story_revisions_v2",
        "story_snapshot_members_v2",
        "story_modeling_batches_v2",
        "story_object_heads_v2",
        "story_facts_v2",
        "story_acceptances_v2",
        "story_work_budgets_v1",
    ] {
        // Formal domains are checked by complete rows, not merely equal counts.
        let mut statement = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
            .unwrap();
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                Ok((0..count)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap();
        result.insert(
            table.into(),
            json!(rows.collect::<Result<Vec<_>, _>>().unwrap()),
        );
    }
    result.insert("head".into(),json!(db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id='hypothesis-story'",[],|r|r.get::<_,String>(0)).unwrap()));
    result.insert(
        "revision".into(),
        json!(
            db.query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id='hypothesis-story'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
        ),
    );
    Value::Object(result)
}

#[tokio::test]
async fn fixed_baseline_pages_simulate_idempotently_without_story_writes_or_budget_reset() {
    let (_dir, path, app, id) = setup("师父").await;
    let uri = format!("/api/projects/hypothesis-story/tasks/{id}/hypothesis");
    let before = story_state(&path);
    let (status, first) = call(&app, &uri, CANDIDATE, base(), "hypothesis-thread").await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(
        (
            first["entries"].as_array().unwrap().len(),
            first["truncated"].clone()
        ),
        (8, json!(true))
    );
    let mut missing = base();
    missing["entityId"] = json!("unpublished-person");
    let (status, unpublished) = call(&app, &uri, CANDIDATE, missing, "hypothesis-thread").await;
    assert_eq!(
        (status, unpublished["entries"].clone()),
        (StatusCode::OK, json!([]))
    );
    assert!(
        !call(
            &app,
            &uri,
            CANDIDATE,
            simulation(&unpublished["baselineToken"], "no-baseline"),
            "hypothesis-thread"
        )
        .await
        .0
        .is_success()
    );
    let mut next = base();
    next["cursor"] = first["nextCursor"].clone();
    let (_, second) = call(&app, &uri, CANDIDATE, next, "hypothesis-thread").await;
    assert_ne!(first["entries"][0], second["entries"][0]);
    let (status, result) = call(
        &app,
        &uri,
        CANDIDATE,
        simulation(&first["baselineToken"], "analysis-1"),
        "hypothesis-thread",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["authority"], json!("hypothesisOnly"));
    assert_eq!(
        result["simulation"]["assumedEntries"][0]["confirmationState"],
        json!("assumed")
    );
    assert_eq!(
        result["simulation"]["coverage"]["globalImpactComplete"],
        json!(false)
    );
    assert!(result["simulation"]["findingCount"].as_u64().unwrap() > 0);
    let (_, replay) = call(
        &app,
        &uri,
        CANDIDATE,
        simulation(&first["baselineToken"], "analysis-1"),
        "hypothesis-thread",
    )
    .await;
    assert_eq!(replay["receipt"]["id"], result["receipt"]["id"]);
    assert_eq!(replay["receipt"]["replayed"], json!(true));
    let mut conflict = simulation(&first["baselineToken"], "analysis-1");
    conflict["assumptions"][0]["value"] = json!(true);
    assert!(
        !call(&app, &uri, CANDIDATE, conflict, "hypothesis-thread")
            .await
            .0
            .is_success()
    );
    let mut foreign = simulation(&first["baselineToken"], "unknown");
    foreign["assumptions"][0]["subjectId"] = json!("unpublished-person");
    assert!(
        !call(&app, &uri, CANDIDATE, foreign, "hypothesis-thread")
            .await
            .0
            .is_success()
    );
    let mut suppression = simulation(&first["baselineToken"], "suppress");
    suppression["assumptions"] = json!([{"kind":"suppressClaim","modelingBatchId":first["entries"][0]["modelingBatchId"],"sourceRevisionId":first["entries"][0]["sourceRevisionId"],"assertionIndex":first["entries"][0]["assertionIndex"]}]);
    let (_, suppressed) = call(&app, &uri, CANDIDATE, suppression, "hypothesis-thread").await;
    assert_eq!(
        suppressed["suppressedEvidence"],
        json!([first["entries"][0]])
    );
    for i in 2..16 {
        assert_eq!(
            call(
                &app,
                &uri,
                CANDIDATE,
                simulation(&first["baselineToken"], &format!("analysis-{i}")),
                "hypothesis-thread"
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert!(
        !call(
            &app,
            &uri,
            CANDIDATE,
            simulation(&first["baselineToken"], "limit-over"),
            "hypothesis-thread"
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        call(
            &app,
            &uri,
            CANDIDATE,
            simulation(&first["baselineToken"], "analysis-1"),
            "hypothesis-thread"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(story_state(&path), before);
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_hypotheses_v1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        16
    );
    db.execute("UPDATE story_tasks_v2 SET output_refs_json='[{\"kind\":\"hypothesis\",\"revisionId\":\"fake-model-report\"}]' WHERE task_id=?1",[&id]).unwrap();
    let repo = SqliteStoryRepository::open(&path).unwrap();
    let work = repo
        .work_task_for_attempt(&ProjectId::parse("hypothesis-story").unwrap(), &id)
        .unwrap();
    let mut delivered = work
        .delivery_report
        .evidence
        .iter()
        .filter(|item| item.kind == "hypothesis")
        .map(|item| item.reference_id.clone().unwrap())
        .collect::<Vec<_>>();
    delivered.sort();
    let mut statement = db
        .prepare("SELECT hypothesis_id FROM story_hypotheses_v1 ORDER BY hypothesis_id")
        .unwrap();
    let persisted = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(delivered, persisted);
    assert!(
        work.delivery_report
            .evidence
            .iter()
            .filter(|item| item.kind == "hypothesis")
            .all(|item| item.state == WorkEvidenceState::Recorded)
    );
}

#[tokio::test]
async fn baseline_tokens_reject_wrong_owner_scope_expiry_and_stale_inputs() {
    let (_dir, path, app, id) = setup("师父").await;
    let uri = format!("/api/projects/hypothesis-story/tasks/{id}/hypothesis");
    let (_, first) = call(&app, &uri, CANDIDATE, base(), "hypothesis-thread").await;
    for caller in ["", "child"] {
        assert!(
            !call(&app, &uri, CANDIDATE, base(), caller)
                .await
                .0
                .is_success()
        );
    }
    for (field, value) in [
        ("snapshotId", json!("foreign")),
        ("hypothesisId", json!("foreign")),
        ("limit", json!(17)),
    ] {
        let mut invalid = base();
        invalid[field] = value;
        assert!(
            !call(&app, &uri, CANDIDATE, invalid, "hypothesis-thread")
                .await
                .0
                .is_success()
        );
    }
    let mut cursor = base();
    cursor["cursor"] = first["nextCursor"].clone();
    cursor["entityId"] = json!("other");
    assert!(
        !call(&app, &uri, CANDIDATE, cursor, "hypothesis-thread")
            .await
            .0
            .is_success()
    );
    let id2 = create_task(&app, "advisory", json!({"type":"project"})).await;
    let uri2 = uri.replace(&id, &id2);
    assert!(
        !call(
            &app,
            &uri2,
            CANDIDATE,
            simulation(&first["baselineToken"], "cross-task"),
            "hypothesis-thread"
        )
        .await
        .0
        .is_success()
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET operation='generateChapter' WHERE task_id=?1",
        [&id2],
    )
    .unwrap();
    assert!(
        !call(&app, &uri2, CANDIDATE, base(), "hypothesis-thread")
            .await
            .0
            .is_success()
    );
    db.execute(
        "UPDATE story_tasks_v2 SET operation='advisory' WHERE task_id=?1",
        [&id2],
    )
    .unwrap();
    drop(db);
    assert!(
        !call(
            &app,
            &uri.replace("hypothesis-story", "other-project"),
            CANDIDATE,
            base(),
            "hypothesis-thread"
        )
        .await
        .0
        .is_success()
    );
    let intents = format!("/api/projects/hypothesis-story/tasks/{id}/intents");
    let (status, pending) = call(
        &app,
        &intents,
        AUTHOR,
        json!({"expectedIntentRevision":0,"authorText":"改为只检查历史，不写正式设定"}),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    assert!(
        !call(&app, &uri, CANDIDATE, base(), "hypothesis-thread")
            .await
            .0
            .is_success()
    );
    let delivered = format!("{intents}/1/delivered");
    assert_eq!(
        call(&app, &delivered, AUTHOR, Value::Null, "").await.0,
        StatusCode::OK
    );
    assert!(
        !call(
            &app,
            &uri,
            CANDIDATE,
            simulation(&first["baselineToken"], "old-intent"),
            "hypothesis-thread"
        )
        .await
        .0
        .is_success()
    );
    let (status, new_baseline) = call(&app, &uri, CANDIDATE, base(), "hypothesis-thread").await;
    assert_eq!(
        (status, new_baseline["intentRevision"].clone()),
        (StatusCode::OK, json!(1))
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET capability_expires_at=0 WHERE task_id=?1",
        [&id2],
    )
    .unwrap();
    assert!(
        !call(&app, &uri2, CANDIDATE, base(), "hypothesis-thread")
            .await
            .0
            .is_success()
    );
    db.execute(
        "UPDATE story_workspaces_v2 SET revision=1 WHERE project_id='hypothesis-story'",
        [],
    )
    .unwrap();
    assert!(
        !call(
            &app,
            &uri,
            CANDIDATE,
            simulation(&first["baselineToken"], "late"),
            "hypothesis-thread"
        )
        .await
        .0
        .is_success()
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_hypotheses_v1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn formerly_token_limited_report_keeps_one_receipt_without_story_changes() {
    let (_dir, path, app, id) = setup(&"\0".repeat(500)).await;
    let uri = format!("/api/projects/hypothesis-story/tasks/{id}/hypothesis");
    let before = story_state(&path);
    let mut small = base();
    small["limit"] = json!(1);
    let (status, baseline) = call(&app, &uri, CANDIDATE, small, "hypothesis-thread").await;
    assert_eq!(status, StatusCode::OK, "{baseline}");
    let request = simulation(&baseline["baselineToken"], "large-report");
    let (status, error) = call(&app, &uri, CANDIDATE, request.clone(), "hypothesis-thread").await;
    assert_eq!(status, StatusCode::OK, "{error}");
    assert!(error.get("error").is_none());
    let db = rusqlite::Connection::open(&path).unwrap();
    let id: String = db
        .query_row("SELECT hypothesis_id FROM story_hypotheses_v1", [], |row| {
            row.get(0)
        })
        .unwrap();
    let (status, repeated) =
        call(&app, &uri, CANDIDATE, request.clone(), "hypothesis-thread").await;
    assert_eq!(status, StatusCode::OK, "{repeated}");
    assert_eq!(repeated["receipt"]["id"], id);
    assert_eq!(repeated["receipt"]["replayed"], true);
    let mut preview = request;
    preview["limit"] = json!(1);
    let (status, read) = call(&app, &uri, CANDIDATE, preview, "hypothesis-thread").await;
    assert_eq!(
        (
            status,
            read["receipt"]["id"].clone(),
            read["receipt"]["replayed"].clone()
        ),
        (StatusCode::OK, json!(id), json!(true)),
        "{read}"
    );
    assert_eq!(
        (
            read["simulation"]["findings"].as_array().unwrap().len(),
            read["simulation"]["findingsTruncated"].clone()
        ),
        (1, json!(true))
    );
    assert_eq!(
        db.query_row("SELECT hypothesis_id FROM story_hypotheses_v1", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap(),
        id
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_hypotheses_v1", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(story_state(&path), before);
}

use novel_codex_story_core::{ProjectRevisionRepository, WorkTasksRepository};
