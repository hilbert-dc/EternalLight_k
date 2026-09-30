use eternallight_engine_core::PersistentBudget;
use napi_derive::napi;
use std::path::PathBuf;

#[napi]
pub struct NativePersistentBudget {
    budget: PersistentBudget,
}

#[napi]
impl NativePersistentBudget {
    #[napi]
    pub fn receipt(path: String, run_id: String) -> napi::Result<String> {
        let result = PersistentBudget::receipt(&PathBuf::from(path), &run_id)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result)
            .map_err(|_| napi::Error::from_reason("invalid budget receipt"))
    }

    #[napi(constructor)]
    pub fn new(path: String, run_id: String, input: String) -> napi::Result<Self> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("budget input exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid budget parameters"))?;
        Ok(Self {
            budget: PersistentBudget::new(&PathBuf::from(path), &run_id, input)
                .map_err(napi::Error::from_reason)?,
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

    #[napi]
    pub fn complete(&mut self, status: String) -> napi::Result<()> {
        self.budget
            .complete(&status)
            .map_err(napi::Error::from_reason)
    }

    #[napi]
    pub fn close(&mut self) {
        self.budget.close();
    }

    #[napi]
    pub fn recover(path: String) -> napi::Result<String> {
        let result =
            PersistentBudget::recover(&PathBuf::from(path)).map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result)
            .map_err(|_| napi::Error::from_reason("invalid recovery result"))
    }
}
