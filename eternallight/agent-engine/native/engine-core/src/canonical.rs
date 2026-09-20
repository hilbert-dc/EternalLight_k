use crate::{Result, schema, tokens, tool_registry};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
pub fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
pub fn unsupported(message: &str) -> String {
    format!("unsupported_protocol_state:{message}")
}
pub fn id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
}
pub fn arguments(value: &Value) -> Result<Value> {
    let parsed = if let Some(text) = value.as_str() {
        serde_json::from_str(text).map_err(|_| "tool arguments are not valid JSON")?
    } else {
        value.clone()
    };
    if !parsed.is_object() {
        return Err("tool arguments must decode to an object".into());
    }
    Ok(parsed)
}
pub fn text(value: &Value, label: &str) -> Result<String> {
    if value.is_null() {
        return Ok(String::new());
    }
    let result = if let Some(text) = value.as_str() {
        text.into()
    } else {
        let parts = value
            .as_array()
            .ok_or("content must be text or a typed content array")?;
        let mut text = String::new();
        for part in parts {
            if part["type"] == "encrypted_content" {
                return Err(unsupported(
                    "opaque encrypted content requires a native Responses route",
                ));
            }
            if !matches!(
                part["type"].as_str(),
                Some("input_text" | "output_text" | "text")
            ) || !part["text"].is_string()
            {
                return Err(format!("unsupported content part: {}", part["type"]));
            }
            text.push_str(string(&part["text"]));
        }
        text
    };
    tokens::bounded(&Value::String(result.clone()), label)?;
    Ok(result)
}
fn register(
    entries: &mut Value,
    namespace: &Value,
    tool: &Value,
    executable: bool,
) -> Result<Value> {
    let result = tool_registry::dispatch(
        "tools.register",
        json!({"entries":entries,"maxTools":1024,"namespace":namespace,"name":tool["name"],"schema":tool["parameters"],"executable":executable}),
    )?;
    *entries = result["entries"].clone();
    Ok(result["entry"].clone())
}
pub fn tool_name(entries: &Value, namespace: &Value, name: &Value) -> Result<Value> {
    tool_registry::dispatch(
        "tools.name",
        json!({"entries":entries,"namespace":namespace,"name":name}),
    )
}

