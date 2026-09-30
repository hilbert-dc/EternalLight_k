use novel_codex_story_core::{ProjectId, SqliteStoryRepository, TaskProviderBudget};
use serde_json::json;

/// Seals and charges the real zero-request engine receipt for a fixture-only
/// attempt. Call explicitly before claiming its derived work, never from the
/// request transport or authorization rejection tests.
pub fn settle(path: &std::path::Path, project: &str, task: &str) {
    let db = rusqlite::Connection::open(path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [task],
            |row| row.get(0),
        )
        .unwrap();
    let grant: TaskProviderBudget = serde_json::from_str(&stored).unwrap();
    let engine_path = path.with_extension("engine.sqlite3");
    let mut budget = eternallight_engine_core::PersistentBudget::new(
        &engine_path,
        task,
        json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,
            "maxTokens":grant.max_tokens,"startedAt":1000},"now":1000}),
    )
    .unwrap();
    budget.complete("completed").unwrap();
    let receipt = eternallight_engine_core::PersistentBudget::receipt(&engine_path, task).unwrap();
    SqliteStoryRepository::open(path)
        .unwrap()
        .charge_work_provider_usage(
            &ProjectId::parse(project).unwrap(),
            &grant.work_task_id,
            task,
            &receipt,
        )
        .unwrap();
}
