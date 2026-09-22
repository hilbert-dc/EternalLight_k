use eternallight_engine_core::TaskCancellation;
use napi_derive::napi;
#[napi]
pub struct NativeTaskCancellation {
    state: TaskCancellation,
}
#[napi]
impl NativeTaskCancellation {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            state: TaskCancellation::default(),
        }
    }
    #[napi]
    pub fn cancel(&mut self) {
        self.state.cancel();
    }
    #[napi]
    pub fn cancelled(&self) -> bool {
        self.state.cancelled()
    }
    #[napi]
    pub fn assert_completable(&self) -> napi::Result<()> {
        self.state
            .assert_completable()
            .map_err(napi::Error::from_reason)
    }
}
