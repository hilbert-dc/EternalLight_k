use eternallight_engine_core::ExecutionTrace;
use napi_derive::napi;

#[napi]
pub struct NativeExecutionTrace {
    trace: ExecutionTrace,
}

#[napi]
impl NativeExecutionTrace {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("trace options exceed limit"));
        }
        let options = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid trace options"))?;
        Ok(Self {
            trace: ExecutionTrace::new(&options).map_err(napi::Error::from_reason)?,
        })
    }

    #[napi]
    pub fn record(&mut self, input: String) -> napi::Result<String> {
        if input.len() > 4 * 1024 * 1024 {
            return Err(napi::Error::from_reason("trace event exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid trace event"))?;
        let value = self
            .trace
            .record(&input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&value).map_err(|_| napi::Error::from_reason("invalid trace result"))
    }

    #[napi]
    pub fn snapshot(&self) -> napi::Result<String> {
        serde_json::to_string(&self.trace.snapshot())
            .map_err(|_| napi::Error::from_reason("invalid trace snapshot"))
    }
}
