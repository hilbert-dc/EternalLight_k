use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use serde_json::Value;

pub struct McpTask {
    input: Value,
}
impl Task for McpTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> napi::Result<()> {
        eternallight_engine_core::run_qualification_mcp(self.input.take())
            .map_err(napi::Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, _output: ()) -> napi::Result<()> {
        Ok(())
    }
}
#[napi]
pub fn run_qualification_mcp(input: String) -> napi::Result<AsyncTask<McpTask>> {
    if input.len() > 64 * 1024 {
        return Err(napi::Error::from_reason("MCP options exceed limit"));
    }
    let input = serde_json::from_str(&input)
        .map_err(|_| napi::Error::from_reason("invalid MCP options"))?;
    Ok(AsyncTask::new(McpTask { input }))
}
