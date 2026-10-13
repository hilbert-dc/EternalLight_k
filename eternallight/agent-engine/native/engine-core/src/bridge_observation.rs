use crate::{
    canonical::{array, string},
    digest, message_evidence,
};
use serde_json::{Value, json};

fn tools(value: &Value) -> Value {
    array(value).iter().flat_map(|tool| {
        if tool["type"]=="namespace" && tool["name"].is_string() {
            array(&tool["tools"]).iter().map(|nested|json!({"type":nested["type"],"name":nested["name"],"namespace":tool["name"]})).collect()
        } else {vec![json!({"type":tool["type"],"name":tool.get("name").unwrap_or(&tool["function"]["name"]),"namespace":tool["namespace"]})]}
    }).collect()
}

fn count(value: &Value) -> Value {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0 && *n <= 9_007_199_254_740_991.0 && n.fract() == 0.0)
        .map(|n| json!(n as u64)).unwrap_or(Value::Null)
}

// Numeric observation only, never a second billing authority. Missing stays unknown.
pub(super) fn usage(value: &Value) -> Value {
    json!({"inputTokens":count(&value["input_tokens"]),"outputTokens":count(&value["output_tokens"]),
        "totalTokens":count(&value["total_tokens"]),"cachedTokens":count(&value["input_tokens_details"]["cached_tokens"]),
        "reasoningTokens":count(&value["output_tokens_details"]["reasoning_tokens"])})
}

fn request_metrics(body: &Value) -> Value {
    let bytes = |v: &Value| if v.is_null() { 0 } else { serde_json::to_vec(v).map(|s| s.len()).unwrap_or(0) };
    json!({"maxOutputTokens":count(&body["max_output_tokens"]),"maxTokens":count(&body["max_tokens"]),
        "maxCompletionTokens":count(&body["max_completion_tokens"]),"generationMaxOutputTokens":count(&body["generationConfig"]["maxOutputTokens"]),
        "bodyBytes":bytes(body),"toolsBytes":bytes(&body["tools"]),"inputBytes":bytes(&body["input"]),
        "messagesBytes":bytes(&body["messages"]),"instructionsBytes":bytes(&body["instructions"])})
}

pub(super) fn request(context: &Value, body: &Value, prepared: &Value) -> Value {
    let mut result = context.clone();
    result["requestMetrics"] = request_metrics(&prepared["body"]);
    let extra = json!({"model":body["model"],"stream":body["stream"]!=false,"toolChoice":body["tool_choice"],"tools":tools(&body["tools"]),"encodedTools":tools(&prepared["body"]["tools"]),"encodedToolChoice":prepared["body"]["tool_choice"],"reasoningEffort":prepared["body"]["reasoning"]["effort"],"visibleTextDigests":message_evidence::dispatch("message.visible",&prepared["body"]).unwrap_or(json!([])),"waitResults":message_evidence::dispatch("message.wait",&body["input"]).unwrap_or(json!([])),"finalAgentMessages":message_evidence::dispatch("message.final",&body["input"]).unwrap_or(json!([])),"inputItemTypes":array(&body["input"]).iter().map(|item|item["type"].as_str().unwrap_or("unknown")).collect::<Vec<_>>(),"toolResultCallIds":array(&body["input"]).iter().filter(|item|item["type"]=="function_call_output").filter_map(|item|item["call_id"].as_str()).collect::<Vec<_>>()});
    result
        .as_object_mut()
        .expect("observation context")
        .extend(extra.as_object().unwrap().clone());
    result
}

pub(super) fn response(context: &Value, payload: &Value) -> Value {
    let output = array(&payload["output"]);
    let answer: String = output
        .iter()
        .flat_map(|item| array(&item["content"]))
        .filter(|part| part["type"] == "output_text")
        .filter_map(|part| part["text"].as_str())
        .collect();
    let calls: Vec<_> = output
        .iter()
        .filter(|item| {
            matches!(
                item["type"].as_str(),
                Some("function_call" | "custom_tool_call")
            )
        })
        .collect();
    let tool_calls:Vec<_>=calls.iter().map(|item| {
        let arguments=if item["type"]=="function_call" {serde_json::from_str::<Value>(string(&item["arguments"])).ok()} else {None};
        let digests:serde_json::Map<_,_>=arguments.as_ref().and_then(Value::as_object).into_iter().flatten().map(|(key,value)|(key.clone(),digest::digest(value).into())).collect();
        json!({"callId":item.get("call_id").unwrap_or(&item["id"]),"name":item["name"],"namespace":item["namespace"],"argumentValueDigests":digests})
    }).collect();
    let lower = answer.to_ascii_lowercase();
    let final_text = if calls.is_empty() && !answer.trim().is_empty() {
        message_evidence::dispatch("message.digest", &answer.trim().into()).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let final_lines = if final_text.is_null() {
        json!([])
    } else {
        message_evidence::dispatch("message.visible", &json!({"input":output})).unwrap_or(json!([]))
    };
    let extra = json!({"responseId":payload["id"],"status":payload["status"],"outputTypes":output.iter().map(|item|item["type"].as_str().unwrap_or("unknown")).collect::<Vec<_>>(),"toolNames":calls.iter().map(|item|item["name"].clone()).collect::<Vec<_>>(),"toolCalls":tool_calls,"textCharacters":answer.encode_utf16().count(),"finalTextDigest":final_text,"finalTextLineDigests":final_lines,"textLooksLikeToolCall":lower.contains("dsml")||lower.contains("<")&&lower.contains("calls")||lower.contains("invoke name=")||lower.contains("tool_call")||lower.contains("tool call")||lower.contains("toolcall")});
    let mut result = context.clone();
    result["normalizedUsage"] = usage(&payload["usage"]);
    result
        .as_object_mut()
        .expect("observation context")
        .extend(extra.as_object().unwrap().clone());
    result
}

#[cfg(test)]
#[path = "bridge_observation_tests.rs"]
mod tests;
