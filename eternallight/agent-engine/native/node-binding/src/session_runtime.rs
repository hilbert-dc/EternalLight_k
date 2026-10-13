use eternallight_engine_core::RuntimeCoordinator;
use napi_derive::napi;

#[napi]
pub struct NativeSessionRuntime {
    state: RuntimeCoordinator,
}

#[napi]
impl NativeSessionRuntime {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            state: RuntimeCoordinator::default(),
        }
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 8 * 1024 * 1024 {
            return Err(napi::Error::from_reason(
                "session runtime input exceeds limit",
            ));
        }
        let value = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid session runtime input"))?;
        self.state
            .apply(&method, &value)
            .map(|value| value.to_string())
            .map_err(napi::Error::from_reason)
    }
}
