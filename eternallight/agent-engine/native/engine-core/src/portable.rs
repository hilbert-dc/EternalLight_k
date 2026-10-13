use crate::{Result, canonical::*, tokens};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn append(items: &mut Vec<Value>, role: &str, field: &str, part: Value) {
    if let Some(previous) = items
        .last_mut()
        .filter(|v| v["role"] == role && v[field].is_array())
    {
        previous[field].as_array_mut().unwrap().push(part);
    } else {
        let mut v = json!({"role":role});
        v[field] = json!([part]);
        items.push(v);
    }
}
fn endpoint(connection: &Value, path: &str) -> Result<String> {
    Ok(string(&super::canonical::dispatch(
        "canonical.endpoint",
        json!({"endpoint":connection["endpoint"],"path":path}),
    )?)
    .to_owned())
}
fn copy(source: &Value, target: &mut Value, keys: &[&str]) {
    for key in keys {
        if let Some(value) = source.get(key) {
            target[*key] = value.clone();
        }
    }
}

pub fn encode(protocol: &str, request: &Value, connection: &Value, key: &str) -> Result<Value> {
    let context = prepare(request, connection, &protocol.into())?;
    let format = &request["text"]["format"];
    if !format.is_null() && !matches!(format["type"].as_str(), Some("text" | "json_schema")) {
        return Err(format!("unsupported text format: {}", format["type"]));
    }
    let choice = &context["toolChoice"];
    let tools = array(&context["tools"]);
    let mut messages = Vec::new();
    if protocol == "chat-completions" {
        if !string(&context["instructions"]).is_empty() {
            messages.push(json!({"role":"system","content":context["instructions"]}));
        }
        for m in array(&context["conversation"]) {
            if m["role"] == "assistant" && m["toolCall"].is_object() {
                let call = json!({"id":m["toolCall"]["id"],"type":"function","function":{"name":m["toolCall"]["name"],"arguments":m["toolCall"]["arguments"]}});
                if let Some(previous) = messages.last_mut().filter(|v| {
                    v["role"] == "assistant"
                        && v["tool_calls"].is_array()
                        && string(&v["content"]).is_empty()
                }) {
                    previous["tool_calls"].as_array_mut().unwrap().push(call);
                } else {
                    messages.push(
                        json!({"role":"assistant","content":m["content"],"tool_calls":[call]}),
                    );
                }
            } else if m["role"] == "tool" {
                messages.push(
                    json!({"role":"tool","tool_call_id":m["toolCallId"],"content":m["content"]}),
                );
            } else {
                messages.push(json!({"role":m["role"],"content":m["content"]}));
            }
        }
        let field = connection["protocolOptions"]["maxOutputTokensField"]
            .as_str()
            .unwrap_or("max_tokens");
        if !["max_tokens", "max_completion_tokens"].contains(&field) {
            return Err(format!("unsupported chat output token field: {field}"));
        }
        let mode = connection["protocolOptions"]["toolChoiceMode"]
            .as_str()
            .unwrap_or("full");
        if mode == "none" && !choice.is_null() {
            return Err("connection does not support tool_choice".into());
        }
        let streaming = request["stream"] != false;
        let mut body = json!({"model":connection["model"],"messages":messages,"stream":streaming});
        body[field] = context["maxOutputTokens"].clone();
        if streaming {
            body["stream_options"] = json!({"include_usage":true});
        }
        if !tools.is_empty() {
            body["tools"] = tools
                .iter()
                .map(|t| json!({"type":"function","function":t}))
                .collect();
        }
        if !choice.is_null() && !(mode == "auto-only" && choice != "auto") {
            body["tool_choice"] = if choice.is_string() {
                choice.clone()
            } else {
                json!({"type":"function","function":{"name":choice["name"]}})
            };
        }
        copy(request, &mut body, &["parallel_tool_calls", "temperature"]);
        if format["type"] == "json_schema" {
            let mut schema = json!({"schema":format["schema"],"strict":format["strict"]==true});
            copy(format, &mut schema, &["name", "description"]);
            body["response_format"] = json!({"type":"json_schema","json_schema":schema});
        }
        let items: Value = messages
            .iter()
            .map(|m| m.get("content").cloned().unwrap_or(m.clone()))
            .collect();
        return Ok(
            json!({"context":finalize(context,&body,&items)?,"url":endpoint(connection,"/v1/chat/completions")?,"headers":{"authorization":format!("Bearer {key}"),"content-type":"application/json"},"body":body}),
        );
    }
    let initial = initial(&context["conversation"], protocol)?;
    let mut instructions = vec![string(&context["instructions"]).to_owned()];
    instructions.extend(
        array(&initial["instructions"])
            .iter()
            .map(|v| string(v).to_owned()),
    );
    let system = instructions
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");
    tokens::bounded(&system.clone().into(), "system instructions")?;
    for m in array(&initial["conversation"]) {
        let call = &m["toolCall"];
        let role = string(&m["role"]);
        if protocol == "messages" {
            if role == "assistant" && call.is_object() {
                append(
                    &mut messages,
                    "assistant",
                    "content",
                    json!({"type":"tool_use","id":call["id"],"name":call["name"],"input":arguments(&call["arguments"])?}),
                );
            } else if role == "tool" {
                append(
                    &mut messages,
                    "user",
                    "content",
                    json!({"type":"tool_result","tool_use_id":m["toolCallId"],"content":m["content"]}),
                );
            } else {
                messages.push(json!({"role":if role=="assistant"{"assistant"}else{"user"},"content":m["content"]}));
            }
        } else {
            if role == "assistant" && call.is_object() {
                let mut function =
                    json!({"name":call["name"],"args":arguments(&call["arguments"])?});
                if !string(&call["id"]).starts_with("call_local_") {
                    function["id"] = call["id"].clone();
                }
                append(
                    &mut messages,
                    "model",
                    "parts",
                    json!({"functionCall":function}),
                );
            } else if role == "tool" {
                let mut function = json!({"name":m["toolName"],"response":{"output":m["content"]}});
                if !string(&m["toolCallId"]).starts_with("call_local_") {
                    function["id"] = m["toolCallId"].clone();
                }
                append(
                    &mut messages,
                    "user",
                    "parts",
                    json!({"functionResponse":function}),
                );
            } else {
                append(
                    &mut messages,
                    if role == "assistant" { "model" } else { "user" },
                    "parts",
                    json!({"text":m["content"]}),
                );
            }
        }
    }
    if protocol == "messages" {
        if tools.iter().any(|t| t["strict"] == true) {
            return Err(unsupported("messages cannot guarantee strict tool schemas"));
        }
        if format["type"] == "json_schema" {
            return Err(unsupported(
                "messages json_schema output is not implemented with equivalent guarantees",
            ));
        }
        let mut body = json!({"model":connection["model"],"max_tokens":context["maxOutputTokens"],"system":system,"messages":messages,"stream":false});
        if !tools.is_empty() {
            body["tools"]=tools.iter().map(|t|json!({"name":t["name"],"description":t["description"],"input_schema":t["parameters"]})).collect();
        }
        let mut choice = choice.clone();
        if request["parallel_tool_calls"] == false && choice.is_null() && !tools.is_empty() {
            choice = "auto".into();
        }
        if !choice.is_null() {
            let mut mapped = if let Some(c) = choice.as_str() {
                json!({"type":if c=="required"{"any"}else{c}})
            } else {
                json!({"type":"tool","name":choice["name"]})
            };
            if request["parallel_tool_calls"] == false && choice != "none" {
                mapped["disable_parallel_tool_use"] = true.into();
            }
            body["tool_choice"] = mapped;
        }
        copy(request, &mut body, &["temperature"]);
        let mut items = vec![system.into()];
        items.extend(messages.iter().map(|m| m["content"].clone()));
        return Ok(
            json!({"context":finalize(context,&body,&items.into())?,"url":endpoint(connection,"/v1/messages")?,"headers":{"x-api-key":key,"anthropic-version":"2023-06-01","content-type":"application/json"},"body":body}),
        );
    }
    if request["parallel_tool_calls"] == false {
        return Err(unsupported(
            "generate-content cannot guarantee sequential tool calls",
        ));
    }
    let mut body = json!({"systemInstruction":{"parts":[{"text":system}]},"contents":messages,"generationConfig":{"maxOutputTokens":context["maxOutputTokens"]}});
    if !tools.is_empty() {
        body["tools"] = json!([{"functionDeclarations":tools.iter().map(|t|json!({"name":t["name"],"description":t["description"],"parametersJsonSchema":t["parameters"]})).collect::<Vec<_>>()}]);
    }
    if !choice.is_null() {
        body["toolConfig"] = json!({"functionCallingConfig":if choice.is_string(){json!({"mode":match string(choice){"required"=>"ANY","none"=>"NONE",_=>"AUTO"}})}else{json!({"mode":"ANY","allowedFunctionNames":[choice["name"]]})}});
    }
    copy(request, &mut body["generationConfig"], &["temperature"]);
    if format["type"] == "json_schema" {
        body["generationConfig"]["responseMimeType"] = "application/json".into();
        body["generationConfig"]["responseJsonSchema"] = format["schema"].clone();
    }
    let model = string(&connection["model"]);
    let escaped = url::form_urlencoded::byte_serialize(model.as_bytes())
        .collect::<String>()
        .replace('+', "%20");
    let base = endpoint(
        connection,
        &format!("/v1beta/models/{escaped}:generateContent"),
    )?;
    let mut url = url::Url::parse(&base).map_err(|_| "invalid provider endpoint")?;
    url.query_pairs_mut().append_pair("key", key);
    let mut items = vec![system.into()];
    items.extend(messages.iter().map(|m| m["parts"].clone()));
    Ok(
        json!({"context":finalize(context,&body,&items.into())?,"url":url.to_string(),"headers":{"content-type":"application/json"},"body":body}),
    )
}

