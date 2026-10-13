use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use serde_json::Value;

pub struct CommandTask {
    input: Value,
}
impl Task for CommandTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> napi::Result<String> {
        let result = eternallight_engine_core::execute_behavior_command(&self.input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result)
            .map_err(|_| napi::Error::from_reason("invalid behavior result"))
    }
    fn resolve(&mut self, _env: Env, output: String) -> napi::Result<String> {
        Ok(output)
    }
}

#[napi]
pub fn execute_behavior_command(input: String) -> napi::Result<AsyncTask<CommandTask>> {
    if input.len() > 4 * 1024 * 1024 {
        return Err(napi::Error::from_reason(
            "behavior command input exceeds limit",
        ));
    }
    let input = serde_json::from_str(&input)
        .map_err(|_| napi::Error::from_reason("invalid behavior command input"))?;
    Ok(AsyncTask::new(CommandTask { input }))
}
