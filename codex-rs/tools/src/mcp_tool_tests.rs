use super::mcp_call_tool_result_output_schema;
use super::parse_agent_plugin_mcp_tool;
use super::parse_mcp_tool;
use crate::ToolDefinition;
use pretty_assertions::assert_eq;

fn mcp_tool(name: &str, description: &str, input_schema: serde_json::Value) -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        name.to_string(),
        description.to_string(),
        std::sync::Arc::new(rmcp::model::object(input_schema)),
    )
}

#[test]
fn mcp_schema_max_bytes_rejects_overflow_without_modifying_the_contract() {
    let input_schema = serde_json::json!({
        "type": "object",
        "properties": {"query": {"type": "string", "description": "parameter guidance é ".repeat(400)}},
        "required": ["query"],
        "additionalProperties": false,
    });
    let tool = mcp_tool("search", "Search", input_schema.clone());
    let schema_bytes = serde_json::to_vec(&input_schema)
        .expect("schema JSON")
        .len();
    let refused = super::parse_mcp_tool_with_schema_max_bytes(&tool, schema_bytes - 1)
        .expect_err("a byte allowance cannot authorize removing schema content");
    assert!(refused.to_string().contains("MCP tool search input schema"));
    assert!(refused.to_string().contains("byte limit"));
    let expanded = super::parse_mcp_tool_with_schema_max_bytes(&tool, schema_bytes)
        .expect("parameter description fits");
    assert_eq!(
        serde_json::to_value(expanded.input_schema).expect("schema"),
        input_schema
    );
}

#[test]
fn parse_mcp_tool_preserves_a_type_only_declaration() {
    let tool = mcp_tool(
        "no_props",
        "No properties",
        serde_json::json!({
            "type": "object"
        }),
    );

    assert_eq!(
        parse_mcp_tool(&tool).expect("parse MCP tool"),
        ToolDefinition {
            name: "no_props".to_string(),
            description: "No properties".to_string(),
            input_schema: serde_json::from_value(serde_json::json!({"type":"object"}))
                .expect("type-only declaration"),
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({})).into()),
            defer_loading: false,
        }
    );
}

#[test]
fn agent_plugin_mcp_tool_bounds_model_visible_description() {
    let description = format!("{}é", "a".repeat(super::MAX_MCP_TOOL_DESCRIPTION_BYTES - 1));
    let tool = mcp_tool(
        "bounded_description",
        &description,
        serde_json::json!({"type": "object"}),
    );

    let parsed = parse_agent_plugin_mcp_tool(&tool).expect("parse Agent Plugin MCP tool");

    assert_eq!(
        parsed.description.len(),
        super::MAX_MCP_TOOL_DESCRIPTION_BYTES - 1
    );
}

#[test]
fn mcp_tool_preserves_long_description() {
    let description = "a".repeat(super::MAX_MCP_TOOL_DESCRIPTION_BYTES + 100);
    let tool = mcp_tool(
        "legacy_description",
        &description,
        serde_json::json!({"type": "object"}),
    );

    assert_eq!(
        parse_mcp_tool(&tool)
            .expect("parse legacy MCP tool")
            .description,
        description
    );
}

#[test]
fn parse_mcp_tool_preserves_top_level_output_schema() {
    let mut tool = mcp_tool(
        "with_output",
        "Has output schema",
        serde_json::json!({
            "type": "object"
        }),
    );
    tool.output_schema = Some(std::sync::Arc::new(rmcp::model::object(
        serde_json::json!({
            "properties": {
                "result": {
                    "properties": {
                        "nested": {}
                    }
                }
            },
            "required": ["result"]
        }),
    )));

    assert_eq!(
        parse_mcp_tool(&tool).expect("parse MCP tool"),
        ToolDefinition {
            name: "with_output".to_string(),
            description: "Has output schema".to_string(),
            input_schema: serde_json::from_value(serde_json::json!({"type":"object"}))
                .expect("type-only declaration"),
            output_schema: Some(
                mcp_call_tool_result_output_schema(serde_json::json!({
                    "properties": {
                        "result": {
                            "properties": {
                                "nested": {}
                            }
                        }
                    },
                    "required": ["result"]
                }))
                .into()
            ),
            defer_loading: false,
        }
    );
}

#[test]
fn parse_mcp_tool_preserves_output_schema_without_inferred_type() {
    let mut tool = mcp_tool(
        "with_enum_output",
        "Has enum output schema",
        serde_json::json!({
            "type": "object"
        }),
    );
    tool.output_schema = Some(std::sync::Arc::new(rmcp::model::object(
        serde_json::json!({
            "enum": ["ok", "error"]
        }),
    )));

    assert_eq!(
        parse_mcp_tool(&tool).expect("parse MCP tool"),
        ToolDefinition {
            name: "with_enum_output".to_string(),
            description: "Has enum output schema".to_string(),
            input_schema: serde_json::from_value(serde_json::json!({"type":"object"}))
                .expect("type-only declaration"),
            output_schema: Some(
                mcp_call_tool_result_output_schema(serde_json::json!({
                    "enum": ["ok", "error"]
                }))
                .into()
            ),
            defer_loading: false,
        }
    );
}

#[test]
fn mcp_tool_preserves_unknown_constraints_and_conditional_schemas_exactly() {
    let schema = serde_json::json!({
        "type":"object","additionalProperties":false,
        "properties":{
            "id":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$","minLength":1,"maxLength":160},
            "count":{"type":"integer","minimum":1,"maximum":100,"exclusiveMinimum":0,"exclusiveMaximum":101},
            "evidence":{"type":"array","minItems":1,"maxItems":4,"uniqueItems":true,"items":{"enum":["first","second"]}},
            "status":{"enum":["supported","unmet"]},"start":{"type":"integer"},"end":{"type":"integer"}
        },
        "required":["id","count","status"],
        "dependentRequired":{"start":["end"],"end":["start"]},
        "allOf":[{"if":{"properties":{"status":{"const":"supported"}}},"then":{"required":["evidence"]},"else":{"not":{"required":["evidence"]}}}],
        "x-provenance":{"version":1,"constraints":["do not simplify"]}
    });
    let tool = mcp_tool("constrained", "Original contract", schema.clone());
    let parsed = parse_mcp_tool(&tool).expect("current contract is representable");
    assert_eq!(
        serde_json::to_value(parsed.input_schema).expect("schema"),
        schema
    );
}

#[test]
fn mcp_tool_rejects_invalid_or_lossy_shapes_instead_of_advertising_fallbacks() {
    for schema in [
        serde_json::json!({"type":"unknown-type"}),
        serde_json::json!({"type":"object","properties":null}),
        serde_json::json!({"type":"object","required":[1]}),
        serde_json::json!({"type":"object","properties":{"value":false}}),
        serde_json::json!({"type":"object","properties":{"value":{"type":null}}}),
    ] {
        let error = parse_mcp_tool(&mcp_tool("invalid", "Invalid declaration", schema))
            .expect_err("invalid or unrepresentable declarations must not be published");
        assert!(error.to_string().contains("MCP tool invalid input schema"));
    }
}
