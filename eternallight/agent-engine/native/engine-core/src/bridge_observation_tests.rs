use super::response;
use crate::message_evidence;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn final_answer_digest_excludes_reasoning_and_preserves_deliverable_lines() {
    let expected = "CHILD_TERMINAL:nonce";
    let digest = message_evidence::dispatch("message.digest", &json!(expected)).unwrap();
    let observed = response(
        &json!({"threadId":"child"}),
        &json!({
            "id": "response-1",
            "status": "completed",
            "output": [
                {"type":"reasoning","content":[{"type":"reasoning_text","text":"private reasoning"}]},
                {"type":"message","content":[{"type":"output_text","text":expected}]}
            ]
        }),
    );
    assert_eq!(
        json!({
            "threadId": observed["threadId"],
            "finalTextDigest": observed["finalTextDigest"],
            "finalTextLineDigests": observed["finalTextLineDigests"],
            "textCharacters": observed["textCharacters"]
        }),
        json!({
            "threadId": "child",
            "finalTextDigest": digest,
            "finalTextLineDigests": [digest],
            "textCharacters": expected.len()
        })
    );
}

#[test]
fn reasoning_only_response_is_not_a_deliverable_child_result() {
    let observed = response(
        &json!({"threadId":"child"}),
        &json!({
            "status": "completed",
            "output": [{"type":"reasoning","content":[{"type":"reasoning_text","text":"still thinking"}]}]
        }),
    );
    assert_eq!(
        json!({
            "finalTextDigest": observed["finalTextDigest"],
            "finalTextLineDigests": observed["finalTextLineDigests"]
        }),
        json!({"finalTextDigest":null,"finalTextLineDigests":[]})
    );
}

#[test]
fn numeric_metrics_reflect_encoded_request_without_exporting_content() {
    let body = json!({"max_tokens":8192,"messages":[{"role":"user","content":"PRIVATE_PROSE🙂"}],"tools":[{"name":"PRIVATE_TOOL"}]});
    let observed = super::request(&json!({}), &json!({"max_output_tokens":9000}), &json!({"body":body}));
    let metrics = &observed["requestMetrics"];
    assert_eq!(metrics["maxTokens"],8192);
    assert_eq!(metrics["maxOutputTokens"],serde_json::Value::Null);
    assert_eq!(metrics["bodyBytes"],serde_json::to_vec(&body).unwrap().len());
    assert!(!metrics.to_string().contains("PRIVATE"));
}

#[test]
fn numeric_usage_rejects_invalid_counts_and_preserves_unknown() {
    let actual = super::usage(&json!({"input_tokens":11,"output_tokens":7,"total_tokens":18,
        "input_tokens_details":{"cached_tokens":0,"private":"SECRET"},"output_tokens_details":{"reasoning_tokens":3}}));
    assert_eq!(actual,json!({"inputTokens":11,"outputTokens":7,"totalTokens":18,"cachedTokens":0,"reasoningTokens":3}));
    for invalid in [json!(-1),json!(1.5),json!("SECRET"),json!(9007199254740992_u64),serde_json::Value::Null] {
        assert_eq!(super::usage(&json!({"input_tokens":invalid}))["inputTokens"],serde_json::Value::Null);
    }
    assert_eq!(super::usage(&json!({}))["reasoningTokens"],serde_json::Value::Null);
}
