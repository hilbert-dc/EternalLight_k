use crate::Result;
use serde_json::{Value, json};

/// State for one App Server turn, including late-start cancellation and
/// generation-scoped message assembly. Promise delivery belongs to the binding.
#[derive(Default)]
pub struct AppServerTurn {
    generation: Option<u64>,
    thread: Option<String>,
    turn: Option<String>,
    cancelled: bool,
    terminal: bool,
    message: String,
    item: Option<String>,
}

fn bounded(text: &str) -> String {
    let mut length = 0;
    text.chars()
        .take_while(|c| {
            length += c.len_utf16();
            length <= 64 * 1024
        })
        .collect()
}

impl AppServerTurn {
    pub fn bind_thread(&mut self, generation: u64, thread: String) -> Result<()> {
        if self.terminal || self.thread.is_some() || thread.is_empty() {
            return Err("turn thread binding is invalid".into());
        }
        self.generation = Some(generation);
        self.thread = Some(thread);
        Ok(())
    }

    pub fn bind_started(&mut self, turn: Option<String>) -> Result<bool> {
        if self.terminal {
            return Ok(false);
        }
        if let Some(turn) = turn.filter(|id| !id.is_empty()) {
            if self.turn.as_ref().is_some_and(|current| current != &turn) {
                return Err("App Server turn identity changed".into());
            }
            self.turn = Some(turn);
        }
        Ok(self.cancelled && self.turn.is_some())
    }

    pub fn cancel(&mut self) {
        if !self.terminal {
            self.cancelled = true;
        }
    }
    pub fn finish(&mut self) {
        self.terminal = true;
    }

    pub fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"threadId":self.thread,"turnId":self.turn,"cancellationRequested":self.cancelled,"terminal":self.terminal})
    }

    pub fn observe(&mut self, generation: u64, message: &Value) -> Result<Value> {
        let mut result = json!({"interrupt":false,"completion":null});
        if self.terminal
            || self.generation != Some(generation)
            || message["params"]["threadId"].as_str() != self.thread.as_deref()
        {
            return Ok(result);
        }
        let params = &message["params"];
        if message["method"] == "turn/started" {
            result["interrupt"] = self
                .bind_started(params["turn"]["id"].as_str().map(str::to_owned))?
                .into();
        }
        let event_turn = params["turnId"]
            .as_str()
            .or_else(|| params["turn"]["id"].as_str());
        if self.turn.is_none() || event_turn != self.turn.as_deref() {
            return Ok(result);
        }
        match message["method"].as_str() {
            Some("item/agentMessage/delta") => {
                if let Some(delta) = params["delta"].as_str() {
                    let item = params["itemId"].as_str().map(str::to_owned);
                    if self.item != item {
                        self.item = item;
                        self.message.clear();
                    }
                    self.message = bounded(&format!("{}{delta}", self.message));
                }
            }
            Some("item/completed") if params["item"]["type"] == "agentMessage" => {
                let item = &params["item"];
                if let Some(text) = item["text"].as_str() {
                    self.message = bounded(text);
                } else if let Some(content) = item["content"].as_array() {
                    let text: String = content
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect();
                    if !text.is_empty() {
                        self.message = bounded(&text);
                    }
                }
            }
            Some("turn/completed") if params["turn"]["id"].as_str() == self.turn.as_deref() => {
                // The binding finishes delivery and then marks this state terminal.
                result["completion"] = if params["turn"]["status"] == "failed" {
                    json!({"error":params["turn"]["error"]["message"].as_str().unwrap_or("Codex turn failed")})
                } else {
                    json!({"result":{"mode":"agent-engine","message":self.message,"turn":params["turn"]}})
                };
            }
            _ => {}
        }
        Ok(result)
    }
}