pub fn decode(protocol: &str, payload: &Value, context: &Value) -> Result<Value> {
    let mut items = Vec::new();
    let mut validation = json!({"context":context,"callIds":[]});
    let mut options = json!({"status":"completed"});
    let (usage, identifier) = if protocol == "chat-completions" {
        let choice = &payload["choices"][0];
        let message = &choice["message"];
        if !message.is_object() {
            return Err("chat-completions response has no assistant message".into());
        }
        if !string(&message["reasoning_content"]).is_empty() {
            return Err(unsupported(
                "chat reasoning_content requires an opaque state-preserving route",
            ));
        }
        match string(&choice["finish_reason"]) {
            "stop" | "tool_calls" => {}
            "length" => {
                options = json!({"status":"incomplete","incompleteDetails":{"reason":"max_output_tokens"}})
            }
            "content_filter" => {
                options = json!({"status":"failed","error":{"code":"content_filter","message":"provider filtered the response"}})
            }
            _ => return Err("chat-completions response has no valid finish reason".into()),
        }
        for key in ["content", "refusal"] {
            if !string(&message[key]).is_empty() {
                items.push(text_item(&message[key], &Value::Null));
            }
        }
        for call in array(&message["tool_calls"]) {
            if call["type"] != "function" {
                return Err("unsupported chat tool call type".into());
            }
            let call = tool_call(
                &json!({"id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}),
                &mut validation,
            )?;
            items.push(call_item(&call, &Value::Null));
        }
        (
            json!({"inputTokens":payload["usage"]["prompt_tokens"],"outputTokens":payload["usage"]["completion_tokens"],"totalTokens":payload["usage"]["total_tokens"],"inputTokenDetails":payload["usage"]["prompt_tokens_details"],"outputTokenDetails":payload["usage"]["completion_tokens_details"]}),
            payload["id"].clone(),
        )
    } else if protocol == "messages" {
        match string(&payload["stop_reason"]) {
            "end_turn" | "tool_use" | "stop_sequence" => {}
            "max_tokens" => {
                options = json!({"status":"incomplete","incompleteDetails":{"reason":"max_output_tokens"}})
            }
            _ => return Err("messages response has no valid stop reason".into()),
        }
        for part in array(&payload["content"]) {
            if matches!(
                part["type"].as_str(),
                Some("thinking" | "redacted_thinking")
            ) || !part["signature"].is_null()
            {
                return Err(unsupported(
                    "signed thinking state requires a state-preserving route",
                ));
            }
            match string(&part["type"]) {
                "text" => items.push(text_item(&part["text"], &Value::Null)),
                "tool_use" => {
                    let call = tool_call(
                        &json!({"id":part["id"],"name":part["name"],"arguments":part["input"]}),
                        &mut validation,
                    )?;
                    items.push(call_item(&call, &Value::Null));
                }
                _ => return Err("unsupported messages response part".into()),
            }
        }
        let u = &payload["usage"];
        let parts = [
            "input_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
        ]
        .iter()
        .filter_map(|k| u[*k].as_f64())
        .collect::<Vec<_>>();
        let total = if parts.is_empty() {
            Value::Null
        } else {
            parts.iter().sum::<f64>().into()
        };
        (
            json!({"inputTokens":total,"outputTokens":u["output_tokens"],"inputTokenDetails":{"cached_tokens":u["cache_read_input_tokens"].as_f64().unwrap_or(0.0),"cache_write_tokens":u["cache_creation_input_tokens"].as_f64().unwrap_or(0.0)}}),
            payload["id"].clone(),
        )
    } else {
        let candidate = &payload["candidates"][0];
        let reason = string(&candidate["finishReason"]);
        match reason {
            "STOP" => {}
            "MAX_TOKENS" => {
                options = json!({"status":"incomplete","incompleteDetails":{"reason":"max_output_tokens"}})
            }
            "SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" => {
                options = json!({"status":"failed","error":{"code":reason.to_lowercase(),"message":format!("provider stopped generation: {reason}")}})
            }
            _ => return Err("generate-content response has no valid finish reason".into()),
        }
        for (index, part) in array(&candidate["content"]["parts"]).iter().enumerate() {
            if part["thought"] == true || !part["thoughtSignature"].is_null() {
                return Err(unsupported(
                    "generate-content thought state requires a native-state preserving route",
                ));
            }
            if part["text"].is_string() {
                if !string(&part["text"]).is_empty() {
                    items.push(text_item(&part["text"], &Value::Null));
                }
                continue;
            }
            let function = &part["functionCall"];
            if string(&function["name"]).is_empty() {
                return Err(unsupported("unsupported generate-content response part"));
            }
            let local = format!(
                "{}:{}:{index}:{}",
                context["requestId"].as_str().unwrap_or("unbound"),
                payload["responseId"].as_str().unwrap_or("response"),
                string(&function["name"])
            );
            let call_id = function["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!("call_local_{:.24}", format!("{:x}", Sha256::digest(local)))
                });
            let call = tool_call(
                &json!({"id":call_id,"name":function["name"],"arguments":function["args"]}),
                &mut validation,
            )?;
            items.push(call_item(&call, &Value::Null));
        }
        let m = &payload["usageMetadata"];
        let input = m["promptTokenCount"].as_f64();
        let reasoning = m["thoughtsTokenCount"].as_f64().unwrap_or(0.0);
        let output = if let (Some(total), Some(input)) = (m["totalTokenCount"].as_f64(), input) {
            Some((total - input).max(0.0))
        } else {
            m["candidatesTokenCount"].as_f64().map(|v| v + reasoning)
        };
        (
            json!({"inputTokens":input,"outputTokens":output,"totalTokens":m["totalTokenCount"],"inputTokenDetails":{"cached_tokens":m["cachedContentTokenCount"].as_f64().unwrap_or(0.0)},"outputTokenDetails":{"reasoning_tokens":reasoning}}),
            payload["responseId"].clone(),
        )
    };
    let items = Value::Array(items);
    if options["status"] == "completed" {
        choice_satisfied(context, &items)?;
    }
    Ok(envelope(
        &json!({"items":items,"usage":usage,"id":identifier,"options":options}),
    ))
}
