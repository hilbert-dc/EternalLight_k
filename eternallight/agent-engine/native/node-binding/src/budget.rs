use eternallight_engine_core::TaskBudget;
use napi_derive::napi;

#[napi]
pub struct NativeTaskBudget {
    budget: TaskBudget,
}

#[napi]
impl NativeTaskBudget {
    #[napi(constructor)]
    pub fn new(input: String) -> napi::Result<Self> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("budget input exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid budget parameters"))?;
        Ok(Self {
            budget: TaskBudget::new(input).map_err(napi::Error::from_reason)?,
        })
    }

    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("budget input exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid budget parameters"))?;
        let value = self
            .budget
            .apply(&method, input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&value).map_err(|_| napi::Error::from_reason("invalid budget result"))
    }

    #[napi]
    pub fn inspect(&self) -> napi::Result<String> {
        serde_json::to_string(&self.budget.inspect())
            .map_err(|_| napi::Error::from_reason("invalid budget snapshot"))
    }
}
