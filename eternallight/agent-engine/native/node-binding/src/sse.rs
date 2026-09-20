use eternallight_engine_core::SseDecoder;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

#[napi]
pub struct NativeSseDecoder {
    decoder: SseDecoder,
}

#[napi]
impl NativeSseDecoder {
    #[napi(constructor)]
    pub fn new(limit: u32) -> napi::Result<Self> {
        Ok(Self {
            decoder: SseDecoder::new(limit as usize).map_err(napi::Error::from_reason)?,
        })
    }

    #[napi]
    pub fn push(&mut self, bytes: Buffer) -> napi::Result<String> {
        let events = self
            .decoder
            .push(&bytes)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&events).map_err(|_| napi::Error::from_reason("invalid SSE events"))
    }

    #[napi]
    pub fn finish(&mut self) -> napi::Result<String> {
        let events = self.decoder.finish().map_err(napi::Error::from_reason)?;
        serde_json::to_string(&events).map_err(|_| napi::Error::from_reason("invalid SSE events"))
    }

    #[napi]
    pub fn close(&mut self) {
        self.decoder.close();
    }
}
