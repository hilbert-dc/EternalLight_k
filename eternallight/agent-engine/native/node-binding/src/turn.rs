use eternallight_engine_core::AppServerTurn;
use napi_derive::napi;

#[napi]
#[derive(Default)]
pub struct NativeAppServerTurn {
    turn: AppServerTurn,
}

#[napi]
impl NativeAppServerTurn {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    #[napi]
    pub fn bind_thread(&mut self, generation: u32, thread: String) -> napi::Result<()> {
        self.turn
            .bind_thread(generation.into(), thread)
            .map_err(napi::Error::from_reason)
    }
    #[napi]
    pub fn bind_started(&mut self, turn: Option<String>) -> napi::Result<bool> {
        self.turn
            .bind_started(turn)
            .map_err(napi::Error::from_reason)
    }
    #[napi]
    pub fn cancel(&mut self) {
        self.turn.cancel();
    }
    #[napi]
    pub fn finish(&mut self) {
        self.turn.finish();
    }
    #[napi]
    pub fn inspect(&self) -> napi::Result<String> {
        serde_json::to_string(&self.turn.snapshot())
            .map_err(|_| napi::Error::from_reason("invalid turn state"))
    }
    #[napi]
    pub fn observe(&mut self, generation: u32, message: String) -> napi::Result<String> {
        if message.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("turn event exceeds limit"));
        }
        let value = serde_json::from_str(&message)
            .map_err(|_| napi::Error::from_reason("invalid turn event"))?;
        let output = self
            .turn
            .observe(generation.into(), &value)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&output)
            .map_err(|_| napi::Error::from_reason("invalid turn observation"))
    }
}
