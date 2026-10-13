use crate::{
    Result,
    canonical::{array, string},
    contracts,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn toml_array(value: &Value) -> String {
    format!(
        "[{}]",
        array(value)
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn mcp_tool_output_config(server: &Value) -> Result<Vec<String>> {
    let Some(tools) = server.get("tools") else {
        return Ok(Vec::new());
    };
    let tools = tools
        .as_object()
        .filter(|tools| tools.len() <= 1024)
        .ok_or("MCP tools must be an object containing at most 1024 entries")?;
    let mut lines = Vec::new();
    for (name, configuration) in tools {
        if name.is_empty()
            || name.len() > 256
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err("MCP tool name must be a bounded ASCII identifier".into());
        }
        let fields = configuration
            .as_object()
            .ok_or("MCP tool configuration must be an object")?;
        if fields.keys().any(|field| field != "outputTokenLimit") {
            return Err("MCP tool configuration contains unsupported fields".into());
        }
        let limit = fields
            .get("outputTokenLimit")
            .and_then(Value::as_u64)
            .filter(|limit| {
                *limit > 0 && *limit <= 9_007_199_254_740_991 && usize::try_from(*limit).is_ok()
            })
            .ok_or("MCP outputTokenLimit must be a positive safe integer")?;
        lines.extend([
            String::new(),
            format!("[mcp_servers.{}.tools.{name}]", string(&server["id"])),
            format!("output_token_limit = {limit}"),
        ]);
    }
    Ok(lines)
}

fn multi_agent(route: &Value) -> bool {
    array(&contracts::definitions()["DELEGATION_EXECUTION_CAPABILITIES"])
        .iter()
        .all(|capability| array(&route["authorizedExtensions"]).contains(capability))
}

pub(crate) fn catalog(input: &Value) -> Result<Value> {
    let instructions = string(&input["baseInstructions"]);
    if instructions.trim().is_empty() {
        return Err("Codex model catalog requires non-empty engine base instructions".into());
    }
    // A maintenance preference is distinct from request admission capacity.
    let compact_target = match input.get("autoCompactTargetTokens") {
        None => None,
        Some(value) => Some(
            value
                .as_u64()
                .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
                .ok_or("autoCompactTargetTokens must be a positive safe integer")?,
        ),
    };
    let mut models = Vec::new();
    for (index, route) in array(&input["plan"]["routes"]).iter().enumerate() {
        let enabled = |name: &str| {
            array(&route["authorizedExtensions"])
                .iter()
                .any(|c| c == name)
        };
        let output = route["outputBudget"]
            .as_u64()
            .ok_or("invalid route output budget")?;
        let input = route["inputBudget"]
            .as_u64()
            .ok_or("invalid route input budget")?;
        let context = input
            .checked_add(output)
            .ok_or("route token budget overflow")?;
        let mut model = json!({
            "slug":route["logicalModel"],"display_name":route["logicalModel"],"description":format!("{} {}",string(&route["provider"]),string(&route["model"])),
            "support_verbosity":false,"default_verbosity":null,"apply_patch_tool_type":"freeform","web_search_tool_type":"text",
            "input_modalities":if enabled("imageInput") { vec!["text","image"] } else { vec!["text"] },"supports_image_detail_original":false,
            // Tool results are input: a small generation allowance must not clip them.
            // This covers every UTF-8 byte admitted by the item transport guard.
            "truncation_policy":{"mode":"tokens","limit":262_144},"supports_parallel_tool_calls":enabled("parallelToolCalls"),"tool_mode":"direct",
            "multi_agent_version":if multi_agent(route) { Some("v2") } else { None },"use_responses_lite":false,
            "include_skills_usage_instructions":true,"include_apps_usage_instructions":false,"include_plugin_usage_instructions":false,
            "node_repl_auto_review_required":false,"node_repl_disabled":true,"auto_review_model_override":null,"model_specialty":null,
            // Core's provider-token history is smaller than the conservative
            // wire estimate. Reserve room for that difference, tool schemas
            // and the next complete task prompt before the hard input guard.
            "context_window":context,"max_context_window":context,"auto_compact_token_limit":compact_target.unwrap_or(u64::MAX).min((input / 5 * 3 + input % 5 * 3 / 5).max(1))
        });
        model.as_object_mut().unwrap().extend(json!({
            "comp_hash":null,"default_reasoning_summary":"none","supported_reasoning_levels":[],"shell_type":"unified_exec","visibility":"list","supported_in_api":true,
            "availability_nux":null,"upgrade":null,"priority":index,"base_instructions":instructions,"model_messages":null,"experimental_supported_tools":[],"available_in_plans":[],
            "supports_search_tool":false,"default_service_tier":null,"service_tiers":[],"additional_speed_tiers":[],
            "supports_reasoning_summary_parameter":enabled("reasoningSummary"),"supports_reasoning_summaries":enabled("reasoningSummary")
        }).as_object().unwrap().clone());
        models.push(model);
    }
    Ok(json!({"models":models}))
}

pub(crate) fn compile(input: &Value) -> Result<Value> {
    let compact_scope = match input.get("autoCompactTokenLimitScope") {
        None => None,
        Some(Value::String(scope)) if matches!(scope.as_str(), "total" | "body_after_prefix") => {
            Some(scope)
        }
        Some(_) => {
            return Err("autoCompactTokenLimitScope must be total or body_after_prefix".into());
        }
    };
    let post_turn_percent = match input.get("postTurnCompactThresholdPercent") {
        None => None,
        Some(value) => Some(
            value
                .as_u64()
                .filter(|percent| *percent <= 100)
                .ok_or("postTurnCompactThresholdPercent must be an integer from 0 to 100")?,
        ),
    };
    let developer_instructions = match input.get("developerInstructions") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let instructions = value
                .as_str()
                .ok_or("Codex developerInstructions must be a string or null")?;
            // Configuration preserves policy. Every provider request separately
            // admits the complete encoded input against the actual route budget.
            Some(instructions)
        }
    };
    if string(&input["modelCatalogPath"]).trim().is_empty() {
        return Err("Codex config requires a generated model catalog path".into());
    }
    if input.get("maxConcurrentChildren").is_some() {
        return Err("Codex config cannot override the frozen child concurrency limit".into());
    }
    let plan = &input["sessionPlan"];
    let route = array(&plan["routes"])
        .iter()
        .find(|r| r["connectionId"] == plan["defaultModel"])
        .ok_or("session plan has no default route")?;
    if !input["bridge"].is_object() {
        return Err("controlled bridge configuration is required by every session plan".into());
    }
    let servers = array(&input["mcpServers"]);
    let ids: BTreeSet<_> = servers.iter().map(|server| string(&server["id"])).collect();
    if ids.len() != servers.len() {
        return Err("Codex config MCP server ids must be unique".into());
    }
    if ids.iter().copied().collect::<Vec<_>>()
        != array(&plan["allowedMcpServerIds"])
            .iter()
            .map(string)
            .collect::<Vec<_>>()
    {
        return Err("Codex config MCP servers must exactly match the frozen SessionPlan".into());
    }
    if ids.iter().any(|id| {
        id.is_empty()
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    }) {
        return Err("Codex config MCP server id is invalid".into());
    }
    let enabled = multi_agent(route);
    let tool_search = array(&route["authorizedExtensions"])
        .iter()
        .any(|c| c == "toolSearch");
    let children: Vec<_> = array(&plan["allowedChildModels"])
        .iter()
        .filter(|model| **model != plan["defaultModel"])
        .collect();
    let usage = input["usageHintText"].as_str().unwrap_or("Delegate bounded independent work only when it materially improves the result. Wait for child evidence and synthesize the final answer.");
    let instructions = input["subagentDeveloperInstructions"].as_str().unwrap_or("Work only on the assigned scope. Return concise evidence to the parent agent and do not claim side effects that no tool result confirms.");
    let sensitive = input
        .get("sensitiveEnvironmentPatterns")
        .cloned()
        .unwrap_or_else(|| {
            json!([
                "*_API_KEY",
                "*_ACCESS_TOKEN",
                "CODEX_API_KEY",
                "CODEX_ACCESS_TOKEN"
            ])
        });
    let mut lines = vec![
        "# Generated from an immutable Agent Engine SessionPlan.".into(),
        format!("model = {}", plan["defaultModel"]),
        "model_provider = \"agent_engine\"".into(),
        format!("model_catalog_json = {}", input["modelCatalogPath"]),
        "web_search = \"disabled\"".into(),
        "approval_policy = \"never\"".into(),
        "sandbox_mode = \"read-only\"".into(),
        "cli_auth_credentials_store = \"ephemeral\"".into(),
    ];
    if let Some(instructions) = developer_instructions {
        lines.push(format!("developer_instructions = {}", json!(instructions)));
    }
    if let Some(scope) = compact_scope {
        lines.push(format!(
            "model_auto_compact_token_limit_scope = {}",
            json!(scope)
        ));
    }
    if let Some(percent) = post_turn_percent {
        lines.push(format!(
            "model_post_turn_compact_threshold_percent = {percent}"
        ));
    }
    lines.extend([
        String::new(),
        "[features]".into(),
        "default_mode_request_user_input = true".into(),
        format!("multi_agent = {enabled}"),
        format!("subagent_default_context_limits = {enabled}"),
        format!("tool_search = {tool_search}"),
    ]);
    for feature in [
        "memories",
        "apps",
        "plugins",
        "recommended_plugins",
        "plugin_sharing",
        "plugin_hooks",
        "remote_plugin",
        "computer_use",
        "in_app_browser",
        "image_generation",
        "js_repl",
    ] {
        lines.push(format!("{feature} = false"));
    }
    lines.extend([
        String::new(),
        "[features.multi_agent_v2]".into(),
        format!("enabled = {enabled}"),
        format!("preserve_fork_prefix = {enabled}"),
        "non_code_mode_only = false".into(),
        "hide_spawn_agent_metadata = false".into(),
        format!(
            "expose_spawn_agent_model_overrides = {}",
            !children.is_empty()
        ),
        format!("root_agent_usage_hint_text = {}", json!(usage)),
        format!("subagent_developer_instructions = {}", json!(instructions)),
    ]);
    // Mode and role guidance are separate settings in the current Codex core.
    if let Some(mode) = input["multiAgentModeHintText"].as_str() {
        lines.push(format!("multi_agent_mode_hint_text = {}", json!(mode)));
    }
    lines.extend([
        String::new(),
        "[agents]".into(),
        format!("enabled = {enabled}"),
        format!(
            "max_concurrent_threads_per_session = {}",
            plan["maxActiveChildren"]
                .as_u64()
                .ok_or("invalid child capacity")?
                .saturating_add(1)
        ),
    ]);
    if children.len() == 1 {
        lines.push(format!("default_subagent_model = {}", children[0]));
    }
    lines.extend([
        String::new(),
        "[shell_environment_policy]".into(),
        "inherit = \"core\"".into(),
        "ignore_default_excludes = false".into(),
        format!("exclude = {}", toml_array(&sensitive)),
        String::new(),
        "[model_providers.agent_engine]".into(),
        "name = \"Eternal Light Agent Engine\"".into(),
        format!("base_url = {}", input["bridge"]["baseUrl"]),
        "wire_api = \"responses\"".into(),
        format!("env_key = {}", input["bridge"]["credentialRef"]),
    ]);
    for server in servers {
        let fields = server
            .as_object()
            .ok_or("MCP server configuration must be an object")?;
        if fields.keys().any(|field| {
            !matches!(
                field.as_str(),
                "id" | "command"
                    | "args"
                    | "envVars"
                    | "approvalMode"
                    | "required"
                    | "startupTimeoutMs"
                    | "inputSchemaMaxBytes"
                    | "tools"
            )
        }) {
            return Err("MCP server configuration contains unsupported fields".into());
        }
        let tool_lines = mcp_tool_output_config(server)?;
        let required = match server.get("required") {
            None => false,
            Some(Value::Bool(required)) => *required,
            Some(_) => return Err("MCP required must be a boolean".into()),
        };
        let startup_timeout = match server.get("startupTimeoutMs") {
            None => None,
            Some(timeout) => Some(
                timeout
                    .as_u64()
                    .filter(|ms| (1..=300_000).contains(ms))
                    .ok_or("MCP startupTimeoutMs must be an integer between 1 and 300000")?,
            ),
        };
        let input_schema_max_bytes = match server.get("inputSchemaMaxBytes") {
            None => None,
            Some(limit) => Some(
                limit
                    .as_u64()
                    .filter(|limit| {
                        *limit > 0
                            && *limit <= 9_007_199_254_740_991
                            && usize::try_from(*limit).is_ok()
                    })
                    .ok_or("MCP inputSchemaMaxBytes must be a positive safe integer")?,
            ),
        };
        let args = Value::Array(array(&server["args"]).to_vec());
        let vars = Value::Array(array(&server["envVars"]).to_vec());
        let approval = server["approvalMode"].as_str().unwrap_or("approve");
        lines.extend([
            String::new(),
            format!("[mcp_servers.{}]", string(&server["id"])),
            format!("command = {}", server["command"]),
            format!("args = {}", toml_array(&args)),
            format!("env_vars = {}", toml_array(&vars)),
            format!("default_tools_approval_mode = {}", json!(approval)),
            format!("required = {required}"),
        ]);
        if let Some(timeout) = startup_timeout {
            let seconds = timeout / 1000;
            let milliseconds = timeout % 1000;
            lines.push(format!("startup_timeout_sec = {seconds}.{milliseconds:03}"));
        }
        if let Some(limit) = input_schema_max_bytes {
            lines.push(format!("tool_input_schema_max_bytes = {limit}"));
        }
        lines.extend(tool_lines);
    }
    Ok(format!("{}\n", lines.join("\n")).into())
}

#[cfg(test)]
#[path = "config_developer_tests.rs"]
mod tests;
