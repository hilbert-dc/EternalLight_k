//! Portable schema selection. A discovery receipt selects identities from the
//! current admitted catalog; it never supplies schemas or execution authority.
use crate::{Result, digest};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const MAX_TOOLS: usize = 1024;
const MAX_PENDING: usize = 32;
const MAX_SCOPES: usize = 1024;

fn identifier(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 256
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
        .ok_or_else(|| "tool discovery identifier is invalid".into())
}
fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or("tool discovery object is required")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("tool discovery contains unsupported fields".into());
    }
    Ok(())
}
fn key(namespace: &str, name: &str) -> String {
    json!([namespace, name]).to_string()
}
fn identity(value: &Value) -> Result<(String, String)> {
    fields(value, &["namespace", "name"])?;
    Ok((
        identifier(&value["namespace"])?.into(),
        identifier(&value["name"])?.into(),
    ))
}

/// Compile only trusted embedding policy. Malformed configuration is an error,
/// never a request to expose the full catalog.
pub(crate) fn configuration(value: &Value) -> Result<Value> {
    fields(
        value,
        &[
            "version",
            "deferredNamespaces",
            "discoveryTools",
            "initialTools",
        ],
    )?;
    if value["version"] != 1 {
        return Err("unsupported portable tool discovery version".into());
    }
    let namespaces = value["deferredNamespaces"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .ok_or("deferred tool namespaces must contain 1 to 64 entries")?;
    let mut deferred = BTreeSet::new();
    for ns in namespaces {
        if !deferred.insert(identifier(ns)?.to_owned()) {
            return Err("duplicate deferred namespace".into());
        }
    }
    let mut normalized =
        json!({"version":1,"deferredNamespaces":deferred,"discoveryTools":[],"initialTools":[]});
    for field in ["discoveryTools", "initialTools"] {
        let tools = value[field]
            .as_array()
            .filter(|v| v.len() <= MAX_TOOLS)
            .ok_or("tool discovery identities must be a bounded array")?;
        if field == "discoveryTools" && tools.is_empty() {
            return Err("a discovery tool is required".into());
        }
        let mut unique = BTreeSet::new();
        for tool in tools {
            let (ns, name) = identity(tool)?;
            if !deferred.contains(&ns) {
                return Err("discovery identity must belong to a deferred namespace".into());
            }
            if !unique.insert(key(&ns, &name)) {
                return Err("duplicate tool discovery identity".into());
            }
        }
        normalized[field] = tools.clone().into();
    }
    Ok(normalized)
}

#[derive(Clone)]
struct Pending {
    namespace: String,
    name: String,
    arguments: String,
}
#[derive(Default)]
struct Scope {
    catalog: String,
    config: String,
    active: BTreeSet<String>,
    declarations: BTreeMap<String, String>,
    pending: BTreeMap<String, Pending>,
}
#[derive(Default)]
pub(crate) struct PortableDiscovery {
    scopes: HashMap<String, Scope>,
}

fn caller(value: &Value) -> Result<String> {
    let thread = value["threadId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or("portable discovery requires trusted caller identity")?;
    for field in ["parentThreadId", "sessionId"] {
        if !value[field].is_null()
            && value[field]
                .as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 256)
        {
            return Err("portable discovery caller identity is invalid".into());
        }
    }
    Ok(json!([thread, value["parentThreadId"], value["sessionId"]]).to_string())
}
fn arguments(value: &Value) -> Result<String> {
    let parsed: Value = if let Some(s) = value.as_str() {
        if s.len() > 38_000 {
            return Err("discovery arguments exceed item limit".into());
        }
        serde_json::from_str(s).map_err(|_| "discovery call arguments are not JSON")?
    } else {
        value.clone()
    };
    if !parsed.is_object() {
        return Err("discovery arguments must be an object".into());
    }
    Ok(digest::digest(&parsed))
}

// Codex MCP output has an exact transport timing header, followed by JSON.
// Do not search arbitrary prose for a JSON fragment or a capability name.
fn json_text(text: &str) -> Option<Value> {
    if text.len() > 38_000 {
        return None;
    }
    let text = text.trim();
    let text = if let Some(rest) = text.strip_prefix("Wall time: ") {
        let (seconds, rest) = rest.split_once(" seconds\nOutput:")?;
        if seconds.is_empty() || !seconds.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return None;
        }
        rest.trim()
    } else {
        text
    };
    serde_json::from_str(text).ok()
}
fn output(value: &Value) -> Option<Value> {
    if let Some(text) = value.as_str() {
        return json_text(text);
    }
    if let Some(parts) = value.as_array() {
        // MCP content-items put the timing header in its own first item.
        let mut text = String::new();
        for part in parts {
            if !matches!(part["type"].as_str(), Some("input_text" | "text")) {
                return None;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(part["text"].as_str()?);
            if text.len() > 38_000 {
                return None;
            }
        }
        return json_text(&text);
    }
    if value.is_object() {
        Some(value.clone())
    } else {
        None
    }
}
fn discovery_data(value: &Value) -> Option<Value> {
    if value["isError"] == true || value.get("error").is_some_and(|v| !v.is_null()) {
        return None;
    }
    if value.get("toolDiscovery").is_some() {
        return Some(value["toolDiscovery"].clone());
    }
    if let Some(content) = value.get("content") {
        return output(content).and_then(|v| discovery_data(&v));
    }
    None
}

impl PortableDiscovery {
    pub(crate) fn reset(&mut self) {
        self.scopes.clear();
    }
    // Pending receipts belong to an execution; loaded schemas belong to the
    // caller session and are revalidated against the next admitted catalog.
    pub(crate) fn begin_execution(&mut self) {
        for scope in self.scopes.values_mut() {
            scope.pending.clear();
        }
    }
    pub(crate) fn snapshot(&self, who: &Value) -> Result<Value> {
        Ok(self
            .scopes
            .get(&caller(who)?)
            .map(|scope| Value::from(scope.catalog.clone()))
            .unwrap_or(Value::Null))
    }

    pub(crate) fn prepare(
        &mut self,
        mut body: Value,
        config: &Value,
        who: &Value,
    ) -> Result<Value> {
        if config.is_null() {
            return Ok(body);
        }
        let config = configuration(config)?;
        let owner = caller(who)?;
        if who["requestKind"] == "compaction" {
            if body["tools"].as_array().is_none_or(|v| !v.is_empty()) {
                return Err("trusted compaction request cannot expose tools".into());
            }
            return Ok(body);
        }
        let deferred: BTreeSet<_> = config["deferredNamespaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let initial: BTreeSet<_> = ["initialTools", "discoveryTools"]
            .into_iter()
            .flat_map(|field| config[field].as_array().unwrap())
            .map(|v| {
                key(
                    v["namespace"].as_str().unwrap(),
                    v["name"].as_str().unwrap(),
                )
            })
            .collect();
        let tools = body["tools"].as_array().cloned().unwrap_or_default();
        let mut catalog = BTreeSet::new();
        let mut declarations = BTreeMap::new();
        let mut count = 0;
        for tool in &tools {
            let entries = if tool["type"] == "namespace" {
                tool["tools"]
                    .as_array()
                    .ok_or("namespace tools are missing")?
            } else {
                continue;
            };
            let ns = tool["name"]
                .as_str()
                .ok_or("namespace identity is missing")?;
            let mut namespace = tool.clone();
            namespace.as_object_mut().unwrap().remove("tools");
            for entry in entries {
                count += 1;
                if count > MAX_TOOLS {
                    return Err("portable discovery catalog exceeds 1024 tools".into());
                }
                if deferred.contains(ns) {
                    let name = identifier(&entry["name"])?;
                    if entry["type"] != "function" || !catalog.insert(key(ns, name)) {
                        return Err("invalid or duplicate deferred tool".into());
                    }
                    declarations.insert(key(ns, name), digest::digest(&json!([namespace, entry])));
                }
            }
        }
        // Fingerprint schemas and namespace metadata, not only tool identities.
        let fingerprint = digest::digest(&json!({"tools":tools,"config":config}));
        if !self.scopes.contains_key(&owner) && self.scopes.len() >= MAX_SCOPES {
            return Err("portable discovery caller limit reached".into());
        }
        let scope = self.scopes.entry(owner).or_default();
        let config_hash = digest::digest(&config);
        if scope.catalog != fingerprint || scope.config != config_hash {
            let active = if scope.config == config_hash {
                scope.active.iter().filter(|id| {
                    declarations.get(*id).is_some_and(|definition|
                        scope.declarations.get(*id) == Some(definition))
                }).cloned().collect()
            } else {
                BTreeSet::new()
            };
            *scope = Scope {
                catalog: fingerprint,
                config: config_hash,
                active,
                declarations,
                ..Scope::default()
            };
        }
        let input = body["input"].as_array().cloned().unwrap_or_default();
        let mut activated = scope.active.clone();
        let mut consumed = Vec::new();
        for item in &input {
            if item["type"] != "function_call_output" {
                continue;
            }
            let Some(call_id) = item["call_id"].as_str() else {
                continue;
            };
            let Some(pending) = scope.pending.get(call_id) else {
                continue;
            };
            let calls: Vec<_> = input
                .iter()
                .filter(|call| call["type"] == "function_call" && call["call_id"] == call_id)
                .collect();
            if calls.len() != 1
                || calls[0]["name"] != pending.name
                || calls[0]["namespace"] != pending.namespace
                || arguments(&calls[0]["arguments"])? != pending.arguments
            {
                return Err("discovery receipt does not match the actual caller tool call".into());
            }
            // A failed/no-tools discovery stays readable and cannot activate schemas.
            let Some(data) = output(&item["output"]).and_then(|v| discovery_data(&v)) else {
                consumed.push(call_id.to_owned());
                continue;
            };
            fields(&data, &["version", "tools"])?;
            if data["version"] != 1 {
                return Err("unsupported discovery receipt version".into());
            }
            let selected = data["tools"]
                .as_array()
                .filter(|v| v.len() <= MAX_TOOLS)
                .ok_or("invalid discovery receipt tools")?;
            for selected in selected {
                fields(selected, &["namespace", "name"])?;
                let name = identifier(&selected["name"])?;
                let ns = if selected["namespace"].is_null() {
                    pending.namespace.as_str()
                } else {
                    identifier(&selected["namespace"])?
                };
                if ns != pending.namespace || !catalog.contains(&key(ns, name)) {
                    return Err("discovered tool is outside the current admitted catalog".into());
                }
                activated.insert(key(ns, name));
            }
            consumed.push(call_id.to_owned());
        }
        let filtered: Vec<_> = tools
            .into_iter()
            .filter_map(|mut tool| {
                if tool["type"] != "namespace"
                    || !deferred.contains(tool["name"].as_str().unwrap_or(""))
                {
                    return Some(tool);
                }
                let ns = tool["name"].as_str().unwrap();
                let selected: Vec<_> = tool["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|entry| {
                        let id = key(ns, entry["name"].as_str().unwrap());
                        initial.contains(&id) || activated.contains(&id)
                    })
                    .cloned()
                    .collect();
                if selected.is_empty() {
                    None
                } else {
                    tool["tools"] = selected.into();
                    Some(tool)
                }
            })
            .collect();
        if let Some(choice) = body["tool_choice"].as_object() {
            let ns = choice
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or("");
            let name = choice.get("name").and_then(Value::as_str).unwrap_or("");
            if deferred.contains(ns)
                && !initial.contains(&key(ns, name))
                && !activated.contains(&key(ns, name))
            {
                return Err("required tool schema has not been discovered for this caller".into());
            }
        }
        let discovery_entries: Vec<Value> = config["discoveryTools"]
            .as_array().unwrap().iter()
            .filter(|entry| filtered.iter().any(|tool| {
                tool["type"] == "namespace" && tool["name"] == entry["namespace"]
                    && tool["tools"].as_array().is_some_and(|tools| tools.iter().any(|t| t["name"] == entry["name"]))
            }))
            .cloned().collect();
        if !discovery_entries.is_empty() {
            let previous = if body["instructions"].is_null() { "" } else {
                body["instructions"].as_str().ok_or("tool discovery instructions must be text")?
            };
            body["instructions"] = json!(format!("{previous}\n\nTool availability: this request uses on-demand tool discovery. Only tools listed in this request are callable now. A tool mentioned in context, method descriptions or history may not have its schema loaded. For a needed tool absent from the current list, first use an available discovery entry: {}. Use its actual receipt and the tools listed on the following request; do not guess tool names or provider aliases. Discovery does not grant additional authority. Choose whether any optional method is useful; no method is mandatory.", serde_json::to_string(&discovery_entries).map_err(|e| e.to_string())?));
        }
        body["tools"] = filtered.into();
        scope.active = activated;
        for id in consumed {
            scope.pending.remove(&id);
        }
        Ok(body)
    }

    /// Called only after codec terminal validation and before delivery to Codex.
    pub(crate) fn observe(
        &mut self,
        response: &Value,
        config: &Value,
        who: &Value,
        catalog: &Value,
    ) -> Result<()> {
        if config.is_null() || response["status"] != "completed" {
            return Ok(());
        }
        let config = configuration(config)?;
        let owner = caller(who)?;
        if who["requestKind"] == "compaction" {
            return Ok(());
        }
        let scope = self
            .scopes
            .get_mut(&owner)
            .ok_or("discovery response has no prepared caller")?;
        if catalog.as_str() != Some(scope.catalog.as_str()) {
            return Err("discovery admitted catalog changed during request".into());
        }
        if scope.config != digest::digest(&config) {
            return Err("discovery configuration changed during request".into());
        }
        let mut pending = scope.pending.clone();
        for call in response["output"].as_array().into_iter().flatten() {
            if call["type"] != "function_call"
                || !config["discoveryTools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|id| id["name"] == call["name"] && id["namespace"] == call["namespace"])
            {
                continue;
            }
            let id = call["call_id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or("discovery response call id is invalid")?;
            let entry = Pending {
                namespace: call["namespace"].as_str().unwrap().into(),
                name: call["name"].as_str().unwrap().into(),
                arguments: arguments(&call["arguments"])?,
            };
            if let Some(previous) = pending.get(id) {
                if previous.namespace != entry.namespace
                    || previous.name != entry.name
                    || previous.arguments != entry.arguments
                {
                    return Err("discovery call id was reused with different arguments".into());
                }
            } else if pending.len() >= MAX_PENDING {
                return Err("portable discovery pending call limit reached".into());
            }
            pending.insert(id.into(), entry);
        }
        scope.pending = pending;
        Ok(())
    }
}
