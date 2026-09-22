use eternallight_engine_core::BridgeService;
use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use std::sync::Arc;

#[napi]
pub struct NativeBridgeService {
    service: Arc<BridgeService>,
}
pub struct BridgeWait {
    service: Arc<BridgeService>,
    operation: WaitOperation,
}
enum WaitOperation {
    Drain,
    Close,
}
impl Task for BridgeWait {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> napi::Result<()> {
        match self.operation {
            WaitOperation::Drain => self.service.drain(),
            WaitOperation::Close => self.service.close(),
        }
        .map_err(napi::Error::from_reason)
    }
    fn resolve(&mut self, _env: Env, _output: ()) -> napi::Result<()> {
        Ok(())
    }
}
#[napi]
impl NativeBridgeService {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        if input.len() > 2 * 1024 * 1024 {
            return Err(napi::Error::from_reason("bridge options exceed byte limit"));
        }
        let options = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid bridge options"))?;
        Ok(Self {
            service: Arc::new(BridgeService::start(options).map_err(napi::Error::from_reason)?),
        })
    }
    #[napi]
    pub fn apply(&self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 40 * 1024 * 1024 {
            return Err(napi::Error::from_reason(
                "bridge callback exceeds byte limit",
            ));
        }
        let value = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid bridge callback"))?;
        serde_json::to_string(
            &self
                .service
                .apply(&method, &value)
                .map_err(napi::Error::from_reason)?,
        )
        .map_err(|_| napi::Error::from_reason("invalid bridge effect"))
    }
    #[napi]
    pub fn drain(&self) -> AsyncTask<BridgeWait> {
        AsyncTask::new(BridgeWait {
            service: self.service.clone(),
            operation: WaitOperation::Drain,
        })
    }
    #[napi]
    pub fn close(&self) -> AsyncTask<BridgeWait> {
        AsyncTask::new(BridgeWait {
            service: self.service.clone(),
            operation: WaitOperation::Close,
        })
    }
}
