use super::compile;
use crate::tokens;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn input() -> Value {
    json!({
        "modelCatalogPath": "model-catalog.json",
        "sessionPlan": { "defaultModel": "model", "routes": [{ "connectionId": "model", "authorizedExtensions": [] }],
            "allowedMcpServerIds": [], "allowedChildModels": [], "maxActiveChildren": 1 },
        "bridge": { "baseUrl": "http://127.0.0.1:12345", "credentialRef": "FIXTURE_TOKEN" },
        "mcpServers": []
    })
}

#[test]
fn developer_policy_is_escaped_at_root_without_changing_other_config() {
    let mut request = input();
    let baseline = compile(&request).unwrap();
    let policy = "公开进度使用中文。\n作者可明确要求\"English\"；不要修改代码\\引用。";
    request["developerInstructions"] = policy.into();
    let compiled = compile(&request).unwrap();
    let content = compiled.as_str().unwrap();
    let line = content
        .lines()
        .find(|line| line.starts_with("developer_instructions = "))
        .unwrap();
    let restored: String =
        serde_json::from_str(line.strip_prefix("developer_instructions = ").unwrap()).unwrap();
    assert_eq!(restored, policy);
    assert!(content.find(line).unwrap() < content.find("[features]").unwrap());
    assert_eq!(
        Value::String(content.replace(&format!("{line}\n"), "")),
        baseline
    );
    request["developerInstructions"] = Value::Null;
    assert_eq!(compile(&request).unwrap(), baseline);
}

#[test]
fn developer_policy_rejects_invalid_type_and_independent_byte_and_token_limits() {
    let mut request = input();
    for invalid in [json!(true), json!(42), json!([]), json!({"text": "中文"})] {
        request["developerInstructions"] = invalid;
        assert_eq!(
            compile(&request),
            Err("Codex developerInstructions must be a string or null".into())
        );
    }
    let byte_limited = "a".repeat(4097);
    assert!(tokens::count(&byte_limited) <= 1000);
    request["developerInstructions"] = byte_limited.into();
    assert_eq!(
        compile(&request),
        Err("Codex developerInstructions exceeds the 4KiB byte limit".into())
    );
    let token_limited = "a ".repeat(1100);
    assert!(token_limited.len() <= 4096 && tokens::count(&token_limited) > 1000);
    request["developerInstructions"] = token_limited.into();
    assert_eq!(
        compile(&request),
        Err("Codex developerInstructions exceeds the 1K-token conservative limit".into())
    );
    request["developerInstructions"] = "a".repeat(4096).into();
    assert!(compile(&request).is_ok());
}
