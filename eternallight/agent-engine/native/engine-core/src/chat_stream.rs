use crate::{Result, canonical, portable};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Accumulates candidate tool calls without releasing executable items before EOF.
pub struct ChatStream {
    context: Value,
    id: Option<String>,
    text: String,
    refusal: String,
    calls: BTreeMap<u64, Value>,
    usage: Value,
    finish_reason: Option<String>,
    done: bool,
    closed: bool,
    bytes: usize,
}

impl ChatStream {
    pub fn new(context: Value) -> Self {
        Self {
            context,
            id: None,
            text: String::new(),
            refusal: String::new(),
            calls: BTreeMap::new(),
            usage: Value::Null,
            finish_reason: None,
            done: false,
            closed: false,
            bytes: 0,
        }
    }

    pub fn push(&mut self, frame: Value) -> Result<Vec<Value>> {
        let result = self.consume(frame);
        if result.is_err() {
            self.close();
        }
        result
    }

    fn consume(&mut self, frame: Value) -> Result<Vec<Value>> {
        if self.closed {
            return Err("chat stream closed".into());
        }
        self.bytes += frame.to_string().len();
        if self.bytes > 8 * 1024 * 1024 {
            return Err("streaming response exceeds codec limit".into());
        }
        if frame["done"] == true {
            self.done = true;
            return Ok(Vec::new());
        }
        if self.done {
            return Err("chat-completions stream emitted data after [DONE]".into());
        }
        let payload = &frame["data"];
        if !payload["error"].is_null() {
            return Err("chat-completions provider returned an error frame".into());
        }
        let mut events = Vec::new();
        if self.id.is_none() {
            self.id = Some(
                payload["id"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                    .unwrap_or("response_chat_completions")
                    .into(),
            );
            events.push(json!({"type":"response.created","response":{"id":self.id}}));
        }
        if !payload["usage"].is_null() {
            self.usage = payload["usage"].clone();
        }
        let choice = &payload["choices"][0];
        if let Some(reason) = choice["finish_reason"].as_str() {
            if self
                .finish_reason
                .as_deref()
                .is_some_and(|previous| previous != reason)
            {
                return Err("chat-completions stream changed finish reason".into());
            }
            self.finish_reason = Some(reason.into());
        }
        let delta = &choice["delta"];
        if !canonical::string(&delta["reasoning_content"]).is_empty() {
            return Err(canonical::unsupported(
                "chat reasoning_content requires an opaque state-preserving route",
            ));
        }
        self.refusal.push_str(canonical::string(&delta["refusal"]));
        let text = canonical::string(&delta["content"]);
        if !text.is_empty() {
            let item_id = format!("msg_{}", self.id.as_deref().unwrap());
            if self.text.is_empty() {
                events.push(json!({"type":"response.output_item.added","output_index":0,"item":{"id":item_id,"type":"message","role":"assistant","status":"in_progress","content":[]}}));
            }
            self.text.push_str(text);
            events.push(json!({"type":"response.output_text.delta","output_index":0,"item_id":item_id,"content_index":0,"delta":text}));
        }
        for partial in canonical::array(&delta["tool_calls"]) {
            let index = partial["index"]
                .as_u64()
                .ok_or("chat-completions tool delta requires a non-negative index")?;
            if !self.calls.contains_key(&index) && self.calls.len() >= 1024 {
                return Err("too many streamed tool calls".into());
            }
            let call = self.calls.entry(index).or_insert_with(
                || json!({"id":null,"type":"function","function":{"name":"","arguments":""}}),
            );
            if let Some(id) = partial["id"].as_str().filter(|v| !v.is_empty()) {
                if call["id"].as_str().is_some_and(|previous| previous != id) {
                    return Err("chat-completions tool call id changed during streaming".into());
                }
                call["id"] = id.into();
            }
            for key in ["name", "arguments"] {
                let append = canonical::string(&partial["function"][key]);
                if !append.is_empty() {
                    call["function"][key] =
                        format!("{}{append}", canonical::string(&call["function"][key])).into();
                }
            }
            let args = canonical::string(&partial["function"]["arguments"]);
            if !args.is_empty() {
                let item_id = call["id"]
                    .as_str()
                    .map(|id| format!("fc_{id}"))
                    .unwrap_or_else(|| format!("fc_pending_{index}"));
                let output_index = index
                    .checked_add(u64::from(!self.text.is_empty()))
                    .ok_or("tool index exceeds limit")?;
                events.push(json!({"type":"response.function_call_arguments.delta","output_index":output_index,"item_id":item_id,"delta":args}));
            }
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<Value>> {
        let result = self.commit();
        self.close();
        result
    }

    fn commit(&self) -> Result<Vec<Value>> {
        if self.closed {
            return Err("chat stream closed".into());
        }
        if !self.done {
            return Err("chat-completions stream ended without [DONE]".into());
        }
        for call in self.calls.values() {
            if canonical::string(&call["id"]).is_empty() {
                return Err("chat-completions stream ended with a missing tool call id".into());
            }
            if canonical::string(&call["function"]["name"]).is_empty() {
                return Err("chat-completions stream ended with an unnamed tool call".into());
            }
        }
        let id = self.id.as_deref().unwrap_or("response_chat_completions");
        let payload = json!({"id":id,"usage":self.usage,"choices":[{"finish_reason":self.finish_reason,"message":{"content":self.text,"refusal":self.refusal,"tool_calls":self.calls.values().collect::<Vec<_>>()}}]});
        let mut response = portable::decode("chat-completions", &payload, &self.context)?;
        for item in response["output"].as_array_mut().unwrap() {
            if item["type"] == "function_call" {
                item["id"] = format!("fc_{}", canonical::string(&item["call_id"])).into();
            }
        }
        if !self.text.is_empty() {
            response["output"][0]["id"] = format!("msg_{id}").into();
        }
        let mut events = canonical::events(&response).as_array().unwrap().clone();
        if self.id.is_some() {
            events.remove(0);
        }
        Ok(events)
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.context = Value::Null;
        self.text.clear();
        self.refusal.clear();
        self.calls.clear();
        self.usage = Value::Null;
    }
}
