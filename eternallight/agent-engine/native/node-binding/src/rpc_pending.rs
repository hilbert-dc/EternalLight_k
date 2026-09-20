use eternallight_engine_core::RpcPending;
use napi_derive::napi;

#[napi]
#[derive(Default)]
pub struct NativeRpcPending {
    pending: RpcPending,
}

#[napi]
impl NativeRpcPending {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("RPC input exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid RPC input"))?;
        let output = self
            .pending
            .apply(&method, &input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&output).map_err(|_| napi::Error::from_reason("invalid RPC result"))
    }
}
