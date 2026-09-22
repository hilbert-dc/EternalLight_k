use napi_derive::napi;
mod app_client;
mod authority;
mod bridge_service;
mod bridge_supervisor;
mod bridge_worker;
pub use bridge_worker::run_bridge_worker;
mod qualification_mcp;
mod qualification_series;
mod trusted_evidence;
pub use qualification_mcp::run_qualification_mcp;
mod behavior_run;
mod bridge_policy;
mod command_executor;
mod host_coordinator;
mod host_state;
mod http_transport;
mod local_http;
mod probe;
mod qualification_run;
mod reconciliation;
mod recovery_journal;
mod session_runtime;
mod task_cancellation;
mod verifier;
pub use command_executor::execute_behavior_command;
mod budget;
mod persistent_budget;
mod process_transport;
mod rpc_pending;
mod storage;
mod trace;
mod turn;
pub use storage::durable_replace_json;
mod chat_stream;
mod responses;
mod sse;

#[napi]
pub fn invoke(method: String, input: String) -> napi::Result<String> {
    if input.len() > 40 * 1024 * 1024 {
        return Err(napi::Error::from_reason("engine request exceeds limit"));
    }
    let value = serde_json::from_str(&input)
        .map_err(|_| napi::Error::from_reason("invalid engine request"))?;
    let result =
        eternallight_engine_core::dispatch(&method, value).map_err(napi::Error::from_reason)?;
    serde_json::to_string(&result).map_err(|_| napi::Error::from_reason("invalid engine response"))
}
