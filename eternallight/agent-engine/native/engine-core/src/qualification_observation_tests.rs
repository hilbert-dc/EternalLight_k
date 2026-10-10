use super::*;
use pretty_assertions::assert_eq;

#[test]
fn failed_qualification_preserves_bounded_startup_diagnostics_without_credentials() {
    let mut run = QualificationRun::new(json!({
        "scenario":"core", "command":["unused"], "baseInstructions":"diagnostic regression",
        "connections":[{"credentialRef":"TEST_PROVIDER_KEY"}],
        "baseEnvironment":{"TEST_PROVIDER_KEY":"private-provider-value"}
    })).unwrap();
    run.address = json!({"token":"private-bridge-value"});
    run.observe("event", &json!({"method":"engine/stderr","params":{"text":"invalid config: private-provider-value private-bridge-value"}})).unwrap();
    assert_eq!(run.failure_diagnostic(), None);
    run.error = Some("Codex App Server exited (1)".into());
    assert_eq!(run.failure_diagnostic().unwrap(), "Codex App Server exited (1): invalid config: [REDACTED] [REDACTED]");
    run.observe("event", &json!({"method":"engine/stderr","params":{"text":"诊断".repeat(1000)}})).unwrap();
    assert_eq!(run.failure_diagnostic().unwrap().chars().count(), 1000);
}

#[test]
fn qualification_preserves_textual_tool_call_evidence_without_treating_it_as_execution() {
    for (textual, expected) in [(true, "tool-call-as-text"), (false, "tool-not-selected")] {
        let mut run = QualificationRun::new(json!({
            "scenario":"message-running", "command":["unused"],
            "baseInstructions":"Local observation regression"
        }))
        .unwrap();
        run.observe(
            "request",
            &json!({
                "threadId":"child", "parentThreadId":"root",
                "tools":[{"namespace":"engine_conformance", "name":"engine_conformance_echo"}]
            }),
        )
        .unwrap();
        run.observe(
            "response",
            &json!({
                "threadId":"child", "parentThreadId":"root", "status":"completed",
                "toolCalls":[], "outputTypes":["message"], "textLooksLikeToolCall":textual
            }),
        )
        .unwrap();
        assert_eq!(
            trace::classify(&json!({
                "trace":run.trace.snapshot(),
                "options":{"expectedTool":"engine_conformance_echo", "requestScope":"child"}
            })),
            json!(expected)
        );
        assert!(!run.proof_ready());
    }
}
