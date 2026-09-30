use super::{AUTHOR, call, setup};
use axum::http::StatusCode;
use novel_codex_story_core::{ProjectId, RepositoryError, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[tokio::test]
async fn an_empty_generation_cannot_claim_delivery_or_complete_through_repository_updates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("delivery-gate.sqlite3");
    let (base, request) = setup(&path).await;
    let (status, run) = call(
        &path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["chapterDeliveries"], json!([]));
    let project = ProjectId::parse(base.trim_start_matches("/api/projects/")).unwrap();
    let id = run["id"].as_str().unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let before = repository.generation_run(&project, id).unwrap();
    let work_before = repository
        .work_task(&project, &format!("work-{id}"))
        .unwrap();
    for replacement in [
        json!({"status":"completed"}),
        json!({"stage":"finished"}),
        json!({"status":"completed","stage":"finished","completedChapters":1}),
        json!({"completedChapters":1,"stage":"rollingPlan"}),
        json!({"chapterDeliveries":[{"chapterId":"made-up-proof"}]}),
    ] {
        let mut next: Value = serde_json::from_str(&before.state_json).unwrap();
        for (key, value) in replacement.as_object().unwrap() {
            next[key] = value.clone();
        }
        next["version"] = json!(before.version + 1);
        assert_eq!(
            repository.update_generation_run(&project, id, before.version, &next.to_string()),
            Err(RepositoryError::TaskBindingInvalid)
        );
        assert_eq!(repository.generation_run(&project, id).unwrap(), before);
        assert_eq!(
            repository
                .work_task(&project, &format!("work-{id}"))
                .unwrap(),
            work_before
        );
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let mut damaged: Value = serde_json::from_str(&stored).unwrap();
    damaged["status"] = json!("completed");
    damaged["stage"] = json!("finished");
    damaged["completedChapters"] = json!(1);
    db.execute(
        "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
        rusqlite::params![damaged.to_string(), id],
    )
    .unwrap();
    assert!(
        !call(
            &path,
            "GET",
            &format!("{base}/generation-runs/{id}"),
            Value::Null,
            AUTHOR
        )
        .await
        .0
        .is_success()
    );
    db.execute(
        "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
        rusqlite::params![stored, id],
    )
    .unwrap();
    assert_eq!(repository.generation_run(&project, id).unwrap(), before);
}
