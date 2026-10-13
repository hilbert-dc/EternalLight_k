use eternallight_engine_core::BehaviorRun;
use napi_derive::napi;

#[napi]
pub struct NativeBehaviorRun {
    run: BehaviorRun,
}

fn parse(input: &str) -> napi::Result<serde_json::Value> {
    if input.len() > 4 * 1024 * 1024 {
        return Err(napi::Error::from_reason("behavior input exceeds limit"));
    }
    serde_json::from_str(input).map_err(|_| napi::Error::from_reason("invalid behavior input"))
}

#[napi]
impl NativeBehaviorRun {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        Ok(Self {
            run: BehaviorRun::new(&parse(&input)?).map_err(napi::Error::from_reason)?,
        })
    }

    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        let value = self
            .run
            .apply(&method, &parse(&input)?)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&value)
            .map_err(|_| napi::Error::from_reason("invalid behavior result"))
    }
}
