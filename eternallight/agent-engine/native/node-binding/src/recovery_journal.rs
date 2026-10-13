use eternallight_engine_core::RecoveryJournal;
use napi_derive::napi;

#[napi]
#[derive(Default)]
pub struct NativeRecoveryJournal {
    journal: RecoveryJournal,
}

#[napi]
impl NativeRecoveryJournal {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 4 * 1024 * 1024 {
            return Err(napi::Error::from_reason("recovery event exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid recovery event"))?;
        let output = self
            .journal
            .apply(&method, &input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&output)
            .map_err(|_| napi::Error::from_reason("invalid recovery journal result"))
    }
}
