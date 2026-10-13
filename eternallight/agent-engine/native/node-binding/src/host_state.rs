use eternallight_engine_core::{HostRun, HostSession};
use napi_derive::napi;

fn parse(input: &str) -> napi::Result<serde_json::Value> {
    if input.len() > 1024 * 1024 {
        return Err(napi::Error::from_reason("host input exceeds byte limit"));
    }
    serde_json::from_str(input).map_err(|_| napi::Error::from_reason("invalid host input"))
}
fn encode(result: Result<serde_json::Value, String>) -> napi::Result<String> {
    serde_json::to_string(&result.map_err(napi::Error::from_reason)?)
        .map_err(|_| napi::Error::from_reason("invalid host result"))
}
#[napi]
pub struct NativeHostRun {
    state: HostRun,
}
#[napi]
impl NativeHostRun {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        Ok(Self {
            state: HostRun::new(&parse(&input)?).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        encode(self.state.apply(&method, &parse(&input)?))
    }
}
#[napi]
pub struct NativeHostSession {
    state: HostSession,
}
#[napi]
impl NativeHostSession {
    #[napi(constructor)]
    pub fn new(state: String) -> napi::Result<Self> {
        Ok(Self {
            state: HostSession::new(&state).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        encode(self.state.apply(&method, &parse(&input)?))
    }
}
