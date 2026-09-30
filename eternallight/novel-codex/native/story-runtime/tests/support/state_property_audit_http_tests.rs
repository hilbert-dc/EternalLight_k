use super::{Fixture, Role, call};
use axum::http::StatusCode;
use novel_codex_story_runtime::knowledge_audit_tick;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn idle_worker_builds_long_source_proof_without_an_author_start_request() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = Fixture::new(directory.path().join("automatic-source-audit.sqlite3")).await;
    let claim = fixture.claim(
        "alive",
        "single",
        json!({"type":"boolean"}),
        json!(true),
        json!(1),
    );
    let (status, published) = fixture.publish(vec![claim]).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    // Scale the fixture with historical copies of a genuinely published source.
    // They remain bound to exact revisions and are audited, not mocked as certified.
    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    for index in 0..200 {
        connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) SELECT ?1,project_id,object_id,object_kind,content_digest,content_json FROM story_revisions_v2 WHERE revision_id=?2",rusqlite::params![format!("historical-revision-{index}"),fixture.draft["revisionId"].as_str().unwrap()]).unwrap();
        connection.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at,published_at) SELECT ?1,project_id,object_id,?2,input_project_revision,extractor_version,'stale',assertions_json,entities_json,created_at,published_at FROM story_modeling_batches_v2 WHERE batch_id=?3",rusqlite::params![format!("historical-batch-{index}"),format!("historical-revision-{index}"),fixture.batch["id"].as_str().unwrap()]).unwrap();
    }
    drop(connection);
    let project = novel_codex_story_core::ProjectId::parse(&fixture.project).unwrap();
    let mut repository =
        novel_codex_story_core::SqliteStoryRepository::open(&fixture.path).unwrap();
    repository.rebuild_entity_index(&project).unwrap();
    repository.rebuild_assertion_index(&project).unwrap();
    drop(repository);
    let base = format!("/api/projects/{}/knowledge-audits", fixture.project);
    let (_, before) = call(&fixture.path, "GET", &base, Value::Null, Role::Author).await;
    assert_eq!(before["job"], Value::Null);
    let queued = knowledge_audit_tick(&fixture.path).unwrap().unwrap();
    assert_eq!(
        (queued.status.as_str(), queued.source_count),
        ("queued", 201)
    );
    let mut completed = None;
    for _ in 0..16 {
        let progress = knowledge_audit_tick(&fixture.path).unwrap().unwrap();
        if progress.status == "completed" {
            completed = Some(progress);
            break;
        }
        assert_eq!(progress.status, "running");
    }
    assert_eq!(completed.unwrap().verified_consistent, Some(true));
    assert_eq!(knowledge_audit_tick(&fixture.path).unwrap(), None);
    let (status, visible) = call(&fixture.path, "GET", &base, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{visible}");
    assert_eq!(
        (
            &visible["job"]["verifiedConsistent"],
            &visible["job"]["audit"]["assertions"]["publishedBatches"]
        ),
        (&json!(true), &json!(201))
    );
}

#[tokio::test]
async fn audit_worker_rejects_damaged_source_and_author_sees_failure_not_success() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = Fixture::new(directory.path().join("source-audit.sqlite3")).await;
    let claim = fixture.claim(
        "alive",
        "single",
        json!({"type":"boolean"}),
        json!(true),
        json!(1),
    );
    let (status, published) = fixture.publish(vec![claim]).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let base = format!("/api/projects/{}/knowledge-audits", fixture.project);
    let (status, _) = call(&fixture.path, "POST", &base, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::CREATED);
    let completed = knowledge_audit_tick(&fixture.path).unwrap().unwrap();
    assert_eq!(
        (completed.status.as_str(), completed.verified_consistent),
        ("completed", Some(true))
    );
    rusqlite::Connection::open(&fixture.path).unwrap().execute(
        "UPDATE story_revisions_v2 SET content_json='{\"body\":\"不同正文\"}' WHERE revision_id=?1",
        [fixture.draft["revisionId"].as_str().unwrap()],
    ).unwrap();
    let (status, old) = call(
        &fixture.path,
        "GET",
        &format!("{base}/{}", completed.id),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{old}");
    assert_eq!(
        (&old["status"], &old["verifiedConsistent"]),
        (&json!("stale"), &Value::Null)
    );
    let (status, started) = call(&fixture.path, "POST", &base, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let failed = knowledge_audit_tick(&fixture.path).unwrap().unwrap();
    assert_eq!(
        (
            failed.status.as_str(),
            failed.verified_consistent,
            failed.audit.assertions.published_batches
        ),
        ("failed", None, 0)
    );
    let (status, visible) = call(&fixture.path, "GET", &base, Value::Null, Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{visible}");
    assert_eq!(
        (
            &visible["job"]["status"],
            &visible["job"]["verifiedConsistent"]
        ),
        (&json!("failed"), &Value::Null)
    );
    let (status, _) = call(&fixture.path, "GET", &base, Value::Null, Role::Candidate).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
