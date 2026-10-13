use crate::Result;
use serde_json::Value;
use std::sync::OnceLock;

pub(crate) fn definitions() -> &'static Value {
    static DEFINITIONS: OnceLock<Value> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        serde_json::from_str(include_str!("contracts.json"))
            .expect("compiled engine contracts must be valid JSON")
    })
}

pub(crate) fn domain(capability: &str) -> Result<&'static str> {
    for (key, domain) in [
        ("MODEL_CAPABILITIES", "model"),
        ("ENGINE_CAPABILITIES", "engine"),
    ] {
        if definitions()[key]
            .as_object()
            .unwrap()
            .values()
            .any(|value| value == capability)
        {
            return Ok(domain);
        }
    }
    Err(format!("unknown capability: {capability}"))
}

pub(crate) fn profile(id: &str) -> Result<Value> {
    definitions()["CAPABILITY_PROFILES"]
        .get(id)
        .cloned()
        .ok_or_else(|| format!("unknown capability profile: {id}"))
}
