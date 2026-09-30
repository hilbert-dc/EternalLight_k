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
