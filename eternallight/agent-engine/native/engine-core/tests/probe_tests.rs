use eternallight_engine_core::{ProbeSession, dispatch};
use serde_json::{Value, json};

#[test]
fn fatal_failure_seals_attempts_and_rejects_stale_completion() {
    let mut session = ProbeSession::new(&json!({"protocol":"auto","maxAttempts":4})).unwrap();
    assert_eq!(session.apply("next", &Value::Null).unwrap(), "responses");
    assert!(session.apply("next", &Value::Null).is_err());
    assert!(
        session
            .apply("success", &json!({"protocol":"messages"}))
            .is_err()
    );
    session.apply("failure",&json!({"protocol":"responses","error":{"status":401,"category":"transport","message":"denied"}})).unwrap();
    assert_eq!(session.apply("next", &Value::Null).unwrap(), Value::Null);
    assert!(
        session
            .apply("success", &json!({"protocol":"responses"}))
            .is_err()
    );
    assert_eq!(
        session.apply("result", &Value::Null).unwrap(),
        json!({"attempts":[{"protocol":"responses","status":"failed","category":"authentication","message":"denied"}]})
    );
}

#[test]
fn retry_cannot_override_access_failures_and_round_trip_requires_one_exact_call() {
    for (status, retry) in [
        (400, true),
        (401, false),
        (403, false),
        (422, true),
        (429, false),
        (500, false),
    ] {
        assert_eq!(
            dispatch(
                "probe.retryToolChoice",
                json!({"status":status,"payload":{"error":{"message":"unsupported tool_choice"}}})
            )
            .unwrap(),
            retry
        );
    }
    let call = json!({"type":"function_call","name":"engine_probe","arguments":"{\"nonce\":\"agent-engine\"}"});
    assert_eq!(
        dispatch("probe.roundTrip", json!({"output":[call]})).unwrap(),
        Value::Null
    );
    assert!(dispatch("probe.roundTrip", json!({"output":[call,call]})).is_err());
    assert!(
        ProbeSession::new(&json!({"protocols":["responses","responses"],"maxAttempts":4})).is_err()
    );
}
