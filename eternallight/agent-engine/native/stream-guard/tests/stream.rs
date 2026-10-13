use eternallight_stream_guard::{Command, StreamGuard};
use serde_json::{Value, json};

fn push(guard: &mut StreamGuard, event: Value) -> Result<Vec<Value>, &'static str> {
    guard.apply(Command::Push { event })
}

#[test]
fn preserves_reasoning_prefix_and_buffered_suffix() {
    let mut guard = StreamGuard::default();
    let part = json!({"type":"response.reasoning_summary_part.added"});
    assert_eq!(push(&mut guard, part.clone()), Ok(vec![part.clone()]));
    let tool = json!({"type":"response.output_item.done","item":{"type":"function_call"}});
    assert_eq!(push(&mut guard, tool.clone()), Ok(vec![]));
    assert_eq!(push(&mut guard, part.clone()), Ok(vec![]));
    let terminal =
        json!({"type":"response.completed","response":{"status":"completed","output":[]}});
    assert_eq!(push(&mut guard, terminal.clone()), Ok(vec![]));
    assert_eq!(guard.apply(Command::Finish), Ok(vec![tool, part, terminal]));
    assert_eq!(guard.apply(Command::Finish), Err("stream already closed"));
}

#[test]
fn failed_and_truncated_streams_never_commit() {
    for kind in ["error", "response.failed", "response.incomplete"] {
        let mut guard = StreamGuard::default();
        assert_eq!(
            push(&mut guard, json!({"type":"response.output_item.done"})),
            Ok(vec![])
        );
        assert!(push(&mut guard, json!({"type":kind})).is_err());
        assert!(guard.apply(Command::Finish).is_err());
    }
    assert!(StreamGuard::default().apply(Command::Finish).is_err());
}

#[test]
fn rejects_data_after_completion_and_buffer_overflow() {
    let mut guard = StreamGuard::default();
    assert_eq!(
        push(
            &mut guard,
            json!({"type":"response.completed","response":{"status":"completed","output":[]}})
        ),
        Ok(vec![])
    );
    assert!(push(&mut guard, json!({"type":"response.created"})).is_err());
    assert!(guard.apply(Command::Finish).is_err());
    let mut guard = StreamGuard::default();
    assert!(
        push(
            &mut guard,
            json!({"type":"unknown","data":"x".repeat(32 * 1024 * 1024)})
        )
        .is_err()
    );
}
