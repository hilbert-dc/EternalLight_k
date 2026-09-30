use novel_codex_story_core::{ProjectId, RepositoryError, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::path::Path;

pub async fn exhaust_final_assessment_allowance(path: &Path, project: &str, task: &str) {
    let (status, grant) = super::call(
        path,
        "GET",
        &format!("/api/projects/{project}/tasks/{task}/provider-budget"),
        /*payload*/ None,
        /*candidate*/ false,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{grant}");
    let mut budget = eternallight_engine_core::PersistentBudget::new(
        &path.with_extension("budget.sqlite3"),task,
        json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000},"now":1000}),
    ).unwrap();
    for _ in 0..grant["maxRequests"].as_u64().unwrap() {
        budget
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":1},"now":1001}),
            )
            .unwrap();
    }
    budget.complete("terminal").unwrap();
}

pub fn before_commit(path: &Path, project: &str, run: &Value) {
    let project = ProjectId::parse(project).unwrap();
    let id = run["id"].as_str().unwrap();
    let mut repository = SqliteStoryRepository::open(path).unwrap();
    let before = repository.generation_run(&project, id).unwrap();
    let work = repository
        .work_task(&project, &format!("work-{id}"))
        .unwrap();
    let mut proposed: Value = serde_json::from_str(&before.state_json).unwrap();
    let number = proposed["completedChapters"].as_i64().unwrap() + 1;
    proposed["completedChapters"] = json!(number);
    proposed["version"] = json!(before.version + 1);
    proposed["status"] = json!(if proposed["targetChapters"] == number {
        "completed"
    } else {
        "running"
    });
    proposed["stage"] = json!(if proposed["targetChapters"] == number {
        "finished"
    } else {
        "rollingPlan"
    });
    proposed["currentChapterId"] = Value::Null;
    proposed["currentChapterNumber"] = Value::Null;
    proposed["acceptedFeedbackRevisionId"] = Value::Null;
    proposed["stepToken"] = json!("next-delivery-check");
    if !run["chapterDeliveries"].as_array().unwrap().is_empty() {
        let mut forged = proposed.clone();
        forged["chapterDeliveries"][0]["checkedSnapshotId"] = json!("not-the-checked-snapshot");
        assert_eq!(
            repository.update_generation_run(&project, id, before.version, &forged.to_string()),
            Err(RepositoryError::TaskBindingInvalid)
        );
        assert_eq!(repository.generation_run(&project, id).unwrap(), before);
    }
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_delivery_work BEFORE UPDATE ON story_work_tasks_v1 BEGIN SELECT RAISE(ABORT,'late delivery failure'); END;").unwrap();
    assert!(matches!(
        repository.update_generation_run(&project, id, before.version, &proposed.to_string()),
        Err(RepositoryError::Storage(_))
    ));
    assert_eq!(repository.generation_run(&project, id).unwrap(), before);
    assert_eq!(
        repository
            .work_task(&project, &format!("work-{id}"))
            .unwrap(),
        work
    );
    db.execute_batch("DROP TRIGGER fail_delivery_work;")
        .unwrap();
}

pub fn after_commit(path: &Path, project: &str, previous: &Value, checked: &Value) {
    let number = previous["currentChapterNumber"].as_i64().unwrap();
    let deliveries = checked["chapterDeliveries"].as_array().unwrap();
    let prior = previous["chapterDeliveries"].as_array().unwrap();
    assert_eq!(&deliveries[..prior.len()], prior);
    assert_eq!(deliveries.len(), prior.len() + 1);
    let proof = deliveries.last().unwrap();
    assert_eq!(
        json!({"chapter":proof["chapterId"],"number":proof["chapterNumber"],"version":proof["checkedRunVersion"],"source":proof["sourceDigest"],"task":proof["assessmentTaskId"]}),
        json!({"chapter":previous["currentChapterId"],"number":number,"version":previous["version"],"source":previous["sourceDigest"],"task":previous["lastTaskId"]})
    );
    assert_eq!(
        proof["assessmentRevisionId"],
        previous["acceptedFeedbackRevisionId"]
    );
    let decision = previous["feedbackDecisions"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|decision| decision["choice"] == "continue")
        .unwrap();
    assert_eq!(proof["feedbackDecisionId"], decision["id"]);
    assert_eq!(checked["providerBudget"], previous["providerBudget"]);
    let project = ProjectId::parse(project).unwrap();
    let repository = SqliteStoryRepository::open(path).unwrap();
    let id = checked["id"].as_str().unwrap();
    let reopened: Value =
        serde_json::from_str(&repository.generation_run(&project, id).unwrap().state_json).unwrap();
    assert_eq!(reopened, *checked);
    let work = repository
        .work_task(&project, &format!("work-{id}"))
        .unwrap();
    assert_eq!(
        (work.status.as_str(), work.delivery_status.as_str()),
        (
            checked["status"].as_str().unwrap(),
            if checked["status"] == "completed" {
                "ready_for_review"
            } else {
                "partial"
            }
        )
    );
    let db = rusqlite::Connection::open(path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let mut damaged: Value = serde_json::from_str(&stored).unwrap();
    damaged["chapterDeliveries"][0]["checkedSnapshotId"] = json!("missing-snapshot");
    db.execute(
        "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
        rusqlite::params![damaged.to_string(), id],
    )
    .unwrap();
    assert_eq!(
        repository.generation_run(&project, id),
        Err(RepositoryError::TaskBindingInvalid)
    );
    db.execute(
        "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
        rusqlite::params![stored, id],
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&repository.generation_run(&project, id).unwrap().state_json)
            .unwrap(),
        *checked
    );
}
