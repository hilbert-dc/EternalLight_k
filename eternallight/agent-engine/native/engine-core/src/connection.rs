use crate::{
    Result,
    canonical::{array, string},
    contracts,
};
use serde_json::{Value, json};

pub(crate) fn endpoint(value: &Value) -> Result<Value> {
    let mut url = url::Url::parse(string(value))
        .map_err(|_| "connection endpoint must be an absolute URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("connection endpoint must be a clean HTTP(S) URL".into());
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url.to_string().trim_end_matches('/').into())
}

pub(crate) fn define(input: &Value) -> Result<Value> {
    if !input.is_object() {
        return Err("connection must be an object".into());
    }
    let id = string(&input["id"]).trim().to_lowercase();
    if id.is_empty()
        || id.len() > 64
        || !id.as_bytes()[0].is_ascii_lowercase()
        || !id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
    {
        return Err("connection id is invalid".into());
    }
    let provider = input["provider"].as_str().unwrap_or(&id).trim();
    let model = string(&input["model"]).trim();
    for (value, name, limit) in [(provider, "provider", 80), (model, "model", 160)] {
        if value.is_empty() || value.encode_utf16().count() > limit || value.contains(['\r', '\n'])
        {
            return Err(format!("connection {name} is invalid"));
        }
    }
    let credential = string(&input["credentialRef"]).trim();
    if credential.len() < 2
        || credential.len() > 128
        || !credential.as_bytes()[0].is_ascii_uppercase()
        || !credential
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
    {
        return Err("credentialRef must name an environment variable".into());
    }
    let protocol = input["protocol"].as_str().unwrap_or("auto").trim();
    if protocol != "auto"
        && !array(&contracts::definitions()["PROTOCOLS"])
            .iter()
            .any(|p| p == protocol)
    {
        return Err(format!("unsupported protocol: {protocol}"));
    }
    let mut limits = json!({});
    for key in ["contextTokens", "outputTokens"] {
        if let Some(value) = input["declaredLimits"].get(key) {
            if value.as_u64().is_none_or(|v| v == 0) {
                return Err(format!("{key} must be a positive integer"));
            }
            limits[key] = value.clone();
        }
    }
    let options = input
        .get("protocolOptions")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !options.is_object() {
        return Err("protocolOptions must be an object".into());
    }
    for (key, values) in [
        (
            "maxOutputTokensField",
            &["max_tokens", "max_completion_tokens"][..],
        ),
        ("toolChoiceMode", &["full", "auto-only", "none"][..]),
        (
            "reasoningState",
            &["native", "assistant-message", "none"][..],
        ),
        ("agentMessageMode", &["native", "message"][..]),
        ("namespaceToolsMode", &["native", "flatten", "none"][..]),
        (
            "developerRoleMode",
            &["native", "system", "instructions", "unsupported"][..],
        ),
    ] {
        if let Some(value) = options.get(key)
            && !values.contains(&string(value))
        {
            return Err(format!("protocolOptions.{key} is invalid"));
        }
    }
    if options.get("developerRoleMode").is_some() && protocol != "responses" {
        return Err("protocolOptions.developerRoleMode requires the responses protocol".into());
    }
    if let Some(effort) = options.get("reasoningEffort") {
        if protocol != "responses" {
            return Err("protocolOptions.reasoningEffort requires the responses protocol".into());
        }
        if !matches!(effort.as_str(), Some("none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max")) {
            return Err("protocolOptions.reasoningEffort is invalid".into());
        }
    }
    if let Some(allowed) = options.get("allowedReasoningEfforts") {
        let values = allowed.as_array().filter(|values| !values.is_empty() && values.len() <= 7)
            .ok_or("protocolOptions.allowedReasoningEfforts must be a nonempty bounded array")?;
        if protocol != "responses" || options.get("reasoningEffort").is_none()
            || !values.contains(&options["reasoningEffort"])
            || values.iter().any(|value| !matches!(value.as_str(), Some("none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max")))
        {
            return Err("protocolOptions.allowedReasoningEfforts requires a Responses default included in the allowed efforts".into());
        }
    }
    let support = input
        .get("capabilitySupport")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    for (capability, mode) in support
        .as_object()
        .ok_or("capabilitySupport must be an object")?
    {
        if contracts::domain(capability) != Ok("model") {
            return Err(format!(
                "capabilitySupport must name a model capability: {capability}"
            ));
        }
        if !array(&contracts::definitions()["CAPABILITY_SUPPORT_MODES"]).contains(mode) {
            return Err(format!("capabilitySupport mode is invalid: {mode}"));
        }
    }
    Ok(
        json!({"id":id,"provider":provider,"model":model,"credentialRef":credential,"protocol":protocol,"endpoint":endpoint(&input["endpoint"])? ,"declaredLimits":limits,"protocolOptions":options,"capabilitySupport":support}),
    )
}

#[cfg(test)]
#[path = "reasoning_effort_tests.rs"]
mod tests;
