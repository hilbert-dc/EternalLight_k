use napi_derive::napi;
mod authority;
mod behavior_run;
mod command_executor;
mod http_transport;
mod recovery_journal;
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
