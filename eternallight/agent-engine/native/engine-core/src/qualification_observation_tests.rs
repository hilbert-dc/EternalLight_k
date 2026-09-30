use super::*;
use pretty_assertions::assert_eq;

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
