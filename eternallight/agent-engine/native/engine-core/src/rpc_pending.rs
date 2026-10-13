use crate::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Pending {
    generation: u64,
    method: String,
    deadline: u64,
}

/// Correlates only responses from the process generation that accepted a call.
/// The binding retains promises, but cannot use old responses to settle new calls.
#[derive(Default)]
pub struct RpcPending {
    next_id: u64,
    pending: BTreeMap<u64, Pending>,
}

impl RpcPending {
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "register" => {
                if self.pending.len() >= 1024 {
                    return Err("App Server pending request limit exceeded".into());
                }
                let name = input["method"]
                    .as_str()
                    .filter(|name| !name.is_empty() && name.len() <= 200)
                    .ok_or("invalid App Server RPC method")?;
                let generation = input["generation"]
                    .as_u64()
                    .ok_or("invalid App Server process generation")?;
                let now = input["now"].as_u64().ok_or("invalid App Server clock")?;
                let timeout = input["timeoutMs"]
                    .as_u64()
                    .filter(|t| *t > 0)
                    .ok_or("invalid App Server request timeout")?;
                self.next_id = self
                    .next_id
                    .checked_add(1)
                    .filter(|id| *id <= 9_007_199_254_740_991)
                    .ok_or("App Server request identity exhausted")?;
                self.pending.insert(
                    self.next_id,
                    Pending {
                        generation,
                        method: name.into(),
                        deadline: now.saturating_add(timeout),
                    },
                );
                Ok(self.next_id.into())
            }
            "response" => {
                let message = &input["message"];
                if message["method"].is_string() {
                    return Ok(Value::Null);
                }
                let Some(id) = message["id"].as_u64() else {
                    return Ok(Value::Null);
                };
                if self
                    .pending
                    .get(&id)
                    .is_none_or(|pending| Some(pending.generation) != input["generation"].as_u64())
                {
                    return Ok(Value::Null);
                }
                let pending = self
                    .pending
                    .remove(&id)
                    .ok_or("missing App Server request")?;
                if !message["error"].is_null() {
                    let fallback = format!("Codex request failed: {}", pending.method);
                    Ok(
                        json!({"id":id,"error":message["error"]["message"].as_str().unwrap_or(&fallback),"rpcCode":message["error"]["code"].as_i64()}),
                    )
                } else {
                    Ok(json!({"id":id,"result":message["result"]}))
                }
            }
            "expire" => {
                let now = input["now"].as_u64().ok_or("invalid App Server clock")?;
                let expired: Vec<_> = self.pending.iter().filter(|(_,p)|p.deadline<=now).map(|(id,p)|json!({"id":id,"error":format!("Codex App Server request timed out: {}",p.method)})).collect();
                self.pending.retain(|_, p| p.deadline > now);
                Ok(expired.into())
            }
            "cancel" => {
                if let Some(id) = input["id"].as_u64() {
                    self.pending.remove(&id);
                }
                Ok(Value::Null)
            }
            "clear" => {
                self.pending.clear();
                Ok(Value::Null)
            }
            _ => Err("unknown App Server RPC pending operation".into()),
        }
    }
}
