//! Fixture builder: split an expected Runtime request envelope into trusted
//! process context and current model arguments. No production fallback exists.
use super::RuntimeClient;
use serde_json::Value;
pub(super) fn rpc(message: &Value, client: &Result<RuntimeClient, String>) -> Option<Value> {
    let mut wire = message.clone();
    if wire["method"] == "tools/call" {
        if let Some(meta) = wire["params"]["_meta"].as_object_mut() {
            meta.entry("samplingRequestId").or_insert_with(|| serde_json::json!(format!("fixture-{}", uuid_for_fixture())));
        }
        if let Some(client) = client.as_ref().ok() {
            let args = &mut wire["params"]["arguments"];
            let mut binding = serde_json::json!({"operation":client.operation});
            for field in super::tool_binding::FIELDS {
                if let Some(value) = args.as_object_mut().and_then(|o| o.remove(field)) {
                    binding[field] = value;
                }
            }
            if binding["projectId"].is_string() {
                *client.tool_binding.borrow_mut() = Some(binding);
            }
        }
    }
    super::handle_message(&wire, client)
}

// Expected wire contract, independent of the production error constructor.
pub(super) fn binding_error(reason: &str, field: &str, message: &str) -> String {
    serde_json::json!({"code":"tool_binding_rejected","origin":"novelMcp","phase":"binding","error":{
        "code":reason,"message":message,"details":{"origin":"novelMcp","phase":"binding","reason":reason,
        "instancePath":field,"nextAction":"verify_execution_binding"}}}).to_string()
}

fn uuid_for_fixture() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
