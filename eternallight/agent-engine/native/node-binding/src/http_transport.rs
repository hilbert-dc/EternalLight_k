use eternallight_engine_core::{HttpEvent, HttpTransport};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

#[napi(object)]
pub struct NativeHttpEvent {
    pub kind: String,
    pub status: Option<u32>,
    pub headers: Option<String>,
    pub chunk: Option<Buffer>,
    pub error: Option<String>,
}

#[napi]
pub struct NativeHttpTransport {
    transport: HttpTransport,
}

#[napi]
impl NativeHttpTransport {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        if input.len() > 8 * 1024 * 1024 {
            return Err(napi::Error::from_reason("HTTP options exceed limit"));
        }
        let options = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid HTTP options"))?;
        Ok(Self {
            transport: HttpTransport::start(options).map_err(napi::Error::from_reason)?,
        })
    }

    #[napi]
    pub fn poll(&mut self) -> napi::Result<Option<NativeHttpEvent>> {
        let Some(event) = self.transport.poll() else {
            return Ok(None);
        };
        let mut result = NativeHttpEvent {
            kind: String::new(),
            status: None,
            headers: None,
            chunk: None,
            error: None,
        };
        match event {
            HttpEvent::Headers { status, headers } => {
                result.kind = "headers".into();
                result.status = Some(status.into());
                result.headers = Some(
                    serde_json::to_string(&headers)
                        .map_err(|_| napi::Error::from_reason("invalid HTTP headers"))?,
                );
            }
            HttpEvent::Chunk(chunk) => {
                result.kind = "chunk".into();
                result.chunk = Some(chunk.into());
            }
            HttpEvent::End => result.kind = "end".into(),
            HttpEvent::Error(error) => {
                result.kind = "error".into();
                result.error = Some(error);
            }
        }
        Ok(Some(result))
    }

    #[napi]
    pub fn cancel(&mut self) {
        self.transport.cancel();
    }
}
