use eternallight_engine_core::HostCoordinator;
use napi_derive::napi;
#[napi]
pub struct NativeHostCoordinator {
    state: HostCoordinator,
}
#[napi]
impl NativeHostCoordinator {
    #[napi(constructor)]
    pub fn new(maximum: u32) -> napi::Result<Self> {
        Ok(Self {
            state: HostCoordinator::new(maximum.into()).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 4 * 1024 * 1024 {
            return Err(napi::Error::from_reason(
                "host coordinator input exceeds limit",
            ));
        }
        let value = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid host coordinator input"))?;
        serde_json::to_string(
            &self
                .state
                .apply(&method, &value)
                .map_err(napi::Error::from_reason)?,
        )
        .map_err(|_| napi::Error::from_reason("invalid host coordinator result"))
    }
}
