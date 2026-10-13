use eternallight_engine_core::ProcessTransport;
use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use std::sync::Arc;

#[napi]
pub struct NativeProcessTransport {
    transport: Arc<ProcessTransport>,
}

pub struct CloseTask {
    transport: Arc<ProcessTransport>,
    timeout_ms: u64,
}

impl Task for CloseTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> napi::Result<()> {
        self.transport
            .close(self.timeout_ms)
            .map_err(napi::Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, _output: ()) -> napi::Result<()> {
        Ok(())
    }
}

#[napi]
impl NativeProcessTransport {
    #[napi(getter)]
    pub fn pid(&self) -> napi::Result<u32> {
        self.transport
            .process_id()
            .map_err(napi::Error::from_reason)
    }
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        if input.len() > 2 * 1024 * 1024 {
            return Err(napi::Error::from_reason("process options exceed limit"));
        }
        let options = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid process options"))?;
        Ok(Self {
            transport: Arc::new(
                ProcessTransport::start(options).map_err(napi::Error::from_reason)?,
            ),
        })
    }
    #[napi]
    pub fn send(&self, message: String) -> napi::Result<()> {
        self.transport
            .send(&message)
            .map_err(napi::Error::from_reason)
    }
    #[napi]
    pub fn drain(&self) -> napi::Result<String> {
        serde_json::to_string(&self.transport.drain().map_err(napi::Error::from_reason)?)
            .map_err(|_| napi::Error::from_reason("invalid process event"))
    }
    #[napi]
    pub fn close(&self, timeout_ms: u32) -> AsyncTask<CloseTask> {
        AsyncTask::new(CloseTask {
            transport: self.transport.clone(),
            timeout_ms: timeout_ms.into(),
        })
    }
}
