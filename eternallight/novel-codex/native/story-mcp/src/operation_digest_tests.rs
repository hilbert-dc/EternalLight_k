use super::operation_digest;
use serde_json::json;

#[test]
fn numeric_spellings_share_the_host_json_identity_without_losing_fractional_values() {
    let wire:serde_json::Value=serde_json::from_str(r#"{"payload":{"count":1.0,"zero":-0.0,"fraction":1.25,"small":1e-6},"limits":[9007199254740991.0,-9007199254740991.0]}"#).unwrap();
    let host = json!({"payload":{"count":1,"zero":0,"fraction":1.25,"small":0.000001},"limits":[9_007_199_254_740_991i64,-9_007_199_254_740_991i64]});
    assert_eq!(super::host_json(&wire), super::host_json(&host));
    assert_eq!(
        operation_digest("save_brief", &wire),
        operation_digest("save_brief", &host)
    );
    assert_ne!(
        operation_digest("save_brief", &host),
        operation_digest("save_brief", &json!({"payload":{"count":2}}))
    );
    for value in [
        json!(9_007_199_254_740_992u64),
        json!(9_007_199_254_740_993u64),
        json!(-9_007_199_254_740_992i64),
        json!(1e20),
    ] {
        assert!(operation_digest("save_brief", &json!({"payload":{"count":value}})).is_err());
    }
}
