use crate::{Result, canonical::string};
use serde_json::{Value, json};

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "hostIdentity.meta" => {
            if !input.is_object() {
                return Err("trusted MCP metadata is required".into());
            }
            let caller = string(&input["callerThreadId"]);
            let root = string(&input["rootThreadId"]);
            if caller.is_empty() || root.is_empty() {
                return Err("trusted MCP caller and root thread identities are required".into());
            }
            if input["sessionId"] != root {
                return Err("trusted MCP session and root identities disagree".into());
            }
            let parent = input["parentThreadId"].as_str().filter(|s| !s.is_empty());
            Ok(
                json!({"callerThreadId":caller,"parentThreadId":parent,"rootThreadId":root,"sessionId":root,"turnId":input["turnId"].as_str().filter(|s|!s.is_empty()),"toolCallId":input["callId"].as_str().filter(|s|!s.is_empty()),"source":input["sessionSource"].as_str().filter(|s|!s.is_empty()),"role":input["agentRole"].as_str().filter(|s|!s.is_empty()),"depth":input["agentDepth"].as_u64().unwrap_or(u64::from(parent.is_some())),"kind":if caller==root&&parent.is_none(){"root"}else{"subagent"}}),
            )
        }
        "hostIdentity.authorize" => {
            let identity = &input["identity"];
            let reason = if input["requestedThreadId"] != identity["rootThreadId"] {
                Some("thread_identity_mismatch")
            } else if identity["kind"] == "subagent" && input["operation"] != "read_context" {
                Some("subagent_read_only")
            } else {
                None
            };
            Ok(
                json!({"allowed":reason.is_none(),"reason":reason,"boundThreadId":identity["rootThreadId"]}),
            )
        }
        _ => Err("unknown host identity operation".into()),
    }
}

/// Irreversible cancellation shared by embedding-owned task handles.
#[derive(Default)]
pub struct TaskCancellation {
    cancelled: bool,
}
impl TaskCancellation {
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
    pub fn assert_completable(&self) -> Result<()> {
        if self.cancelled {
            Err("TASK_CANCELLED:生成已停止".into())
        } else {
            Ok(())
        }
    }
}
