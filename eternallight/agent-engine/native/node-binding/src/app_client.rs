use eternallight_engine_core::AppClient;
use napi_derive::napi;

fn parse(input: &str) -> napi::Result<serde_json::Value> {
    if input.len() > 2 * 1024 * 1024 {
        return Err(napi::Error::from_reason(
            "App Server client input exceeds byte limit",
        ));
    }
    serde_json::from_str(input)
        .map_err(|_| napi::Error::from_reason("invalid App Server client input"))
}
#[napi]
pub struct NativeAppClient {
    state: AppClient,
}
#[napi]
impl NativeAppClient {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        Ok(Self {
            state: AppClient::new(parse(&input)?).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        let value = self
            .state
            .apply(&method, &parse(&input)?)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&value)
            .map_err(|_| napi::Error::from_reason("invalid App Server client result"))
    }
}
