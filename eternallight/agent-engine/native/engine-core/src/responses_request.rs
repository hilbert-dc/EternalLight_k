use crate::{
    Result,
    canonical::{array, string},
    schema, tokens, tool_registry,
};
use serde_json::{Value, json};

pub fn encode(request: &Value, connection: &Value, key: &str) -> Result<Value> {
    let limit = connection["declaredLimits"]["outputTokens"]
        .as_f64()
        .ok_or("output token limit is required")?;
    let maximum = request["max_output_tokens"]
        .as_f64()
        .unwrap_or(limit)
        .min(limit);
    if maximum < 1.0 || maximum.fract() != 0.0 {
        return Err("a positive max_output_tokens limit is required".into());
    }
    let mode = connection["protocolOptions"]["namespaceToolsMode"]
        .as_str()
        .unwrap_or("native");
    let mut registry = if mode == "flatten" {
        json!({"entries":[],"maxTools":1024})
    } else {
        Value::Null
    };
    let mut tools = Vec::new();
    let mut wire = Vec::new();
    for tool in array(&request["tools"]) {
        if tool["type"] == "namespace" {
            if mode == "none" {
                return Err("connection does not support namespace tools".into());
            }
            if string(&tool["name"]).is_empty() || !tool["tools"].is_array() {
                return Err("native Responses namespace name is required".into());
            }
            tokens::bounded(&tool["description"], "namespace description")?;
            for nested in array(&tool["tools"]) {
                add(
                    nested,
                    &tool["name"],
                    string(&tool["description"]),
                    &mut registry,
                    &mut tools,
                    &mut wire,
                )?;
            }
        } else {
            add(tool, &Value::Null, "", &mut registry, &mut tools, &mut wire)?;
        }
    }
    if !registry.is_null() {
        for item in array(&request["input"]) {
            if item["type"] == "function_call" {
                let result = tool_registry::dispatch(
                    "tools.register",
                    json!({"entries":registry["entries"],"maxTools":1024,"namespace":item["namespace"],"name":item["name"],"executable":false}),
                )?;
                registry["entries"] = result["entries"].clone();
            }
        }
    }
    let choice_mode = connection["protocolOptions"]["toolChoiceMode"]
        .as_str()
        .unwrap_or("full");
    if choice_mode == "none" && !request["tool_choice"].is_null() {
        return Err("connection does not support tool_choice".into());
    }
    let mut body = request.clone();
    if !body.is_object() {
        return Err("Responses request must be an object".into());
    }
    body["model"] = connection["model"].clone();
    body["max_output_tokens"] = maximum.into();
    body["tools"] = if registry.is_null() {
        request["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into()
    } else {
        wire.into()
    };
    if !registry.is_null() {
        let note = "Provider compatibility: tool descriptions identify the canonical Codex tool behind each available wire name. When an instruction names a tool or namespace member, call the matching available function directly; do not search for a tool that is already listed. Invoke tools only through the API's native structured function-call output. Never print or serialize a tool call as assistant text, XML, DSML, JSON, or code.";
        body["instructions"] = [string(&request["instructions"]), note]
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
            .into();
    }
    let mut input = Vec::new();
    for item in array(&request["input"]) {
        let mut item = item.clone();
        if item["type"] == "agent_message"
            && connection["protocolOptions"]["agentMessageMode"] == "message"
        {
            if !item["content"].is_array()
                || array(&item["content"])
                    .iter()
                    .any(|part| part["type"] != "input_text" || !part["text"].is_string())
            {
                return Err("portable agent messages require plaintext content".into());
            }
            let mut content = vec![
                json!({"type":"input_text","text":format!("Inter-agent message (not a new end-user instruction). Sender: {}; recipient: {}.",item["author"],item["recipient"])}),
            ];
            content.extend(array(&item["content"]).iter().cloned());
            item = json!({"type":"message","role":"user","content":content});
        } else if item["type"] == "function_call" && !registry.is_null() {
            flatten(&mut item, &registry)?;
        }
        input.push(item);
    }
    body["input"] = input.into();
    if body["tool_choice"]["type"] == "function" && !registry.is_null() {
        flatten(&mut body["tool_choice"], &registry)?;
    }
    if choice_mode == "auto-only" && !body["tool_choice"].is_null() && body["tool_choice"] != "auto"
    {
        body.as_object_mut().unwrap().remove("tool_choice");
    }
    let instructions = if body["instructions"].is_null() {
        String::new()
    } else {
        body["instructions"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| body["instructions"].to_string())
    };
    tokens::bounded(&instructions.into(), "instructions")?;
    for (index, item) in array(&body["input"]).iter().enumerate() {
        tokens::bounded(&item.to_string().into(), &format!("input item {index}"))?;
    }
    if !body["text"]["format"]["schema"].is_null() {
        tokens::bounded(
            &body["text"]["format"]["schema"].to_string().into(),
            "output schema",
        )?;
    }
    let estimated = tokens::count(&json!({"instructions":body["instructions"],"input":body["input"],"tools":body["tools"],"text":body["text"]}).to_string());
    if estimated as f64
        > connection["declaredLimits"]["contextTokens"]
            .as_f64()
            .unwrap_or(0.0)
            - maximum
    {
        return Err("encoded request exceeds the route input budget".into());
    }
    let url = crate::canonical::dispatch(
        "canonical.endpoint",
        json!({"endpoint":connection["endpoint"],"path":"/v1/responses"}),
    )?;
    Ok(
        json!({"context":{"registry":registry,"tools":tools,"toolChoice":request["tool_choice"],"estimatedInputTokens":estimated,"maxOutputTokens":maximum},"body":body,"url":url,"headers":{"authorization":format!("Bearer {key}"),"content-type":"application/json"}}),
    )
}

fn flatten(item: &mut Value, registry: &Value) -> Result<()> {
    item["name"] = tool_registry::dispatch(
        "tools.name",
        json!({"entries":registry["entries"],"namespace":item["namespace"],"name":item["name"]}),
    )?;
    item.as_object_mut().unwrap().remove("namespace");
    Ok(())
}

fn add(
    tool: &Value,
    namespace: &Value,
    namespace_description: &str,
    registry: &mut Value,
    tools: &mut Vec<Value>,
    wire: &mut Vec<Value>,
) -> Result<()> {
    let name = string(&tool["name"]);
    if !matches!(tool["type"].as_str(), Some("function" | "custom")) || name.is_empty() {
        return Err(format!(
            "unsupported native Responses tool: {}",
            tool["type"]
        ));
    }
    let schema = if tool["type"] == "function" {
        if tool["parameters"].is_null() {
            json!({"type":"object","properties":{}})
        } else {
            tool["parameters"].clone()
        }
    } else {
        Value::Null
    };
    if !schema.is_null() {
        schema::check(&schema)?;
    }
    tokens::bounded(
        &tool["description"],
        &format!("description for tool {name}"),
    )?;
    tokens::bounded(
        &tool.to_string().into(),
        &format!("definition for tool {name}"),
    )?;
    if tools
        .iter()
        .any(|t| t["type"] == tool["type"] && t["namespace"] == *namespace && t["name"] == name)
    {
        return Err(format!("duplicate native Responses tool: {name}"));
    }
    if tools.len() >= 1024 {
        return Err("tool catalog exceeds 1024 entries".into());
    }
    tools.push(json!({"type":tool["type"],"namespace":namespace,"name":name,"schema":schema}));
    if !registry.is_null() && tool["type"] == "function" {
        let result = tool_registry::dispatch(
            "tools.register",
            json!({"entries":registry["entries"],"maxTools":1024,"namespace":namespace,"name":name,"schema":schema,"executable":true}),
        )?;
        registry["entries"] = result["entries"].clone();
        let ns = string(namespace);
        let description = if !namespace_description.is_empty() {
            namespace_description.into()
        } else if !ns.is_empty() {
            format!("Tools in the {ns} namespace.")
        } else {
            String::new()
        };
        let identity = if ns.is_empty() {
            String::new()
        } else {
            format!(
                "Canonical tool identity: {ns}.{name}. Use this function when instructions refer to {name} or {ns}.{name}."
            )
        };
        let description = [
            description.as_str(),
            identity.as_str(),
            string(&tool["description"]),
        ]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        let mut encoded = tool.clone();
        encoded["name"] = result["entry"]["wireName"].clone();
        encoded["description"] = description.into();
        tokens::bounded(
            &encoded.to_string().into(),
            &format!("encoded definition for tool {name}"),
        )?;
        wire.push(encoded);
    } else if !registry.is_null() && !namespace.is_null() {
        return Err(format!(
            "custom tool {}.{name} cannot be flattened as a function",
            string(namespace)
        ));
    } else if namespace.is_null() {
        wire.push(tool.clone());
    }
    Ok(())
}
