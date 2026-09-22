use eternallight_engine_core::{HostRun, HostSession};
use serde_json::{Value, json};

#[test]
fn closed_or_blocked_sessions_cannot_be_reopened_by_late_callbacks() {
    let mut session = HostSession::new("preparing").unwrap();
    session.apply("closing", &Value::Null).unwrap();
    session.apply("closed", &Value::Null).unwrap();
    assert!(session.apply("ready", &json!("thread")).is_err());
    session.apply("fail", &Value::Null).unwrap();
    assert_eq!(
        session.apply("snapshot", &Value::Null).unwrap(),
        json!({"state":"closed","threadId":null,"activeRunId":null})
    );
    let mut session = HostSession::new("recovering").unwrap();
    session.apply("ready", &json!("thread")).unwrap();
    session.apply("attach", &json!("run")).unwrap();
    assert!(
        session
            .apply("finish", &json!({"runId":"other","state":"completed"}))
            .is_err()
    );
    session
        .apply("finish", &json!({"runId":"run","state":"outcome-unknown"}))
        .unwrap();
    assert!(session.apply("attach", &json!("next")).is_err());
}

#[test]
fn child_events_and_late_events_cannot_reopen_cancellation_or_terminal_state() {
    let mut run =
        HostRun::new(&json!({"runId":"run","threadId":"root","epoch":1,"maximum":2})).unwrap();
    run.apply("start", &Value::Null).unwrap();
    run.apply("record",&json!({"event":{"method":"item/started","params":{"threadId":"child","item":{"id":"a","type":"mcpToolCall"}}}})).unwrap();
    assert_eq!(
        run.apply("snapshot", &Value::Null).unwrap()["state"],
        "running"
    );
    run.apply("cancel", &Value::Null).unwrap();
    assert!(run.apply("checkTerminal", &json!("completed")).is_err());
    run.apply("cancelFailed", &Value::Null).unwrap();
    run.apply(
        "record",
        &json!({"event":{"method":"turn/started","params":{"threadId":"root"}}}),
    )
    .unwrap();
    assert_eq!(
        run.apply("snapshot", &Value::Null).unwrap()["state"],
        "cancellation-failed"
    );
    run.apply("terminal", &json!("outcome-unknown")).unwrap();
    run.apply(
        "record",
        &json!({"event":{"method":"turn/started","params":{}}}),
    )
    .unwrap();
    assert_eq!(
        run.apply("snapshot", &Value::Null).unwrap()["state"],
        "outcome-unknown"
    );
    assert_eq!(
        run.apply("replay", &json!(0))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn replay_bytes_and_sequence_cursor_are_bounded() {
    let mut run =
        HostRun::new(&json!({"runId":"run","threadId":"root","epoch":1,"maximum":2})).unwrap();
    for n in 0..4 {
        run.apply(
            "record",
            &json!({"event":{"method":"notice","params":{"n":n}}}),
        )
        .unwrap();
    }
    assert!(run.apply("replay", &json!(1)).is_err());
    let replay = run.apply("replay", &json!(2)).unwrap();
    assert_eq!(
        replay
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["params"].clone())
            .collect::<Vec<_>>(),
        vec![json!({"n":2}), json!({"n":3})]
    );
    let before = run.apply("snapshot", &Value::Null).unwrap();
    assert!(
        run.apply(
            "record",
            &json!({"event":{"params":{"text":"x".repeat(256*1024)}}})
        )
        .is_err()
    );
    assert_eq!(run.apply("snapshot", &Value::Null).unwrap(), before);
}
