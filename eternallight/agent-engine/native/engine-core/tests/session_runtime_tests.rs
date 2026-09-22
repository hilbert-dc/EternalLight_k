use eternallight_engine_core::SessionRuntime;
use serde_json::json;

#[test]
fn late_acquisition_after_revoke_is_cleaned_but_cannot_dispatch() {
    let mut runtime = SessionRuntime::default();
    let lease = json!({"runId":"r","leaseId":"l","epoch":0});
    let generation = runtime.apply("begin", &json!({"lease":lease})).unwrap();
    for step in ["budget", "bridge", "budgetStarted"] {
        runtime
            .apply("ack", &json!({"generation":generation,"step":step}))
            .unwrap();
    }
    assert!(
        runtime
            .apply("dispatch", &json!({"generation":generation}))
            .is_err()
    );
    runtime
        .apply(
            "revoke",
            &json!({"request":{"runId":"r","leaseId":"l","expectedEpoch":0}}),
        )
        .unwrap();
    runtime
        .apply(
            "ack",
            &json!({"generation":generation,"step":"leaseStarted"}),
        )
        .unwrap();
    assert!(
        runtime
            .apply("current", &json!({"generation":generation}))
            .is_err()
    );
    assert_eq!(
        runtime
            .apply("cleanup", &json!({"generation":generation}))
            .unwrap(),
        json!(["lease", "budget"])
    );
    runtime
        .apply("settled", &json!({"generation":generation,"failed":true}))
        .unwrap();
    assert!(runtime.apply("begin", &json!({"lease":lease})).is_err());
}

#[test]
fn runtime_generation_and_single_dispatch_fence_delayed_callbacks() {
    let mut runtime = SessionRuntime::default();
    let lease = json!({"runId":"r","leaseId":"l","epoch":0});
    let first = runtime.apply("begin", &json!({"lease":lease})).unwrap();
    runtime
        .apply("settled", &json!({"generation":first,"failed":false}))
        .unwrap();
    let second = runtime.apply("begin", &json!({"lease":lease})).unwrap();
    assert!(
        runtime
            .apply("ack", &json!({"generation":first,"step":"budget"}))
            .is_err()
    );
    for step in [
        "budget",
        "bridge",
        "budgetStarted",
        "leaseStarted",
        "appBound",
    ] {
        runtime
            .apply("ack", &json!({"generation":second,"step":step}))
            .unwrap();
    }
    runtime
        .apply("dispatch", &json!({"generation":second}))
        .unwrap();
    assert!(
        runtime
            .apply("dispatch", &json!({"generation":second}))
            .is_err()
    );
    runtime.apply("close", &json!({})).unwrap();
    assert!(runtime.apply("begin", &json!({"lease":lease})).is_err());
}
