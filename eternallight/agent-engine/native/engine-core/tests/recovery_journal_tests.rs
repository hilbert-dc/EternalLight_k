use eternallight_engine_core::RecoveryJournal;
use serde_json::{Value, json};

#[test]
fn persistence_failure_does_not_advance_observed_state() {
    let mut journal = RecoveryJournal::default();
    let run = json!({"runId":"run"});
    let begin = journal.apply("begin", &run).unwrap();
    assert!(journal.apply("begin", &run).is_err());
    journal.apply("abort", &Value::Null).unwrap();
    assert_eq!(journal.apply("begin", &run).unwrap(), begin);
    journal.apply("commit", &Value::Null).unwrap();
    let event = json!({"event":{"method":"turn/started","params":{"turn":{"id":"turn"}}}});
    let record = journal.apply("observe", &event).unwrap();
    journal.apply("abort", &Value::Null).unwrap();
    assert_eq!(journal.apply("observe", &event).unwrap(), record);
    journal.apply("commit", &Value::Null).unwrap();
    assert_eq!(journal.apply("observe", &event).unwrap(), Value::Null);
}

#[test]
fn write_completion_without_receipt_remains_uncertain_and_never_terminalizes_run() {
    let mut journal = RecoveryJournal::default();
    journal.apply("begin", &json!({"runId":"run"})).unwrap();
    journal.apply("commit", &Value::Null).unwrap();
    let mut input = json!({"event":{"method":"item/completed","params":{"item":{"type":"mcpToolCall","id":"call","status":"completed","tool":"write"}}}});
    assert_eq!(
        journal.apply("observe", &input).unwrap()["kind"],
        "classify"
    );
    input["classification"] = json!({"effect":"write"});
    input["requestHash"] = "opaque-hash".into();
    journal.apply("observe", &input).unwrap();
    journal.apply("commit", &Value::Null).unwrap();
    assert_eq!(journal.apply("observe", &input).unwrap()["kind"], "receipt");
    input["resolution"] = json!({"outcome":"committed"});
    assert!(journal.apply("observe", &input).is_err());
    input["resolution"] = Value::Null;
    assert_eq!(
        journal.apply("observe", &input).unwrap(),
        json!({"kind":"persist","method":"transitionRequest","args":["run","mcp:call","uncertain"]})
    );
    journal.apply("commit", &Value::Null).unwrap();
    assert_eq!(journal.apply("observe", &input).unwrap(), Value::Null);
    assert_eq!(journal.apply("end", &Value::Null).unwrap(), false);
}
