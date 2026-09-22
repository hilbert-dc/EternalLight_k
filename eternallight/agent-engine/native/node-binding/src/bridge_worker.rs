use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
pub struct BridgeWorkerTask;
impl Task for BridgeWorkerTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> napi::Result<()> {
        eternallight_engine_core::run_bridge_worker().map_err(napi::Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, _output: ()) -> napi::Result<()> {
        Ok(())
    }
}
#[napi]
pub fn run_bridge_worker() -> AsyncTask<BridgeWorkerTask> {
    AsyncTask::new(BridgeWorkerTask)
}
