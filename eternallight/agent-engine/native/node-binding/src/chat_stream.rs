use eternallight_engine_core::ChatStream;
use napi_derive::napi;

#[napi]
pub struct NativeChatStream {
    stream: ChatStream,
}

#[napi]
impl NativeChatStream {
    #[napi(constructor)]
    pub fn new(context: String) -> napi::Result<Self> {
        if context.len() > 40 * 1024 * 1024 {
            return Err(napi::Error::from_reason("context exceeds limit"));
        }
        let context = serde_json::from_str(&context)
            .map_err(|_| napi::Error::from_reason("invalid stream context"))?;
        Ok(Self {
            stream: ChatStream::new(context),
        })
    }

    #[napi]
    pub fn push(&mut self, frame: String) -> napi::Result<String> {
        if frame.len() > 8 * 1024 * 1024 {
            self.stream.close();
            return Err(napi::Error::from_reason("frame exceeds limit"));
        }
        let frame = serde_json::from_str(&frame).map_err(|_| {
            self.stream.close();
            napi::Error::from_reason("invalid stream frame")
        })?;
        let events = self.stream.push(frame).map_err(napi::Error::from_reason)?;
        serde_json::to_string(&events)
            .map_err(|_| napi::Error::from_reason("invalid stream events"))
    }

    #[napi]
    pub fn finish(&mut self) -> napi::Result<String> {
        let events = self.stream.finish().map_err(napi::Error::from_reason)?;
        serde_json::to_string(&events)
            .map_err(|_| napi::Error::from_reason("invalid stream events"))
    }

    #[napi]
    pub fn close(&mut self) {
        self.stream.close();
    }
}
