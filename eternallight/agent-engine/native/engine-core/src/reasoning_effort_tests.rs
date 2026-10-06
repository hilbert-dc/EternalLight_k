use crate::{connection, responses_request};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn definition() -> Value {
    json!({"id":"thinking", "provider":"provider", "model":"provider-model", "protocol":"responses",
        "endpoint":"https://example.invalid", "credentialRef":"MODEL_KEY",
        "declaredLimits":{"contextTokens":64000,"outputTokens":8192}})
}

fn request() -> Value {
    json!({"model":"logical", "instructions":"Original instructions", "stream":true, "max_output_tokens":8192,
        "tools":[], "input":[{"type":"reasoning","id":"rs_original","summary":[],"encrypted_content":"opaque-original"},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"作者原指令"}]}]})
}

#[test]
fn reasoning_effort_validates_only_declared_responses_connections() {
    for effort in ["none", "low", "high", "max"] {
        let mut definition = definition();
        definition["protocolOptions"] = json!({"reasoningEffort":effort});
        assert_eq!(
            connection::define(&definition).unwrap()["protocolOptions"],
            definition["protocolOptions"]
        );
    }
    for protocol in ["auto", "chat-completions", "messages", "generate-content"] {
        let mut definition = definition();
        definition["protocol"] = protocol.into();
        definition["protocolOptions"] = json!({"reasoningEffort":"low"});
        assert_eq!(
            connection::define(&definition).unwrap_err(),
            "protocolOptions.reasoningEffort requires the responses protocol"
        );
    }
    for effort in [
        json!("medium"),
        json!(null),
        json!(false),
        json!(1),
        json!({}),
    ] {
        let mut definition = definition();
        definition["protocolOptions"] = json!({"reasoningEffort":effort});
        assert_eq!(
            connection::define(&definition).unwrap_err(),
            "protocolOptions.reasoningEffort is invalid"
        );
    }
}

#[test]
fn reasoning_effort_defaults_without_rewriting_requests_or_history() {
    let mut definition = definition();
    definition["protocolOptions"] = json!({"reasoningEffort":"low","reasoningState":"native"});
    let connection = connection::define(&definition).unwrap();
    for reasoning in [
        None,
        Some(Value::Null),
        Some(json!({"summary":"auto"})),
        Some(json!({"effort":"high","summary":"auto"})),
        Some(json!({"effort":"none"})),
        Some(json!({"effort":null})),
    ] {
        let mut request = request();
        if let Some(reasoning) = reasoning {
            request["reasoning"] = reasoning;
        }
        let original = request.clone();
        let mut expected = request.clone();
        expected["model"] = "provider-model".into();
        if expected["reasoning"].is_null() {
            expected["reasoning"] = json!({"effort":"low"});
        } else if expected["reasoning"].get("effort").is_none() {
            expected["reasoning"]["effort"] = "low".into();
        }
        assert_eq!(
            responses_request::encode(&request, &connection, "fixture").unwrap()["body"],
            expected
        );
        assert_eq!(request, original);
    }
}

#[test]
fn reasoning_effort_unconfigured_keeps_the_original_wire() {
    let connection = connection::define(&definition()).unwrap();
    let request = request();
    let mut expected = request.clone();
    expected["model"] = "provider-model".into();
    assert_eq!(
        responses_request::encode(&request, &connection, "fixture").unwrap()["body"],
        expected
    );
}
