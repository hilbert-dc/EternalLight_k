use eternallight_engine_core::MediaLedger;
use napi_derive::napi;
use std::path::PathBuf;

#[napi]
pub struct NativeMediaLedger {
    ledger: MediaLedger,
}

#[napi]
impl NativeMediaLedger {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        Ok(Self {
            ledger: MediaLedger::new(&PathBuf::from(path)).map_err(napi::Error::from_reason)?,
        })
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason(
                "media ledger request exceeds limit",
            ));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid media ledger request"))?;
        let result = self
            .ledger
            .apply(&method, &input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result)
            .map_err(|_| napi::Error::from_reason("invalid media ledger result"))
    }
}
