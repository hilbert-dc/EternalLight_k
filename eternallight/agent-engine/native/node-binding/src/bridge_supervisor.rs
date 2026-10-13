use eternallight_engine_core::BridgeSupervisor;
use napi_derive::napi;
fn parse(input: &str) -> napi::Result<serde_json::Value> {
    if input.len() > 4 * 1024 * 1024 {
        return Err(napi::Error::from_reason(
            "bridge supervisor input exceeds limit",
        ));
    }
    serde_json::from_str(input)
        .map_err(|_| napi::Error::from_reason("invalid bridge supervisor input"))
}
#[napi]
pub struct NativeBridgeSupervisor {
    state: BridgeSupervisor,
}
#[napi]
impl NativeBridgeSupervisor {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        Ok(Self {
            state: BridgeSupervisor::new(parse(&input)?).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        serde_json::to_string(
            &self
                .state
                .apply(&method, &parse(&input)?)
                .map_err(napi::Error::from_reason)?,
        )
        .map_err(|_| napi::Error::from_reason("invalid bridge supervisor output"))
    }
}
