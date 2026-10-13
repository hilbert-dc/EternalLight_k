use eternallight_engine_core::EvidenceSnapshot;
use napi_derive::napi;
use serde_json::Value;

fn parse(input: &str) -> napi::Result<Value> {
    if input.len() > 32 * 1024 * 1024 {
        return Err(napi::Error::from_reason("evidence input exceeds limit"));
    }
    serde_json::from_str(input).map_err(|_| napi::Error::from_reason("invalid evidence input"))
}
#[napi]
pub struct NativeEvidenceSnapshot {
    snapshot: EvidenceSnapshot,
}
#[napi]
impl NativeEvidenceSnapshot {
    #[napi(constructor)]
    pub fn new(records: String) -> napi::Result<Self> {
        Ok(Self {
            snapshot: EvidenceSnapshot::new(&parse(&records)?).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi(factory)]
    pub fn from_reports(input: String) -> napi::Result<Self> {
        Ok(Self {
            snapshot: EvidenceSnapshot::from_reports(&parse(&input)?)
                .map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&self, method: String, input: String) -> napi::Result<String> {
        self.snapshot
            .apply(&method, &parse(&input)?)
            .map(|v| v.to_string())
            .map_err(napi::Error::from_reason)
    }
}
