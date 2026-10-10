use eternallight_engine_core::{PersistentBudget, dispatch};
use serde_json::{Value, json};

#[test]
fn media_reservation_identity_is_persistent_and_unknown_usage_never_refunds_requests() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite");
    let mut budget = PersistentBudget::new(
        &path,
        "run",
        json!({"options":{"id":"budget","maxRequests":1,"maxTokens":50},"now":0}),
    )
    .unwrap();
    let parameters =
        json!({"reservationId":"media:op","estimatedInputTokens":0,"maxOutputTokens":0});
    let id = budget
        .apply("budget.reserve", json!({"parameters":parameters,"now":1}))
        .unwrap();
    assert_eq!(id, "media:op");
    assert_eq!(
        budget
            .apply("budget.reserve", json!({"parameters":parameters,"now":2}))
            .unwrap(),
        id
    );
    assert!(
        budget
            .apply(
                "budget.reserve",
                json!({"parameters":{"reservationId":"media:op","maxOutputTokens":1},"now":2})
            )
            .is_err()
    );
    budget
        .apply(
            "budget.settle",
            json!({"id":id,"usage":null,"outcome":"unknown"}),
        )
        .unwrap();
    assert_eq!(budget.inspect()["requests"], 1);
    budget.close();
    // Re-open the actual SQLite file and replay its native state, rather than an
    // in-memory JS cache. Task run ownership/lifecycle still belongs to the Host.
    let database = rusqlite::Connection::open(&path).unwrap();
    let encoded: String = database
        .query_row(
            "SELECT state_json FROM engine_budgets WHERE run_id='run'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let state: Value = serde_json::from_str(&encoded).unwrap();
    let result = dispatch(
        "budget.reserve",
        json!({"state":state,"parameters":parameters,"now":3}),
    )
    .unwrap();
    assert_eq!(result["result"], id);
    assert_eq!(result["state"]["requests"], 1);
    assert!(
        dispatch(
            "budget.reserve",
            json!({"state":result["state"],"parameters":{"reservationId":"media:another"},"now":3})
        )
        .is_err()
    );
    PersistentBudget::recover(&path).unwrap();
    assert_eq!(
        PersistentBudget::receipt(&path, "run").unwrap()["usage"]["requests"],
        1
    );
}

#[test]
fn refunded_reservation_identity_cannot_be_reused_to_bypass_admission() {
    let state = dispatch(
        "budget.create",
        json!({"options":{"id":"budget","maxRequests":1},"now":0}),
    )
    .unwrap();
    let reserved = dispatch(
        "budget.reserve",
        json!({"state":state,"parameters":{"reservationId":"media:op"},"now":1}),
    )
    .unwrap();
    let settled = dispatch(
        "budget.settle",
        json!({"state":reserved["state"],"id":"media:op","usage":null,"outcome":"not-sent"}),
    )
    .unwrap();
    assert_eq!(settled["state"]["requests"], 0);
    assert!(
        dispatch(
            "budget.reserve",
            json!({"state":settled["state"],"parameters":{"reservationId":"media:op"},"now":2})
        )
        .is_err()
    );
}
