use rusqlite::params;
use serde_json::json;

use crate::task_input::project_source_digest;
use crate::{
    BranchId, ProjectId, ProjectRecord, RepositoryError, SnapshotId, SqliteStoryRepository,
};

#[test]
fn generation_check_cannot_commit_after_its_evaluated_story_source_changes() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository =
        SqliteStoryRepository::open(directory.path().join("generation-check.sqlite3")).unwrap();
    let project_id = ProjectId::parse("generation-check").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Check".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial-snapshot").unwrap(),
        )
        .unwrap();
    let budget = json!({"maxRequests":10,"maxTokens":100,"usedRequests":0,"usedTokens":0,"unknownTokens":0,"overrun":false,"charges":[]});
    let evaluated_source = project_source_digest(&repository.connection, &project_id).unwrap();
    let initial = json!({"id":"run-check","projectId":project_id.as_str(),"version":0,"status":"running","stage":"coordinate",
        "authorMessageId":"message-run-check","intentGeneration":0,
        "feedbackDecisions":[], "intentRevisions":[{"id":"intent-check","generation":0,"instruction":"生成故事","effectiveFromChapter":1,"source":{"type":"authorMessage","authorMessageId":"message-run-check","createdAt":1}}],
        "sourceDigest":evaluated_source,"baseSnapshotId":"initial-snapshot","startChapterNumber":1,"completedChapters":0,"chapterDeliveries":[],"targetChapters":1,"maxTaskAttempts":7,"usedTaskAttempts":0,"deadlineAt":600,"providerBudget":budget});
    repository
        .create_generation_run(&project_id, "run-check", &initial.to_string())
        .unwrap();
    let mut persisted = initial.clone();
    persisted.as_object_mut().unwrap().remove("providerBudget");
    repository
        .connection
        .execute(
            "UPDATE story_workspaces_v2 SET revision = 1 WHERE project_id = ?1",
            [project_id.as_str()],
        )
        .unwrap();
    let result = repository.update_generation_run_if_source_matches(
        &project_id,
        "run-check",
        0,
        r#"{"status":"completed"}"#,
        &evaluated_source,
    );
    assert!(matches!(result, Err(RepositoryError::AuthoringRunStale)));
    let unchanged: (i64, String, String) = repository.connection.query_row(
        "SELECT r.version, r.state_json, t.status FROM story_generation_runs_v1 r JOIN story_work_tasks_v1 t ON t.id = 'work-' || r.run_id WHERE r.project_id = ?1 AND r.run_id = 'run-check'",
        [project_id.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(unchanged, (0, persisted.to_string(), "running".into()));

    let current_source = project_source_digest(&repository.connection, &project_id).unwrap();
    let mut completed = initial;
    completed["status"] = json!("paused");
    completed["version"] = json!(1);
    let updated = repository
        .update_generation_run_if_source_matches(
            &project_id,
            "run-check",
            0,
            &completed.to_string(),
            &current_source,
        )
        .unwrap();
    assert_eq!(updated.version, 1);
    let task_status: String = repository
        .connection
        .query_row(
            "SELECT status FROM story_work_tasks_v1 WHERE id = 'work-run-check'",
            params![],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(task_status, "paused");
    let before = repository.generation_run(&project_id, "run-check").unwrap();
    let mut changed = completed;
    changed["version"] = json!(2);
    changed["providerBudget"]["maxTokens"] = json!(200);
    assert!(matches!(
        repository.update_generation_run(&project_id, "run-check", 1, &changed.to_string()),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(
        repository.generation_run(&project_id, "run-check").unwrap(),
        before
    );
    repository
        .connection
        .execute(
            "DELETE FROM story_work_budgets_v1 WHERE work_task_id='work-run-check'",
            [],
        )
        .unwrap();
    assert!(matches!(
        repository.generation_run(&project_id, "run-check"),
        Err(RepositoryError::CorruptState)
    ));
}

#[test]
fn claiming_generation_uses_validated_ledger_and_never_claims_a_refunded_balance() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository =
        SqliteStoryRepository::open(directory.path().join("ledger.sqlite3")).unwrap();
    let project = ProjectId::parse("budget-project").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "Budget".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("budget-snapshot").unwrap(),
        )
        .unwrap();
    let source = project_source_digest(&repository.connection, &project).unwrap();
    repository
        .create_task(crate::CreateTaskRequest {
            task_id: "task-next".into(),
            project_id: project.clone(),
            thread_id: "budget-thread".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"预算测试"}"#.into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut state = json!({
        "id":"run-budget","projectId":project.as_str(),"version":0,"stage":"coordinate",
        "status":"running","activeTaskId":null,"stepToken":"token","intentGeneration":0,
        "authorMessageId":"message-run-budget","completedChapters":0,"chapterDeliveries":[],"startChapterNumber":1,"targetChapters":1,
        "feedbackDecisions":[], "intentRevisions":[{"id":"intent-budget","generation":0,"instruction":"预算测试","effectiveFromChapter":1,"source":{"type":"authorMessage","authorMessageId":"message-run-budget","createdAt":now}}],
        "deadlineAt":now+600,"usedTaskAttempts":1,"maxTaskAttempts":10,"sourceDigest":source,"baseSnapshotId":"budget-snapshot",
        "providerBudget": {
            "maxRequests":10,"maxTokens":100,"usedRequests":1,"usedTokens":20,
            "unknownTokens":0,"overrun":false,"charges":[{
                "taskId":"task-first","budgetId":"budget-first","status":"completed",
                "requests":1,"tokens":20,"unknownTokens":0,"overrun":false
            }]
        }
    });
    let budget = state
        .as_object_mut()
        .unwrap()
        .remove("providerBudget")
        .unwrap();
    repository.connection.execute(
        "INSERT INTO story_work_tasks_v1(id, project_id, thread_id, goal, kind, status, input_revision) VALUES ('work-run-budget', ?1, '', 'generate', 'generation', 'running', 0)",
        [project.as_str()],
    ).unwrap();
    repository.connection.execute("INSERT INTO story_work_budgets_v1(work_task_id,ledger_json) VALUES ('work-run-budget',?1)", [budget.to_string()]).unwrap();
    repository.connection.execute(
        "INSERT INTO story_generation_runs_v1(run_id, project_id, version, state_json) VALUES ('run-budget', ?1, 0, ?2)",
        params![project.as_str(), state.to_string()],
    ).unwrap();
    crate::generation_author_source::record(&repository.connection, &project, "run-budget", &state)
        .unwrap();
    let next = repository
        .next_generation_decision(&project, "run-budget")
        .unwrap();
    assert_eq!(next["action"]["operation"], "advisory");
    let target = next["action"]["target"].clone();
    assert!(target["resumeStepId"].is_string());
    assert_eq!(target["workDecision"]["workTaskId"], "work-run-budget");
    for field in ["usedRequests", "usedTokens", "unknownTokens"] {
        let mut corrupted = budget.clone();
        corrupted[field] = json!(if field == "unknownTokens" { 1 } else { 0 });
        repository
            .connection
            .execute(
                "UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id='work-run-budget'",
                [corrupted.to_string()],
            )
            .unwrap();
        assert!(matches!(
            super::claim_generation_run_for_task(
                &repository.connection,
                &project,
                "run-budget",
                "task-next",
                "advisory",
                &target
            ),
            Err(RepositoryError::CorruptState)
        ));
        let stored: (i64, String) = repository
            .connection
            .query_row(
                "SELECT version,state_json FROM story_generation_runs_v1 WHERE run_id='run-budget'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (0, state.to_string()));
    }
    repository
        .connection
        .execute(
            "UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id='work-run-budget'",
            [budget.to_string()],
        )
        .unwrap();
    super::claim_generation_run_for_task(
        &repository.connection,
        &project,
        "run-budget",
        "task-next",
        "advisory",
        &target,
    )
    .unwrap();
    let stored: String = repository
        .connection
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id='run-budget'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut expected = state;
    expected["version"] = json!(1);
    expected["usedTaskAttempts"] = json!(2);
    expected["activeTaskId"] = json!("task-next");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stored).unwrap(),
        expected
    );
    repository
        .create_task(crate::CreateTaskRequest {
            task_id: "task-later".into(),
            project_id: project.clone(),
            thread_id: "later-thread".into(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: "{}".into(),
            input_manifest_json: r#"{"authorIntent":"第二次执行"}"#.into(),
            capability_hash: "b".repeat(64),
            capability_expires_at: i64::MAX,
        })
        .unwrap();
    assert!(matches!(
        crate::task_provider_budget::reserve(
            &repository.connection,
            &project,
            "work-run-budget",
            "task-later",
            Some(i64::MAX),
        ),
        Err(RepositoryError::WorkUsageUnsettled)
    ));
    // Even another writer bypassing the service guard cannot double-allocate the account.
    let duplicate = repository.connection.execute(
        "INSERT INTO story_task_provider_budgets_v1(task_id,work_task_id,grant_json) VALUES ('task-later','work-run-budget','{}')", [],
    ).unwrap_err();
    assert!(
        matches!(duplicate, rusqlite::Error::SqliteFailure(error, _) if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
    );
    let count: i64 = repository.connection.query_row(
        "SELECT COUNT(*) FROM story_task_provider_budgets_v1 WHERE work_task_id='work-run-budget' AND settled=0", [], |row|row.get(0),
    ).unwrap();
    assert_eq!(count, 1);

    // A terminal provider charge containing unknown usage is not authority for
    // another attempt, even when its finite metered allowance is not exhausted.
    let mut unknown_budget = budget;
    unknown_budget["unknownTokens"] = json!(10);
    unknown_budget["charges"][0]["unknownTokens"] = json!(10);
    let unknown_budget = unknown_budget.to_string();
    repository
        .connection
        .execute(
            "UPDATE story_work_budgets_v1 SET ledger_json=?1 WHERE work_task_id='work-run-budget'",
            [&unknown_budget],
        )
        .unwrap();
    assert!(matches!(
        crate::task_provider_budget::reserve(
            &repository.connection,
            &project,
            "work-run-budget",
            "task-later",
            Some(i64::MAX),
        ),
        Err(RepositoryError::WorkUsageUnknown)
    ));
    let work = repository.work_task(&project, "work-run-budget").unwrap();
    assert_eq!(work.resume_block_reason.as_deref(), Some("unknownUsage"));
    let stopped = repository
        .prepare_work_decision(
            &project,
            "work-run-budget",
            &crate::WorkDecisionRequest {
                expected_work_version: work.version,
                expected_revision: repository.project_revision(&project).unwrap(),
                expected_source_digest: project_source_digest(&repository.connection, &project)
                    .unwrap(),
            },
        )
        .unwrap();
    assert_eq!(stopped.stop_reason, Some("unknownUsage"));
    assert!(stopped.step.is_none());
    assert_eq!(
        repository.work_task(&project, "work-run-budget").unwrap(),
        work
    );
    let stored_budget: String = repository
        .connection
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id='work-run-budget'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_budget, unknown_budget);
    let count: i64 = repository.connection.query_row(
        "SELECT COUNT(*) FROM story_task_provider_budgets_v1 WHERE work_task_id='work-run-budget'",
        [], |row|row.get(0),
    ).unwrap();
    assert_eq!(count, 1);
}

use crate::{AuthoringRunsRepository, ProjectRevisionRepository, TasksRepository, WorkDecisionGenerationRepository, WorkDecisionRepository, WorkTasksRepository};
