use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

fn items(value: &Value) -> impl DoubleEndedIterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn digest(text: &str) -> String {
    // Match the JSON string bytes used at the public boundary, not raw text.
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(text).expect("string serialization"))
    )
}

fn visible(body: &Value) -> Value {
    let mut seen = HashSet::new();
    let mut hashes = Vec::new();
    let mut add = |text: &Value| {
        if let Some(text) = text.as_str() {
            for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
                if hashes.len() >= 2048 {
                    break;
                }
                let hash = digest(line);
                if seen.insert(hash.clone()) {
                    hashes.push(hash);
                }
            }
        }
    };
    let input = if body["input"].is_null() {
        &body["messages"]
    } else {
        &body["input"]
    };
    for item in items(input) {
        if item["content"].is_string() {
            add(&item["content"]);
        } else {
            for part in items(&item["content"]) {
                if matches!(
                    part["type"].as_str(),
                    Some("input_text" | "output_text" | "text")
                ) {
                    add(&part["text"]);
                }
            }
        }
    }
    for item in items(&body["contents"]) {
        for part in items(&item["parts"]) {
            add(&part["text"]);
        }
    }
    json!(hashes)
}

pub fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "message.digest" => Ok(digest(input.as_str().ok_or("message text must be a string")?).into()),
        "message.visible" => Ok(visible(input)),
        "message.acknowledged" => {
            for response in items(&input["responses"]).filter(|r| r["parentThreadId"].as_str().is_none_or(str::is_empty)) {
                for call in items(&response["toolCalls"]) {
                    if call["name"] != "send_message" || !input["expectedDigest"].is_string() || call["argumentValueDigests"]["message"] != input["expectedDigest"] { continue; }
                    if items(&input["events"]).any(|event| event["method"] == "item/completed"
                        && event["params"]["threadId"] == response["threadId"]
                        && event["params"]["item"]["id"] == call["callId"]
                        && event["params"]["item"]["type"] == "subAgentActivity"
                        && event["params"]["item"]["kind"] == "interacted") {
                        let mut result = call.clone();
                        result["threadId"] = response["threadId"].clone();
                        return Ok(result);
                    }
                }
            }
            Ok(Value::Null)
        }
        "message.wait" => {
            let mut facts = Vec::new();
            for item in items(input).filter(|item| item["type"] == "function_call_output") {
                let text = item["output"].as_str().map(str::to_owned).unwrap_or_else(|| items(&item["output"]).filter_map(|part| part["text"].as_str()).collect());
                let Ok(value) = serde_json::from_str::<Value>(&text) else { continue; };
                let Some(timed_out) = value["timed_out"].as_bool() else { continue; };
                facts.push(json!({"callId":item["call_id"],"completed":!timed_out && value["message"].as_str().is_some_and(|message| message.starts_with("Wait completed."))}));
            }
            Ok(facts.into_iter().rev().take(128).collect::<Vec<_>>().into_iter().rev().collect())
        }
        "message.final" => Ok(items(input).filter(|item| item["type"] == "agent_message"
            && item["author"].is_string() && item["recipient"].is_string()
            && items(&item["content"]).any(|part| part["type"] == "input_text" && part["text"].as_str().is_some_and(|text| text.starts_with("Message Type: FINAL_ANSWER\n"))))
            .rev().take(128).map(|item| json!({"author":item["author"],"recipient":item["recipient"],"textDigests":visible(&json!({"input":[item]}))})).collect::<Vec<_>>().into_iter().rev().collect()),
        _ => Err(format!("unknown message evidence method: {method}")),
    }
}