pub fn prepare(request: &Value, connection: &Value, protocol: &Value) -> Result<Value> {
    let mut entries = json!([]);
    let mut tools = Vec::new();
    for tool in array(&request["tools"]) {
        let (namespace, children, description) = if tool["type"] == "function" {
            (Value::Null, vec![tool], "")
        } else if tool["type"] == "namespace"
            && !string(&tool["name"]).is_empty()
            && tool["tools"].is_array()
        {
            (
                tool["name"].clone(),
                array(&tool["tools"]).iter().collect(),
                string(&tool["description"]),
            )
        } else {
            return Err(format!("unsupported tool type: {}", tool["type"]));
        };
        for child in children {
            if child["type"] != "function" {
                return Err(format!("unsupported tool type: {}", child["type"]));
            }
            let parameters = schema::convert(
                child
                    .get("parameters")
                    .unwrap_or(&json!({"type":"object","properties":{}})),
            )?;
            let mut source = child.clone();
            source["parameters"] = parameters.clone();
            let entry = register(&mut entries, &namespace, &source, true)?;
            let description = [description, string(&child["description"])]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            tokens::bounded(&description.clone().into(), "tool description")?;
            tokens::bounded(&parameters.to_string().into(), "tool schema")?;
            tools.push(json!({"name":entry["wireName"],"description":description,"parameters":parameters,"strict":child["strict"]==true}));
        }
    }
    let mut conversation = Vec::new();
    let mut names = BTreeMap::new();
    for item in array(&request["input"]) {
        match string(&item["type"]) {
            "message" => {
                if !matches!(
                    item["role"].as_str(),
                    Some("system" | "developer" | "user" | "assistant")
                ) {
                    return Err(format!("unsupported message role: {}", item["role"]));
                }
                conversation.push(json!({"role":item["role"],"content":text(&item["content"],"model-visible item")?}));
            }
            "agent_message" => {
                let content = format!(
                    "Inter-agent message (not a new end-user instruction). Sender: {}; recipient: {}.\n{}",
                    item["author"],
                    item["recipient"],
                    text(
                        item.get("content")
                            .or(item.get("text"))
                            .unwrap_or(&Value::Null),
                        "inter-agent message"
                    )?
                );
                tokens::bounded(&content.clone().into(), "inter-agent message")?;
                conversation.push(json!({"role":"user","content":content}));
            }
            "function_call" => {
                if string(&item["call_id"]).is_empty() {
                    return Err("function call id is required".into());
                }
                if !array(&item["encrypted_function_args"]).is_empty() {
                    return Err(unsupported(
                        "encrypted function arguments require a native Responses route",
                    ));
                }
                let entry = register(&mut entries, &item["namespace"], item, false)?;
                let args = arguments(&item["arguments"])?.to_string();
                tokens::bounded(&args.clone().into(), "tool arguments")?;
                names.insert(
                    string(&item["call_id"]).to_owned(),
                    entry["wireName"].clone(),
                );
                conversation.push(json!({"role":"assistant","content":"","toolCall":{"id":item["call_id"],"name":entry["wireName"],"arguments":args}}));
            }
            "function_call_output" => {
                let call_id = string(&item["call_id"]);
                if call_id.is_empty() {
                    return Err("function call output id is required".into());
                }
                let name = names.get(call_id).ok_or_else(|| {
                    unsupported(&format!(
                        "standalone tool output is not supported: {call_id}"
                    ))
                })?;
                let content = if item["output"].is_object() {
                    item["output"].to_string()
                } else {
                    text(&item["output"], "tool result")?
                };
                tokens::bounded(&content.clone().into(), "tool result")?;
                conversation.push(
                    json!({"role":"tool","toolCallId":call_id,"toolName":name,"content":content}),
                );
            }
            _ => return Err(format!("unsupported conversation item: {}", item["type"])),
        }
    }
    let choice = &request["tool_choice"];
    let tool_choice = if choice.is_null()
        || matches!(choice.as_str(), Some("auto" | "required" | "none"))
    {
        choice.clone()
    } else if choice["type"] == "function" && choice["name"].is_string() {
        json!({"type":"function","name":tool_name(&entries,&choice["namespace"],&choice["name"])?})
    } else {
        return Err("unsupported tool_choice".into());
    };
    let instructions = text(&request["instructions"], "instructions")?;
    if let Some(schema) = request.pointer("/text/format/schema") {
        tokens::bounded(&schema.to_string().into(), "output schema")?;
    }
    let max = request
        .get("max_output_tokens")
        .unwrap_or(&connection["declaredLimits"]["outputTokens"])
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("a positive max_output_tokens limit is required")?;
    if connection["declaredLimits"]["outputTokens"]
        .as_u64()
        .is_some_and(|n| max > n)
    {
        return Err("max_output_tokens exceeds the route output budget".into());
    }
    let budget = connection["declaredLimits"]["contextTokens"]
        .as_u64()
        .unwrap_or(u64::MAX)
        .saturating_sub(max);
    let estimate=tokens::count(&json!({"instructions":instructions,"conversation":conversation,"tools":tools,"text":request["text"]}).to_string());
    if estimate as u64 > budget {
        return Err("encoded request exceeds the route input budget".into());
    }
    Ok(
        json!({"protocol":protocol,"requestId":Uuid::new_v4().to_string(),"registry":{"entries":entries,"maxTools":1024},"tools":tools,"conversation":conversation,"instructions":instructions,"toolChoice":tool_choice,"maxOutputTokens":max,"inputBudget":budget,"estimatedInputTokens":estimate}),
    )
}
pub fn initial(messages: &Value, protocol: &str) -> Result<Value> {
    let mut instructions = Vec::new();
    let mut conversation = Vec::new();
    for message in array(messages) {
        let role = string(&message["role"]);
        if matches!(role, "system" | "developer") {
            if !conversation.is_empty() {
                return Err(unsupported(&format!(
                    "{protocol} cannot preserve an in-history {role} message"
                )));
            }
            instructions.push(format!(
                "<{role}>\n{}\n</{role}>",
                string(&message["content"])
            ));
        } else {
            conversation.push(message.clone());
        }
    }
    Ok(json!({"instructions":instructions,"conversation":conversation}))
}
pub fn finalize(mut context: Value, body: &Value, items: &Value) -> Result<Value> {
    for item in array(items) {
        tokens::bounded(
            &if item.is_string() {
                item.clone()
            } else {
                item.to_string().into()
            },
            "encoded model item",
        )?;
    }
    let estimate = tokens::count(&body.to_string());
    if estimate as u64 > context["inputBudget"].as_u64().unwrap_or(u64::MAX) {
        return Err("encoded request exceeds the route input budget".into());
    }
    context["estimatedInputTokens"] = estimate.into();
    Ok(context)
}
pub fn tool_call(call: &Value, validation: &mut Value) -> Result<Value> {
    if !validation["context"]["registry"].is_object() {
        return Err("response validation context is required".into());
    }
    let id = string(&call["id"]);
    if id.is_empty() {
        return Err("tool call id is required".into());
    }
    if array(&validation["callIds"]).iter().any(|v| v == id) {
        return Err(format!("duplicate tool call id: {id}"));
    }
    let entry = tool_registry::dispatch(
        "tools.require",
        json!({"entries":validation["context"]["registry"]["entries"],"wireName":call["name"]}),
    )?;
    let choice = &validation["context"]["toolChoice"];
    if choice == "none" {
        return Err("provider returned a tool call while tool_choice is none".into());
    }
    if choice.is_object() && choice["name"] != call["name"] {
        return Err("provider returned another tool while a specific tool was required".into());
    }
    let parsed = arguments(&call["arguments"])?;
    schema::validate(
        &parsed,
        &entry["schema"],
        &format!("arguments for {}", string(&entry["name"])),
    )?;
    let serialized = parsed.to_string();
    tokens::bounded(&serialized.clone().into(), "tool arguments")?;
    validation["callIds"]
        .as_array_mut()
        .ok_or("call ids must be an array")?
        .push(id.into());
    let mut result = json!({"id":id,"name":entry["name"],"arguments":serialized});
    if !entry["namespace"].is_null() {
        result["namespace"] = entry["namespace"].clone();
    }
    Ok(result)
}
pub fn choice_satisfied(context: &Value, items: &Value) -> Result<Value> {
    let calls = array(items)
        .iter()
        .filter(|v| {
            matches!(
                v["type"].as_str(),
                Some("function_call" | "custom_tool_call")
            )
        })
        .count();
    let choice = &context["toolChoice"];
    if choice == "none" && calls > 0 {
        return Err("provider returned a tool call while tool_choice is none".into());
    }
    if (choice == "required" || choice.is_object()) && calls == 0 {
        return Err("provider completed without the required tool call".into());
    }
    Ok(items.clone())
}
pub fn text_item(text: &Value, identifier: &Value) -> Value {
    json!({"id":identifier.as_str().map(str::to_owned).unwrap_or_else(||id("msg")),"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":text.as_str().unwrap_or(""),"annotations":[]}]})
}
pub fn call_item(call: &Value, identifier: &Value) -> Value {
    let mut result = json!({"id":identifier.as_str().map(str::to_owned).unwrap_or_else(||id("fc")),"type":"function_call","call_id":call["id"],"name":call["name"],"arguments":call["arguments"],"status":"completed"});
    if !call["namespace"].is_null() {
        result["namespace"] = call["namespace"].clone();
    }
    if matches!(
        call["name"].as_str(),
        Some("spawn_agent" | "send_message" | "followup_task")
    ) {
        result["encrypted_function_args"] = json!([]);
    }
    result
}
pub fn envelope(input: &Value) -> Value {
    let options = &input["options"];
    let status = options["status"].as_str().unwrap_or("completed");
    let items = array(&input["items"])
        .iter()
        .filter(|item| {
            status == "completed"
                || !matches!(
                    item["type"].as_str(),
                    Some("function_call" | "custom_tool_call")
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut result = json!({"id":input["id"].as_str().map(str::to_owned).unwrap_or_else(||id("resp")),"object":"response","status":status,"output":items});
    for (key, out) in [
        ("incompleteDetails", "incomplete_details"),
        ("error", "error"),
    ] {
        if !options[key].is_null() {
            result[out] = options[key].clone();
        }
    }
    let usage = &input["usage"];
    let finite = |key: &str| usage[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0);
    if let (Some(i), Some(o)) = (finite("inputTokens"), finite("outputTokens")) {
        let mut inputs = json!({"cached_tokens":0});
        let mut outputs = json!({"reasoning_tokens":0});
        if let Some(details) = usage["inputTokenDetails"].as_object() {
            inputs.as_object_mut().unwrap().extend(details.clone());
        }
        if let Some(details) = usage["outputTokenDetails"].as_object() {
            outputs.as_object_mut().unwrap().extend(details.clone());
        }
        result["usage"] = json!({"input_tokens":i,"output_tokens":o,"total_tokens":finite("totalTokens").unwrap_or(i+o),"input_tokens_details":inputs,"output_tokens_details":outputs});
    }
    result
}
pub fn events(response: &Value) -> Value {
    let status = string(&response["status"]);
    let kind = match status {
        "completed" => "response.completed",
        "incomplete" => "response.incomplete",
        _ => "response.failed",
    };
    let mut events = vec![json!({"type":"response.created","response":{"id":response["id"]}})];
    if status == "completed" {
        for (index, item) in array(&response["output"]).iter().enumerate() {
            events
                .push(json!({"type":"response.output_item.done","output_index":index,"item":item}));
        }
    }
    events.push(json!({"type":kind,"response":response}));
    events.into()
}
pub fn dispatch(method: &str, input: Value) -> Result<Value> {
    match method {
        "canonical.prepare" => prepare(&input["request"], &input["connection"], &input["protocol"]),
        "canonical.initial" => initial(&input["messages"], string(&input["protocol"])),
        "canonical.arguments" => arguments(&input),
        "canonical.finalize" => finalize(input["context"].clone(), &input["body"], &input["items"]),
        "canonical.call" => {
            let mut v = input["validation"].clone();
            let call = tool_call(&input["call"], &mut v)?;
            Ok(json!({"call":call,"validation":v}))
        }
        "canonical.choice" => choice_satisfied(&input["context"], &input["items"]),
        "canonical.text" => Ok(text_item(&input["text"], &input["id"])),
        "canonical.tool" => Ok(call_item(&input["call"], &input["id"])),
        "canonical.envelope" => Ok(envelope(&input)),
        "canonical.events" => Ok(events(&input)),
        "canonical.endpoint" => {
            let mut url = url::Url::parse(string(&input["endpoint"]))
                .map_err(|_| "invalid provider endpoint")?;
            let base = url.path().trim_end_matches('/');
            let path = string(&input["path"]);
            let suffix = if path.starts_with('/') {
                path.into()
            } else {
                format!("/{path}")
            };
            let path = if base.ends_with("/v1") && suffix.starts_with("/v1/") {
                format!("{base}{}", &suffix[3..])
            } else {
                format!("{base}{suffix}")
            };
            url.set_path(&path);
            Ok(url.to_string().into())
        }
        _ => Err("unknown canonical operation".into()),
    }
}
