use eternallight_engine_core::Reconciliation;
use serde_json::json;

#[test]
fn reconciliation_requires_persisted_acknowledgements_before_advancing() {
    let mut run = Reconciliation::new(
        &json!({"runId":"r","reconciledAt":"now","runtimeStatus":"cancelled","fence":"f"}),
    )
    .unwrap();
    assert_eq!(
        run.next(),
        json!({"method":"reconciliationCandidates","args":["r"]})
    );
    let candidate = json!({"runId":"r","requestId":"w","operation":"write"});
    run.accept(&json!({"method":"reconciliationCandidates","value":[candidate]}))
        .unwrap();
    assert!(run.accept(&json!({"method":"settleRequest"})).is_err());
    assert!(
        run.accept(&json!({"method":"resolve","value":{"outcome":"committed"}}))
            .is_err()
    );
    assert_eq!(run.next(), json!({"method":"resolve","args":[candidate]}));
    let resolution = json!({"outcome":"committed","receiptRef":"receipt"});
    run.accept(&json!({"method":"resolve","value":resolution}))
        .unwrap();
    assert_eq!(
        run.next(),
        json!({"method":"settleRequest","args":["r","w",resolution,"now"]})
    );
    assert!(
        run.accept(&json!({"method":"resolve","value":resolution}))
            .is_err()
    );
    run.accept(&json!({"method":"settleRequest"})).unwrap();
    assert_eq!(
        run.next(),
        json!({"method":"reconcileRun","args":["r","cancelled","now",{"fence":"f"}]})
    );
    run.accept(&json!({"method":"reconcileRun"})).unwrap();
    run.accept(&json!({"method":"reconciliationCandidates","value":[]}))
        .unwrap();
    assert_eq!(
        run.next(),
        json!({"method":"complete","result":{"runId":"r","status":"reconciled","operations":[{"requestId":"w","outcome":"committed","receiptRef":"receipt"}],"unresolved":[]}})
    );
    assert!(run.accept(&json!({"method":"complete"})).is_err());
}

#[test]
fn invalid_candidates_do_not_change_recovery_state() {
    let mut run = Reconciliation::new(&json!({"runId":"r"})).unwrap();
    let before = run.next();
    for candidates in [
        json!([{"runId":"other","requestId":"a"}]),
        json!([{"requestId":"a"},{"requestId":"a"}]),
    ] {
        assert!(
            run.accept(&json!({"method":"reconciliationCandidates","value":candidates}))
                .is_err()
        );
        assert_eq!(run.next(), before);
    }
}
