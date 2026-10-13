use eternallight_engine_core::LocalHttpServer;
use napi::{Error, bindgen_prelude::Buffer};
use napi_derive::napi;

#[napi]
pub struct NativeLocalHttpServer {
    server: LocalHttpServer,
}

#[napi]
impl NativeLocalHttpServer {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        if input.len() > 16 * 1024 {
            return Err(Error::from_reason("listener options exceed limit"));
        }
        let options = serde_json::from_str(&input)
            .map_err(|_| Error::from_reason("invalid listener options"))?;
        Ok(Self {
            server: LocalHttpServer::start(options).map_err(Error::from_reason)?,
        })
    }
    #[napi(getter)]
    pub fn port(&self) -> u32 {
        self.server.port().into()
    }
    #[napi]
    pub fn poll(&mut self) -> napi::Result<Option<String>> {
        self.server
            .poll()
            .map(|value| {
                serde_json::to_string(&value)
                    .map_err(|_| Error::from_reason("invalid listener request"))
            })
            .transpose()
    }
    #[napi]
    pub fn closed(&self, id: f64) -> bool {
        self.server.closed(id as u64)
    }
    #[napi]
    pub fn head(&mut self, id: f64, status: u32, headers: String) -> napi::Result<()> {
        if headers.len() > 32 * 1024 || !(100..=599).contains(&status) {
            return Err(Error::from_reason("invalid listener response headers"));
        }
        self.server
            .head(
                id as u64,
                status as u16,
                serde_json::from_str(&headers)
                    .map_err(|_| Error::from_reason("invalid listener headers"))?,
            )
            .map_err(Error::from_reason)
    }
    #[napi]
    pub fn write(&mut self, id: f64, chunk: Buffer) -> napi::Result<bool> {
        self.server
            .write(id as u64, &chunk)
            .map_err(Error::from_reason)
    }
    #[napi]
    pub fn end(&mut self, id: f64) {
        self.server.end(id as u64);
    }
    #[napi]
    pub fn abort(&mut self, id: f64) {
        self.server.abort(id as u64);
    }
    #[napi]
    pub fn cancel_uploads(&self) {
        self.server.cancel_uploads();
    }
    #[napi]
    pub fn close(&mut self) -> napi::Result<()> {
        self.server.close().map_err(Error::from_reason)
    }
}
