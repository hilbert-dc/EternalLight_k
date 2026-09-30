use novel_codex_story_core::{ProviderBudget, UsageChargeOutcome};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn budget() -> ProviderBudget {
    ProviderBudget {
        max_requests: 10,
        max_tokens: 100,
        used_requests: 0,
        used_tokens: 0,
        unknown_tokens: 0,
        overrun: false,
        charges: Vec::new(),
    }
}

fn receipt(task: &str, id: &str, tokens: u64, unknown: u64) -> Value {
    json!({
        "runId": task, "budgetId": id, "status": "completed",
        "limits": {"maxRequests": 10, "maxTokens": 100, "maxDurationMs": 1000},
        "usage": {"requests": 1, "tokens": tokens, "unknownTokens": unknown, "overrun": false}
    })
}

#[test]
fn cumulative_usage_survives_reload_and_replay_without_refunding_unknown_cost() {
    let mut account = budget();
    let first = receipt("task-a", "budget-a", 20, 30);
    assert_eq!(
        account.charge_receipt("task-a", &first).unwrap(),
        UsageChargeOutcome::Applied
    );
    let saved = serde_json::to_value(&account).unwrap();
    let mut account: ProviderBudget = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(
        account.charge_receipt("task-a", &first).unwrap(),
        UsageChargeOutcome::AlreadyCharged
    );
    assert_eq!(serde_json::to_value(&account).unwrap(), saved);
    assert_eq!(
        (
            account.remaining_requests(),
            account.remaining_tokens(),
            account.exhausted()
        ),
        (9, 50, false)
    );
    account
        .charge_receipt("task-b", &receipt("task-b", "budget-b", 51, 0))
        .unwrap();
    assert_eq!(
        (
            account.remaining_requests(),
            account.remaining_tokens(),
            account.exhausted(),
            account.overrun
        ),
        (8, 0, true, true)
    );
    // A late terminal receipt must still be accounted after exhaustion.
    account
        .charge_receipt("task-c", &receipt("task-c", "budget-c", 4, 0))
        .unwrap();
    assert_eq!(
        (
            account.used_tokens,
            account.unknown_tokens,
            account.charges.len()
        ),
        (75, 30, 3)
    );
}

#[test]
fn conflicting_receipts_and_reused_budget_identity_never_change_the_account() {
    let mut account = budget();
    account
        .charge_receipt("task-a", &receipt("task-a", "budget-a", 20, 0))
        .unwrap();
    let before = account.clone();
    let cases = [
        ("task-a", receipt("task-a", "budget-a", 21, 0)),
        ("task-b", receipt("task-b", "budget-a", 20, 0)),
        ("task-b", receipt("task-a", "budget-b", 20, 0)),
    ];
    for (task, value) in cases {
        assert!(account.charge_receipt(task, &value).is_err());
        assert_eq!(account, before);
    }
    let mut active = receipt("task-b", "budget-b", 20, 0);
    active["status"] = json!("active");
    assert!(account.charge_receipt("task-b", &active).is_err());
    assert_eq!(account, before);
}

#[test]
fn inconsistent_in_memory_totals_are_rejected_without_partial_changes() {
    let mut account = budget();
    account.used_tokens = u64::MAX;
    let before = account.clone();
    assert!(
        account
            .charge_receipt("task-a", &receipt("task-a", "budget-a", 1, 0))
            .is_err()
    );
    assert_eq!(account, before);
    account.used_tokens = 0;
    account.unknown_tokens = u64::MAX;
    let before = account.clone();
    assert!(
        account
            .charge_receipt("task-a", &receipt("task-a", "budget-a", 1, 1))
            .is_err()
    );
    assert_eq!(account, before);
}

#[test]
fn reloading_rejects_balance_refunds_and_duplicate_or_nonterminal_evidence() {
    let mut account = budget();
    account
        .charge_receipt("task-a", &receipt("task-a", "budget-a", 20, 10))
        .unwrap();
    let saved = serde_json::to_value(account).unwrap();
    for (path, value) in [
        ("/usedRequests", json!(0)),
        ("/usedTokens", json!(0)),
        ("/unknownTokens", json!(0)),
        ("/overrun", json!(true)),
        ("/maxTokens", json!(0)),
        ("/charges/0/status", json!("active")),
        ("/charges/0/taskId", json!("")),
    ] {
        let mut damaged = saved.clone();
        *damaged.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<ProviderBudget>(damaged).is_err(),
            "{path}"
        );
    }
    let mut duplicate = saved.clone();
    duplicate["charges"]
        .as_array_mut()
        .unwrap()
        .push(saved["charges"][0].clone());
    duplicate["usedRequests"] = json!(2);
    duplicate["usedTokens"] = json!(40);
    duplicate["unknownTokens"] = json!(20);
    assert!(serde_json::from_value::<ProviderBudget>(duplicate).is_err());
}

#[test]
fn bounded_receipt_capacity_blocks_new_execution_before_cost_can_go_unrecorded() {
    let mut account = budget();
    account.max_requests = 100;
    for index in 0..32 {
        assert!(!account.exhausted());
        let task = format!("task-{index}");
        let id = format!("budget-{index}");
        account
            .charge_receipt(&task, &receipt(&task, &id, 0, 0))
            .unwrap();
    }
    assert!(account.exhausted());
    assert_eq!(
        (account.remaining_requests(), account.remaining_tokens()),
        (68, 100)
    );
    let saved = account.clone();
    assert_eq!(
        account
            .charge_receipt("task-31", &receipt("task-31", "budget-31", 0, 0))
            .unwrap(),
        UsageChargeOutcome::AlreadyCharged
    );
    assert_eq!(account, saved);
}
