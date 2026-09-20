use eternallight_engine_core::{
    ExecutionTrace, dispatch, durable_replace_json, recovery_operation, report_store_operation,
};
use serde_json::json;

#[test]
fn wait_completion_and_child_final_answer_remain_distinct_proofs() {
    let input = json!([
        {"type":"function_call_output","call_id":"wait","output":"{\"timed_out\":false,\"message\":\"Wait completed.\"}"},
        {"type":"agent_message","author":"child","recipient":"parent","content":[{"type":"input_text","text":"Message Type: PROGRESS\nworking"}]}
    ]);
    assert_eq!(
        dispatch("message.wait", input.clone()).unwrap(),
        json!([{"callId":"wait","completed":true}])
    );
    assert_eq!(dispatch("message.final", input).unwrap(), json!([]));
    let final_message = json!([{"type":"agent_message","author":"child","recipient":"parent","content":[{"type":"input_text","text":"Message Type: FINAL_ANSWER\nfinished"}]}]);
    let hashes = dispatch("message.visible", json!({"input":final_message})).unwrap();
    assert_eq!(
        dispatch("message.final", final_message).unwrap(),
        json!([{"author":"child","recipient":"parent","textDigests":hashes}])
    );
}

#[test]
fn tool_arguments_and_encrypted_content_cannot_be_message_delivery_evidence() {
    let expected = dispatch("message.digest", json!("visible")).unwrap();
    assert_eq!(dispatch("message.visible",json!({"input":[
        {"content":[{"type":"input_text","text":"visible\nvisible"},{"type":"encrypted","text":"secret"}],"arguments":"secret"}
    ]})).unwrap(),json!([expected]));
}

#[test]
fn undefined_suite_capability_assertions_never_qualify() {
    assert_eq!(dispatch("proof.evaluate",json!({"record":{
        "capability":"childWait","measurementStatus":"passed","scope":{"protocol":"responses","suite":"codex-engine-delegation-lifecycle-v1","mode":"app-server-mcp","toolTypes":["function"]},"assertions":[]
    }})).unwrap(),json!({"status":"unsupported-suite","missingAssertions":[],"mismatchedDependencies":[]}));
}

#[test]
fn malformed_recovery_records_reject_without_panic_or_rewrite() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recovery.json");
    let malformed = json!({"version":2,"runs":{"run":{"requests":{"request":42}}}});
    durable_replace_json(&path, &malformed).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(recovery_operation(&path, "report", &json!({})).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn empty_receipt_does_not_clear_uncertain_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recovery.json");
    recovery_operation(&path, "begin", &json!({"runId":"run"})).unwrap();
    recovery_operation(
        &path,
        "record",
        &json!({"runId":"run","requestId":"write","kind":"write","state":"tool-dispatched"}),
    )
    .unwrap();
    recovery_operation(
        &path,
        "transition",
        &json!({"runId":"run","requestId":"write","state":"uncertain","receiptRef":""}),
    )
    .unwrap();
    assert_eq!(
        recovery_operation(
            &path,
            "terminal",
            &json!({"runId":"run","terminalAt":"now"})
        )
        .unwrap(),
        json!(false)
    );
}

#[test]
fn trace_state_is_owned_and_milestones_displace_deltas() {
    let mut trace = ExecutionTrace::new(&json!({"runId":"run","maxEvents":1})).unwrap();
    trace.record(&json!({"source":"agent","type":"agent.event","details":{"method":"textDelta"},"now":1})).unwrap();
    let milestone = trace.record(&json!({"source":"mcp","type":"mcp.call.completed","details":{"callId":"call"},"now":2})).unwrap();
    let mut detached = trace.snapshot();
    detached["events"] = json!([]);
    assert_eq!(
        trace.snapshot(),
        json!({"version":1,"runId":"run","events":[milestone],"droppedCount":1,"incomplete":true})
    );
}

#[test]
fn reports_bound_reads_and_validate_timestamp_before_using_it_as_a_filename() {
    let directory = tempfile::tempdir().unwrap();
    let id = "a".repeat(64);
    let report = json!({"reportId":id,"completedAt":"2026-09-21T00:00:00.000Z","payload":"test"});
    report_store_operation(
        directory.path(),
        "write",
        &json!({"report":report,"maxReportBytes":4096}),
    )
    .unwrap();
    assert_eq!(
        report_store_operation(
            directory.path(),
            "read",
            &json!({"reportId":id,"maxReportBytes":4096})
        )
        .unwrap(),
        report
    );
    assert!(
        report_store_operation(
            directory.path(),
            "read",
            &json!({"reportId":id,"maxReportBytes":8})
        )
        .unwrap_err()
        .contains("size limit")
    );
    let mut invalid = report;
    invalid["completedAt"] = json!("../../outside");
    assert!(
        report_store_operation(
            directory.path(),
            "write",
            &json!({"report":invalid,"maxReportBytes":4096})
        )
        .unwrap_err()
        .contains("completedAt")
    );
}

#[test]
fn recovery_reports_keep_creation_order_across_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recovery.json");
    for id in ["z-first", "a-second"] {
        recovery_operation(&path, "begin", &json!({"runId":id,"projectId":"project"})).unwrap();
    }
    assert_eq!(
        recovery_operation(&path, "unfinished", &json!({})).unwrap(),
        json!([{"runId":"z-first","projectId":"project"},{"runId":"a-second","projectId":"project"}])
    );
}
