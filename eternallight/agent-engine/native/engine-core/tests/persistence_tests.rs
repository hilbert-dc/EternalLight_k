use eternallight_engine_core::{PersistentBudget, recovery_operation};
use serde_json::json;

#[test]
fn active_budget_is_not_recovered_and_orphan_reservations_are_charged_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite3");
    let mut budget = PersistentBudget::new(
        &path,
        "run",
        json!({"options":{"id":"budget","maxTokens":100},"now":0}),
    )
    .unwrap();
    budget
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":24},"now":1}),
        )
        .unwrap();
    assert_eq!(PersistentBudget::recover(&path).unwrap(), json!([]));
    budget.close();
    assert_eq!(
        PersistentBudget::recover(&path).unwrap(),
        json!([{"budgetId":"budget","unknownTokens":24.0,"overrun":false}])
    );
    assert_eq!(PersistentBudget::recover(&path).unwrap(), json!([]));
}

#[test]
fn failed_sql_commit_keeps_memory_unchanged_and_blocks_further_execution() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite3");
    let mut budget = PersistentBudget::new(&path, "run", json!({"options":{},"now":0})).unwrap();
    let previous = budget.inspect();
    let database = rusqlite::Connection::open(&path).unwrap();
    database.execute_batch("CREATE TRIGGER fail_write BEFORE UPDATE ON engine_budgets BEGIN SELECT RAISE(ABORT, 'simulated storage failure'); END;").unwrap();
    assert!(
        budget
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":24},"now":1})
            )
            .unwrap_err()
            .contains("simulated storage failure")
    );
    assert_eq!(budget.inspect(), previous);
    assert!(
        budget
            .apply("budget.reserve", json!({"parameters":{},"now":2}))
            .unwrap_err()
            .contains("requires recovery")
    );
    assert!(budget.complete("completed").is_err());
}

#[test]
fn independent_recovery_instances_serialize_and_failed_batches_do_not_persist() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recovery.json");
    recovery_operation(
        &path,
        "begin",
        &json!({"runId":"run","planId":"plan","threadId":"thread"}),
    )
    .unwrap();
    std::thread::scope(|scope| {
        for index in 0..12 {
            let path = &path;
            scope.spawn(move || recovery_operation(path,"record",&json!({"runId":"run","requestId":format!("request-{index}"),"state":"prepared","kind":"model"})).unwrap());
        }
    });
    let before = recovery_operation(&path, "read", &json!({})).unwrap();
    assert_eq!(
        before["runs"]["run"]["requests"].as_object().unwrap().len(),
        12
    );
    assert!(recovery_operation(&path,"observe",&json!({"runId":"run","operations":[{"run_id":"run","tool_call_id":"write","tool_name":"save","operation_id":"op","state":"committed"},{"run_id":"other"}]})).is_err());
    assert_eq!(
        recovery_operation(&path, "read", &json!({})).unwrap(),
        before
    );
}
