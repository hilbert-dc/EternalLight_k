use eternallight_engine_core::BridgePolicy;
use napi_derive::napi;

#[napi]
pub struct NativeBridgePolicy {
    policy: BridgePolicy,
}

#[napi]
impl NativeBridgePolicy {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            policy: BridgePolicy::default(),
        }
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 4 * 1024 * 1024 {
            return Err(napi::Error::from_reason(
                "bridge policy input exceeds limit",
            ));
        }
        let value = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid bridge policy input"))?;
        self.policy
            .apply(&method, &value)
            .map(|value| value.to_string())
            .map_err(napi::Error::from_reason)
    }
}
