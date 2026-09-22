use crate::{
    Result,
    canonical::{array, string},
    message_evidence,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn evaluate(input: &Value) -> Result<Value> {
    let o = &input["observations"];
    let completed = |name: &str| array(&o["completedOperations"]).iter().any(|v| v == name);
    let selected = |name: &str| array(&o["selectedOperations"]).iter().any(|v| v == name);
    let failed = |name: &str| array(&o["failedOperations"]).iter().any(|v| v == name);
    let yes = |name: &str| o[name] == true;
    let terminal = yes("terminated");
    let mut assertions = Vec::new();
    let mut add = |id: &str, passed: bool, conclusive: bool, observed: Value| {
        assertions.push(json!({"id":id,"result":if passed {"passed"} else if conclusive {"failed"} else {"unknown"},"expected":true,"observed":observed,"traceRef":input["traceRef"]}));
    };
    match string(&input["capability"]) {
        "childSpawn" => {
            add(
                "childSpawn",
                completed("spawn_agent") || yes("childIdentityObserved"),
                terminal || failed("spawn_agent"),
                json!({"selected":selected("spawn_agent"),"completedEvent":completed("spawn_agent"),"trustedChildObserved":yes("childIdentityObserved"),"failed":failed("spawn_agent")}),
            );
            add(
                "trusted-child-identity",
                yes("childIdentityObserved"),
                terminal || completed("spawn_agent"),
                json!(yes("childIdentityObserved")),
            );
        }
        "childMessageRunning" => {
            add(
                "childMessageRunning",
                completed("send_message") || yes("childResultObserved"),
                terminal || failed("send_message"),
                json!({"selected":selected("send_message"),"completedEvent":completed("send_message"),"followupConsumed":yes("childResultObserved"),"failed":failed("send_message")}),
            );
            add(
                "child-running-gate-observed",
                yes("childGateObserved"),
                terminal || completed("spawn_agent"),
                json!(yes("childGateObserved")),
            );
            add(
                "parent-followup-token-queued",
                yes("parentFollowupTokenQueued"),
                terminal || completed("send_message"),
                json!(yes("parentFollowupTokenQueued")),
            );
            add(
                "child-followup-visible",
                yes("childFollowupVisible"),
                terminal,
                json!(yes("childFollowupVisible")),
            );
            add(
                "child-followup-consumed",
                yes("childResultObserved"),
                terminal,
                json!(yes("childResultObserved")),
            );
        }
        "childWait" => {
            add(
                "childWait",
                completed("wait_agent"),
                terminal || failed("wait_agent"),
                json!({"selected":selected("wait_agent"),"completed":completed("wait_agent"),"failed":failed("wait_agent")}),
            );
            add(
                "child-terminal-result-observed",
                yes("childTerminalResultObserved"),
                terminal,
                json!(yes("childTerminalResultObserved")),
            );
        }
        "childDirectedRoute" => {
            add(
                "directed-child-model-observed",
                yes("directedRouteObserved"),
                terminal || completed("spawn_agent"),
                json!(yes("directedRouteObserved")),
            );
            add(
                "trusted-child-identity",
                yes("childIdentityObserved"),
                terminal || completed("spawn_agent"),
                json!(yes("childIdentityObserved")),
            );
        }
        capability => return Err(format!("unsupported delegation capability: {capability}")),
    }
    let status = if assertions.iter().all(|a| a["result"] == "passed") {
        "verified"
    } else if assertions.iter().any(|a| a["result"] == "failed") {
        "failed"
    } else {
        "unknown"
    };
    Ok(json!({"status":status,"assertions":assertions}))
}

fn operation(value: &Value) -> Option<&'static str> {
    let text = string(value).to_lowercase();
    if text.contains("spawn") {
        Some("spawn_agent")
    } else if text.contains("send") || text.contains("message") {
        Some("send_message")
    } else if text.contains("wait") {
        Some("wait_agent")
    } else {
        None
    }
}

fn summary(input: &Value) -> Value {
    let mut selected = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut failed = BTreeSet::new();
    for response in array(&input["responses"]) {
        for name in array(&response["toolNames"]) {
            if let Some(op) = operation(name) {
                selected.insert(op);
            }
        }
    }
    for event in array(&input["events"]) {
        let item = &event["params"]["item"];
        if item["type"] != "collabAgentToolCall" {
            continue;
        }
        let Some(op) = operation(
            item.get("tool")
                .filter(|v| !v.is_null())
                .unwrap_or(&item["action"]),
        ) else {
            continue;
        };
        selected.insert(op);
        if event["method"] == "item/completed" {
            if matches!(item["status"].as_str(), Some("failed" | "cancelled")) {
                failed.insert(op);
            } else {
                completed.insert(op);
            }
        }
    }
    json!({"selectedOperations":selected,"completedOperations":completed,"failedOperations":failed})
}

fn terminal_wait(input: &Value) -> Result<Value> {
    let expected = string(&input["expectedText"]);
    let hash = message_evidence::dispatch("message.digest", &json!(expected))?;
    let events = array(&input["events"]);
    let children = array(&input["childRequests"]);
    for event in events {
        let item = &event["params"]["item"];
        let parent = &event["params"]["threadId"];
        if event["method"] != "item/completed"
            || item["type"] != "collabAgentToolCall"
            || !string(&item["tool"]).to_lowercase().contains("wait")
            || item["status"] != "completed"
        {
            continue;
        }
        let state_proof =
            item["agentsStates"]
                .as_object()
                .into_iter()
                .flatten()
                .any(|(thread, state)| {
                    state["status"] == "completed"
                        && string(&state["message"]).trim() == expected
                        && children
                            .iter()
                            .any(|r| r["threadId"] == *thread && r["parentThreadId"] == *parent)
                });
        if state_proof {
            return Ok(true.into());
        }
        let mailbox = array(&input["responses"]).iter().any(|response| {
            response["parentThreadId"] == *parent
                && response["status"] == "completed"
                && response["finalTextDigest"] == hash
                && children.iter().any(|r| {
                    r["threadId"] == response["threadId"]
                        && r["parentThreadId"] == response["parentThreadId"]
                })
                && events.iter().any(|delivery| {
                    delivery["method"] == "item/completed"
                        && delivery["params"]["threadId"] == *parent
                        && delivery["params"]["item"]["type"] == "subAgentActivity"
                        && delivery["params"]["item"]["kind"] == "completed"
                        && delivery["params"]["item"]["agentThreadId"] == response["threadId"]
                        && array(&input["requests"]).iter().any(|request| {
                            request["threadId"] == *parent
                                && array(&request["waitResults"]).iter().any(|result| {
                                    result["callId"] == item["id"] && result["completed"] == true
                                })
                                && array(&request["finalAgentMessages"]).iter().any(|message| {
                                    message["author"] == delivery["params"]["item"]["agentPath"]
                                        && array(&message["textDigests"]).contains(&hash)
                                })
                        })
                })
        });
        if mailbox {
            return Ok(true.into());
        }
    }
    Ok(false.into())
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "delegation.evaluate" => evaluate(input),
        "delegation.summary" => Ok(summary(input)),
        "delegation.terminalWait" => terminal_wait(input),
        "delegation.followupDiagnostics" => Ok(crate::qualification_mcp::diagnostics(
            &input["actual"],
            string(&input["expected"]),
        )),
        _ => Err("unknown delegation operation".into()),
    }
}
