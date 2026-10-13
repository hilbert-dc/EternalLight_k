use eternallight_engine_core::Reconciliation;
use napi_derive::napi;
use serde_json::Value;

fn parse(input: &str) -> napi::Result<Value> {
    if input.len() > 8 * 1024 * 1024 {
        return Err(napi::Error::from_reason(
            "reconciliation input exceeds limit",
        ));
    }
    serde_json::from_str(input)
        .map_err(|_| napi::Error::from_reason("invalid reconciliation input"))
}

#[napi]
pub struct NativeReconciliation {
    state: Reconciliation,
}

#[napi]
impl NativeReconciliation {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        Ok(Self {
            state: Reconciliation::new(&parse(&input)?).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn next(&self) -> String {
        self.state.next().to_string()
    }
    #[napi]
    pub fn accept(&mut self, input: String) -> napi::Result<String> {
        self.state
            .accept(&parse(&input)?)
            .map(|value| value.to_string())
            .map_err(napi::Error::from_reason)
    }
}
