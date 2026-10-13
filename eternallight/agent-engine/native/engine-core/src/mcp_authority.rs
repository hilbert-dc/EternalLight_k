//! Trusted MCP declarations and invocation correlation above an unmodified Core.
use crate::{Result, digest};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct McpAuthority {
    catalogs: HashMap<(String, String), Value>,
    calls: HashMap<(String, String), Value>,
    scope: Option<String>,
}
fn id(v: &Value) -> Result<&str> {
    v.as_str()
        .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
        .ok_or_else(|| "invalid MCP authority identity".into())
}
impl McpAuthority {
    pub fn begin_execution(&mut self) {
        self.calls.clear();
        self.scope = None;
    }
    pub fn register(&mut self, input: &Value) -> Result<Value> {
        let scope = id(&input["scope"])?;
        let ns = id(&input["namespace"])?;
        if !ns.starts_with("mcp__") {
            return Err("invalid MCP namespace".into());
        }
        let tools = input["tools"]
            .as_array()
            .filter(|v| v.len() <= 1024)
            .ok_or("invalid MCP catalog")?;
        let mut names = std::collections::HashSet::new();
        for tool in tools {
            let name = id(&tool["name"])?;
            if !names.insert(name) || !tool["inputSchema"].is_object() {
                return Err("invalid or duplicate MCP declaration".into());
            }
        }
        let key = (scope.into(), ns.into());
        let value = json!({"digest":digest::digest(&input["tools"]),"tools":input["tools"]});
        if let Some(existing) = self.catalogs.get(&key) {
            if existing != &value {
                return Err("MCP catalog changed within its frozen scope".into());
            }
        } else {
            // Resource protection; scopes are normally retired at first sampling.
            if self.catalogs.len() >= 1024 {
                return Err("MCP catalog capacity exceeded".into());
            }
            self.catalogs.insert(key, value.clone());
        }
        Ok(json!({"catalogDigest":value["digest"]}))
    }
    pub fn prepare(&mut self, body: &mut Value, scope: &str) -> Result<()> {
        id(&json!(scope))?;
        if self.scope.as_deref().is_some_and(|s| s != scope) {
            return Err("MCP authority scope changed within execution".into());
        }
        self.scope = Some(scope.into());
        self.catalogs.retain(|(owner, _), _| owner == scope);
        if let Some(tools) = body["tools"].as_array_mut() {
            for tool in tools {
                if tool["type"] == "namespace" {
                    let ns = id(&tool["name"])?.to_owned();
                    if !ns.starts_with("mcp__") {
                        continue;
                    }
                    for entry in tool["tools"]
                        .as_array_mut()
                        .ok_or("invalid MCP namespace entries")?
                    {
                        self.restore(scope, &ns, entry, false)?;
                    }
                } else if tool["type"] == "function"
                    && tool["name"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("mcp__"))
                {
                    let name = id(&tool["name"])?;
                    let (ns, _) = name.rsplit_once("__").ok_or("invalid flat MCP name")?;
                    self.restore(scope, &ns.to_owned(), tool, true)?;
                }
            }
        }
        Ok(())
    }
    fn restore(&self, scope: &str, ns: &str, tool: &mut Value, flat: bool) -> Result<()> {
        let catalog = self
            .catalogs
            .get(&(scope.into(), ns.into()))
            .ok_or("MCP authoritative catalog is missing")?;
        let wire_name = id(&tool["name"])?;
        let name = if flat {
            wire_name
                .strip_prefix(&format!("{ns}__"))
                .ok_or("MCP name mismatch")?
        } else {
            wire_name
        };
        let original = catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .ok_or("MCP tool is outside authoritative catalog")?;
        tool["parameters"] = original["inputSchema"].clone();
        // Do not change visibility, tool choice, descriptions or add declarations.
        Ok(())
    }
    pub fn record(
        &mut self,
        scope: &str,
        lease: &Value,
        identity: &Value,
        batch: &str,
        item: &Value,
    ) -> Result<()> {
        if item["type"] != "function_call" {
            return Ok(());
        }
        let name = id(&item["name"])?;
        let (ns, tool) = if let Some(ns) = item["namespace"]
            .as_str()
            .filter(|s| s.starts_with("mcp__"))
        {
            (ns, name)
        } else if name.starts_with("mcp__") {
            name.rsplit_once("__").ok_or("invalid MCP response name")?
        } else {
            return Ok(());
        };
        let catalog = self
            .catalogs
            .get(&(scope.into(), ns.into()))
            .ok_or("MCP response catalog is missing")?;
        if !catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == tool)
        {
            return Err("MCP response tool is outside authoritative catalog".into());
        }
        let caller = id(&identity["threadId"])?;
        let call = id(&item["call_id"])?;
        let arguments: Value =
            serde_json::from_str(item["arguments"].as_str().ok_or("missing MCP arguments")?)
                .map_err(|_| "invalid MCP arguments")?;
        let record = json!({"scope":scope,"namespace":ns,"name":tool,"argumentsDigest":digest::digest(&arguments),
            "catalogDigest":catalog["digest"],"leaseId":lease["leaseId"],"epoch":lease["epoch"],
            "meta":{"threadId":caller,"callerThreadId":caller,"rootThreadId":identity["sessionId"],"sessionId":identity["sessionId"],
                "parentThreadId":identity["parentThreadId"],"turnId":identity["turnId"],"agentRole":identity["agentRole"],
                "callId":call,"samplingRequestId":batch}});
        id(&record["meta"]["rootThreadId"])?;
        id(&record["meta"]["turnId"])?;
        let key = (caller.into(), call.into());
        if let Some(existing) = self.calls.get(&key) {
            if existing != &record {
                return Err("MCP call identity collision".into());
            }
        } else {
            if self.calls.len() >= 100_000 {
                return Err("MCP invocation correlation capacity exceeded".into());
            }
            self.calls.insert(key, record);
        }
        Ok(())
    }
    pub fn resolve(&self, input: &Value, lease: &Value) -> Result<Value> {
        let meta = &input["meta"];
        let key = (
            id(&meta["threadId"])?.to_owned(),
            id(&meta["callId"])?.to_owned(),
        );
        let record = self
            .calls
            .get(&key)
            .ok_or("MCP invocation has no admitted model call")?;
        for field in ["scope", "namespace", "name", "catalogDigest"] {
            if input[field] != record[field] {
                return Err(format!("MCP invocation {field} mismatch"));
            }
        }
        if record["leaseId"] != lease["leaseId"]
            || record["epoch"] != lease["epoch"]
            || lease.is_null()
            || meta["sessionId"] != record["meta"]["sessionId"]
            || digest::digest(&input["arguments"]) != record["argumentsDigest"]
        {
            return Err("MCP invocation lease, session or arguments mismatch".into());
        }
        Ok(record["meta"].clone())
    }
}

#[cfg(test)]
#[path = "mcp_authority_tests.rs"]
mod tests;
