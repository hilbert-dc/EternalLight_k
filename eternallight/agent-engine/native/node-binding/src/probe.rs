use eternallight_engine_core::ProbeSession;
use napi_derive::napi;

#[napi]
pub struct NativeProbeSession {
    session: ProbeSession,
}

fn parse(input: &str) -> napi::Result<serde_json::Value> {
    if input.len() > 64 * 1024 {
        return Err(napi::Error::from_reason("probe input exceeds limit"));
    }
    serde_json::from_str(input).map_err(|_| napi::Error::from_reason("invalid probe input"))
}

#[napi]
impl NativeProbeSession {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        Ok(Self {
            session: ProbeSession::new(&parse(&input)?).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        let result = self
            .session
            .apply(&method, &parse(&input)?)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result).map_err(|_| napi::Error::from_reason("invalid probe result"))
    }
}
