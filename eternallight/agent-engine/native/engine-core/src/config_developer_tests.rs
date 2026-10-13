use super::compile;
use crate::responses_request;
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
fn developer_policy_rejects_invalid_type_without_arbitrary_local_capacity_limits() {
    let mut request = input();
    for invalid in [json!(true), json!(42), json!([]), json!({"text": "中文"})] {
        request["developerInstructions"] = invalid;
        assert_eq!(
            compile(&request),
            Err("Codex developerInstructions must be a string or null".into())
        );
    }
    for policy in ["中文创作规则。".repeat(300), "author policy ".repeat(1200)] {
        request["developerInstructions"] = policy.clone().into();
        let compiled = compile(&request).unwrap();
        let line = compiled.as_str().unwrap().lines().find(|line| line.starts_with("developer_instructions = ")).unwrap();
        let restored: String = serde_json::from_str(line.strip_prefix("developer_instructions = ").unwrap()).unwrap();
        assert_eq!(restored,policy);
        let wire = json!({"instructions":policy,"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"继续创作"}]}],"tools":[],"max_output_tokens":512});
        let mut connection = json!({"model":"model","protocol":"responses","endpoint":"https://example.invalid","declaredLimits":{"contextTokens":64000,"outputTokens":8192}});
        assert!(responses_request::encode(&wire,&connection,"fixture").is_ok());
        connection["declaredLimits"]["contextTokens"] = json!(1024);
        assert!(crate::context_capacity::diagnostic(&responses_request::encode(&wire,&connection,"fixture").unwrap_err()).is_some());
    }
}

fn mcp_input() -> Value {
    let mut request = input();
    request["sessionPlan"]["allowedMcpServerIds"] = json!(["workspace"]);
    request["sessionPlan"]["routes"][0]["outputBudget"] = json!(512);
    request["mcpServers"] = json!([{
        "id":"workspace", "command":"mcp", "required":true, "startupTimeoutMs":90000
    }]);
    request
}

#[test]
fn mcp_input_schema_byte_limit_compiles_without_changing_other_config() {
    let mut request = mcp_input();
    let baseline = compile(&request).unwrap();
    let original_plan = request["sessionPlan"].clone();
    assert!(!baseline.as_str().unwrap().contains("tool_input_schema_max_bytes"));
    for limit in [1, 5000, 40_000] {
        request["mcpServers"][0]["inputSchemaMaxBytes"] = json!(limit);
        let result = compile(&request).unwrap();
        let config = result.as_str().unwrap();
        let line = format!("tool_input_schema_max_bytes = {limit}\n");
        assert_eq!(config.matches(&line).count(), 1);
        assert_eq!(Value::String(config.replace(&line, "")), baseline);
        assert_eq!(request["sessionPlan"], original_plan);
    }
}

#[test]
fn mcp_input_schema_byte_limit_rejects_invalid_numbers_types_and_internal_alias() {
    let mut request = mcp_input();
    for invalid in [
        json!(0),
        json!(-1),
        json!(0.5),
        json!("40000"),
        json!("40000\n[other]"),
        json!(null),
        json!(true),
        json!([]),
        json!({}),
        json!(9_007_199_254_740_992u64),
    ] {
        request["mcpServers"][0]["inputSchemaMaxBytes"] = invalid;
        assert_eq!(
            compile(&request),
            Err("MCP inputSchemaMaxBytes must be a positive safe integer".into())
        );
    }
    let fields = request["mcpServers"][0].as_object_mut().unwrap();
    fields.remove("inputSchemaMaxBytes");
    fields.insert("tool_input_schema_max_bytes".into(), json!(40_000));
    assert_eq!(
        compile(&request),
        Err("MCP server configuration contains unsupported fields".into())
    );
}

#[test]
fn mcp_outputs_have_independent_per_tool_current_config() {
    let mut request = mcp_input();
    let baseline = compile(&request).unwrap();
    request["mcpServers"][0]["tools"] = json!({});
    assert_eq!(compile(&request).unwrap(), baseline);
    request["mcpServers"][0]["tools"] = json!({
        "read_problem":{"outputTokenLimit":8192},
        "other-tool":{"outputTokenLimit":512}
    });
    let result = compile(&request).unwrap();
    let config = result.as_str().unwrap();
    assert!(
        config.contains("[mcp_servers.workspace.tools.read_problem]\noutput_token_limit = 8192")
    );
    assert!(config.contains("[mcp_servers.workspace.tools.other-tool]\noutput_token_limit = 512"));
    assert!(config.contains("required = true\nstartup_timeout_sec = 90.000"));
    assert!(config.contains("model = \"model\""));
    assert_eq!(config.matches("output_token_limit = ").count(), 2);
    assert_eq!(request["sessionPlan"]["routes"][0]["outputBudget"], 512);
}

#[test]
fn mcp_output_policy_rejects_unknown_fields_and_invalid_integer_values() {
    let mut request = mcp_input();
    for tools in [json!(null), json!([]), json!("tools"), json!(true)] {
        request["mcpServers"][0]["tools"] = tools;
        assert!(compile(&request).unwrap_err().contains("MCP tools"));
    }
    for configuration in [
        json!(null),
        json!(8192),
        json!({}),
        json!({"output_token_limit":8192}),
        json!({"outputTokenLimit":8192,"approvalMode":"approve"}),
        json!({"outputTokenLimit":0}),
        json!({"outputTokenLimit":-1}),
        json!({"outputTokenLimit":0.5}),
        json!({"outputTokenLimit":"8192"}),
        json!({"outputTokenLimit":null}),
        json!({"outputTokenLimit":true}),
        json!({"outputTokenLimit":9_007_199_254_740_992u64}),
    ] {
        request["mcpServers"][0]["tools"] = json!({"read_problem":configuration});
        assert!(compile(&request).unwrap_err().starts_with("MCP"));
    }
    request["mcpServers"][0]
        .as_object_mut()
        .unwrap()
        .remove("tools");
    request["mcpServers"][0]["outputTokenLimit"] = json!(8192);
    assert_eq!(
        compile(&request),
        Err("MCP server configuration contains unsupported fields".into())
    );
}

#[test]
fn mcp_tool_names_and_map_are_bounded_before_generating_toml() {
    let mut request = mcp_input();
    for name in [
        "".to_owned(),
        "a".repeat(257),
        "tool.name".into(),
        "read\n[other]".into(),
        "read]".into(),
        "读问题".into(),
    ] {
        let mut tools = serde_json::Map::new();
        tools.insert(name, json!({"outputTokenLimit":8192}));
        request["mcpServers"][0]["tools"] = tools.into();
        assert!(compile(&request).unwrap_err().contains("tool name"));
    }
    let mut tools = serde_json::Map::new();
    for n in 0..1024 {
        tools.insert(format!("tool_{n}"), json!({"outputTokenLimit":1}));
    }
    request["mcpServers"][0]["tools"] = json!(tools);
    assert!(compile(&request).is_ok());
    tools.insert("one_more".into(), json!({"outputTokenLimit":1}));
    request["mcpServers"][0]["tools"] = tools.into();
    assert!(compile(&request).unwrap_err().contains("1024 entries"));
}

#[test]
fn default_mode_exposes_optional_user_input_without_enabling_plan_mode() {
    let compiled = compile(&input()).unwrap();
    let config = compiled.as_str().unwrap();
    assert!(config.contains("default_mode_request_user_input = true"));
    assert!(!config.contains("collaboration_mode = "));
}
