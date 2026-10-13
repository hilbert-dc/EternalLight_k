use eternallight_engine_core::ResponsesStream;
use napi_derive::napi;

#[napi]
pub struct NativeResponsesStream {
    stream: ResponsesStream,
}

#[napi]
impl NativeResponsesStream {
    #[napi(constructor)]
    pub fn new(context: String) -> napi::Result<Self> {
        if context.len() > 40 * 1024 * 1024 {
            return Err(napi::Error::from_reason("context exceeds limit"));
        }
        let context = serde_json::from_str(&context)
            .map_err(|_| napi::Error::from_reason("invalid stream context"))?;
        Ok(Self {
            stream: ResponsesStream::new(context).map_err(napi::Error::from_reason)?,
        })
    }

    #[napi]
    pub fn push(&mut self, event: String) -> napi::Result<String> {
        if event.len() > 40 * 1024 * 1024 {
            self.stream.close();
            return Err(napi::Error::from_reason("event exceeds limit"));
        }
        let event = serde_json::from_str(&event).map_err(|_| {
            self.stream.close();
            napi::Error::from_reason("invalid stream event")
        })?;
        let events = self.stream.push(event).map_err(napi::Error::from_reason)?;
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
