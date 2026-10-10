//! Verify MCP declarations through the public converter, without a model call.
use codex_tools::{ToolName, mcp_tool_to_responses_api_tool};
use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    tools: Vec<rmcp::model::Tool>,
    #[serde(default)]
    max_bytes: Option<usize>,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut input)?;
    if input.len() > 2 * 1024 * 1024 {
        return Err("schema verification input exceeds 2 MiB".into());
    }
    let request: Request = serde_json::from_slice(&input)?;
    if request.max_bytes == Some(0) || request.tools.len() > 1024 {
        return Err("positive maxBytes and at most 1024 tools are required".into());
    }
    let mut tools = Vec::with_capacity(request.tools.len());
    for tool in &request.tools {
        let name = tool.name.to_string();
        tools.push(mcp_tool_to_responses_api_tool(
            &ToolName::from(name.as_str()),
            tool,
            request.max_bytes,
        )?);
    }
    serde_json::to_writer(std::io::stdout().lock(), &json!({"tools":tools}))?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("MCP schema wire verification failed: {error}");
        std::process::exit(1);
    }
}
