use eternallight_engine_core::ExecutionAuthority;
use napi_derive::napi;

#[napi]
#[derive(Default)]
pub struct NativeExecutionAuthority {
    authority: ExecutionAuthority,
}

#[napi]
impl NativeExecutionAuthority {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[napi]
    pub fn apply(&mut self, method: String, input: String) -> napi::Result<String> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("authority input exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid authority input"))?;
        let result = self
            .authority
            .apply(&method, input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result)
            .map_err(|_| napi::Error::from_reason("invalid authority snapshot"))
    }
}

#[napi]
pub struct NativeRouteLineage {
    lineage: eternallight_engine_core::RouteLineage,
}

#[napi]
impl NativeRouteLineage {
    #[napi(constructor)]
    pub fn new(lease: String) -> napi::Result<Self> {
        if lease.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("lease exceeds limit"));
        }
        let lease =
            serde_json::from_str(&lease).map_err(|_| napi::Error::from_reason("invalid lease"))?;
        Ok(Self {
            lineage: eternallight_engine_core::RouteLineage::new(lease),
        })
    }

    #[napi]
    pub fn authorize(&mut self, input: String) -> napi::Result<String> {
        if input.len() > 1024 * 1024 {
            return Err(napi::Error::from_reason("lineage input exceeds limit"));
        }
        let input = serde_json::from_str(&input)
            .map_err(|_| napi::Error::from_reason("invalid lineage input"))?;
        let result = self
            .lineage
            .authorize(&input)
            .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&result).map_err(|_| napi::Error::from_reason("invalid route"))
    }
}
