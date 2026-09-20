use crate::{
    Result,
    canonical::{array, string},
    contracts,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn registry() -> Value {
    let mut result = json!({});
    let definitions = contracts::definitions();
    for capability in definitions["ENGINE_CAPABILITIES"]
        .as_object()
        .unwrap()
        .values()
        .filter(|c| *c != "toolLoop")
    {
        result[string(capability)] = json!({"executable":true,"configKey":"multi_agent","requires":array(&definitions["CAPABILITY_DEPENDENCIES"][string(capability)])});
    }
    result["toolSearch"] =
        json!({"executable":true,"configKey":"tool_search","requires":["structuredToolCalls"]});
    result["parallelToolCalls"] = json!({"executable":true,"catalogKey":"supports_parallel_tool_calls","requires":["structuredToolCalls"]});
    result["imageInput"] =
        json!({"executable":"responses-only","catalogKey":"input_modalities","requires":[]});
    result["reasoningSummary"] =
        json!({"executable":true,"catalogKey":"supports_reasoning_summaries","requires":[]});
    result["remoteCompaction"] = json!({"executable":false,"requires":[]});
    result["customTools"] = result["remoteCompaction"].clone();
    result
}

fn executable(definition: &Value, route: &Value) -> bool {
    definition["executable"] == true
        || (definition["executable"] == "responses-only" && route["protocol"] == "responses")
}

pub(crate) fn authorize(route: &Value, product: &Value, required: &Value) -> Result<Value> {
    let registry = registry();
    let authorized: BTreeSet<_> = array(product).iter().map(string).collect();
    let available: BTreeSet<_> = array(&route["enabledExtensions"])
        .iter()
        .map(string)
        .collect();
    for capability in &authorized {
        if registry.get(*capability).is_none() {
            return Err(format!("unknown engine extension: {capability}"));
        }
    }
    for capability in array(required).iter().map(string) {
        if !authorized.contains(capability) {
            return Err(format!(
                "required extension is not authorized by the product: {capability}"
            ));
        }
        if !available.contains(capability) {
            return Err(format!(
                "required extension is not verified for the selected route: {capability}"
            ));
        }
        if !executable(&registry[capability], route) {
            return Err(format!(
                "required extension has no executable compiler path: {capability}"
            ));
        }
    }
    let enabled: Vec<_> = authorized
        .into_iter()
        .filter(|c| available.contains(c) && executable(&registry[*c], route))
        .collect();
    for capability in &enabled {
        for dependency in array(&registry[*capability]["requires"]).iter().map(string) {
            if !available.contains(dependency) {
                return Err(format!("{capability} requires verified {dependency}"));
            }
        }
    }
    Ok(json!(enabled))
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "extensions.registry" => Ok(registry()),
        "extensions.executable" => {
            Ok(executable(&registry()[string(&input["capability"])], &input["route"]).into())
        }
        "extensions.route" => authorize(&input["route"], &input["product"], &input["required"]),
        "extensions.plan" => {
            let plan = &input["plan"];
            let route = array(&plan["routes"])
                .iter()
                .find(|r| r["connectionId"] == plan["defaultModel"])
                .ok_or("session plan has no default route")?;
            authorize(route, &input["product"], &input["required"])
        }
        _ => Err("unknown extension operation".into()),
    }
}
