use crate::{
    Result,
    canonical::{array, string},
    tokens, tool_registry,
};
use eternallight_stream_guard::{Command, StreamGuard};
use serde_json::{Value, json};
use std::collections::HashSet;

fn restore_item(item: &Value, context: &Value) -> Result<Value> {
    if context["registry"].is_null() || item["type"] != "function_call" {
        return Ok(item.clone());
    }
    let entry = tool_registry::dispatch(
        "tools.require",
        json!({"entries":context["registry"]["entries"],"wireName":item["name"]}),
    )?;
    let mut item = item.clone();
    item["name"] = entry["name"].clone();
    if entry["namespace"].is_null() {
        item.as_object_mut().unwrap().remove("namespace");
    } else {
        item["namespace"] = entry["namespace"].clone();
    }
    if entry["namespace"] == "collaboration"
        && matches!(
            entry["name"].as_str(),
            Some("spawn_agent" | "send_message" | "followup_task")
        )
        && item["encrypted_function_args"].is_null()
    {
        item["encrypted_function_args"] = json!([]);
    }
    Ok(item)
}

fn restore_payload(payload: &Value, context: &Value) -> Result<Value> {
    let mut payload = payload.clone();
    if let Some(output) = payload["output"].as_array_mut() {
        for item in output {
            *item = restore_item(item, context)?;
        }
    }
    Ok(payload)
}

fn validate_call(item: &Value, context: &Value, ids: &mut HashSet<String>) -> Result<()> {
    let id = item["call_id"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or("native Responses tool call id is required")?;
    if !ids.insert(id.into()) {
        return Err(format!("duplicate tool call id: {id}"));
    }
    let kind = match string(&item["type"]) {
        "function_call" => "function",
        "custom_tool_call" => "custom",
        _ => return Err("unsupported executable Responses item".into()),
    };
    let name = string(&item["name"]);
    let tool = array(&context["tools"])
        .iter()
        .find(|t| t["type"] == kind && t["namespace"] == item["namespace"] && t["name"] == name)
        .ok_or_else(|| format!("tool is not authorized for this request: {name}"))?;
    let choice = &context["toolChoice"];
    if choice == "none" {
        return Err("provider returned a tool call while tool_choice is none".into());
    }
    if choice.is_object()
        && (choice["type"] != kind
            || choice["namespace"] != item["namespace"]
            || choice["name"] != name)
    {
        return Err(format!(
            "provider returned {name} while another tool was required"
        ));
    }
    if kind == "function" {
        let parsed = crate::canonical::arguments(&item["arguments"])?;
        if !crate::executor_validation::delegated(context, &item["namespace"], &item["name"]) {
            crate::tool_validation::validate(&parsed, &tool["schema"], name)?;
        }
    } else {
        tokens::bounded(&item["input"], &format!("input for {name}"))?;
    }
    Ok(())
}

fn validate_readable(item: &Value) -> Result<()> {
    if !matches!(
        item["type"].as_str(),
        Some("message" | "reasoning" | "agent_message")
    ) {
        return Err(format!(
            "unsupported executable Responses item: {}",
            item["type"]
        ));
    }
    Ok(())
}

fn validate_output(payload: &Value, context: &Value) -> Result<()> {
    if context.is_null() {
        return Err("prepared native Responses context is required".into());
    }
    if payload["status"] != "completed" {
        return Err("Responses output did not complete successfully".into());
    }
    let output = payload["output"]
        .as_array()
        .ok_or("Responses output must be an array")?;
    let mut ids = HashSet::new();
    for item in output {
        tokens::bounded_output_item(item)?;
        if matches!(
            item["type"].as_str(),
            Some("function_call" | "custom_tool_call")
        ) {
            validate_call(item, context, &mut ids)?;
        } else {
            validate_readable(item)?;
        }
    }
    if (context["toolChoice"] == "required" || context["toolChoice"].is_object()) && ids.is_empty()
    {
        return Err("provider completed without the required tool call".into());
    }
    Ok(())
}

pub fn decode(payload: &Value, context: &Value) -> Result<Value> {
    let restored = restore_payload(payload, context)?;
    validate_output(&restored, context)?;
    Ok(restored)
}

/// Request authorization and terminal consistency precede the shared stream commit gate.
pub struct ResponsesStream {
    context: Value,
    ids: HashSet<String>,
    completed: Vec<Value>,
    guard: Option<StreamGuard>,
}

impl ResponsesStream {
    pub fn new(context: Value) -> Result<Self> {
        if context.is_null() {
            return Err("prepared native Responses context is required".into());
        }
        Ok(Self {
            context,
            ids: HashSet::new(),
            completed: Vec::new(),
            guard: Some(StreamGuard::default()),
        })
    }

    pub fn push(&mut self, event: Value) -> Result<Vec<Value>> {
        let result = self.consume(event);
        if result.is_err() {
            self.close();
        }
        result
    }

    fn consume(&mut self, mut event: Value) -> Result<Vec<Value>> {
        if self.guard.is_none() {
            return Err("Responses stream closed".into());
        }
        if !event["item"].is_null() {
            event["item"] = restore_item(&event["item"], &self.context)?;
        } else if !event["response"].is_null() {
            event["response"] = restore_payload(&event["response"], &self.context)?;
        }
        let kind = string(&event["type"]);
        if matches!(
            kind,
            "response.output_item.added" | "response.output_item.done"
        ) {
            let item = &event["item"];
            // Only completed items enter Codex history. Deltas remain readable,
            // but neither a full added item nor a completed item may bypass the
            // same admission bound used for the next request's input.
            tokens::bounded_output_item(item)?;
            if matches!(
                item["type"].as_str(),
                Some("function_call" | "custom_tool_call")
            ) {
                if kind == "response.output_item.done" {
                    validate_call(item, &self.context, &mut self.ids)?;
                    self.completed.push(item.clone());
                }
            } else {
                validate_readable(item)?;
            }
        }
        if kind == "response.completed" {
            validate_output(&event["response"], &self.context)?;
            for item in &self.completed {
                let final_item = array(&event["response"]["output"])
                    .iter()
                    .find(|candidate| candidate["call_id"] == item["call_id"]);
                if final_item.is_none_or(|candidate| {
                    ["type", "name", "namespace", "arguments", "input"]
                        .iter()
                        .any(|key| candidate[*key] != item[*key])
                }) {
                    return Err("Responses stream tool output differs from terminal output".into());
                }
            }
        }
        self.guard
            .as_mut()
            .unwrap()
            .apply(Command::Push { event })
            .map_err(str::to_owned)
    }

    pub fn finish(&mut self) -> Result<Vec<Value>> {
        let result = self
            .guard
            .as_mut()
            .ok_or_else(|| "Responses stream closed".to_owned())?
            .apply(Command::Finish)
            .map_err(str::to_owned);
        self.close();
        result
    }

    pub fn close(&mut self) {
        self.guard = None;
        self.context = Value::Null;
        self.ids.clear();
        self.completed.clear();
    }
}
