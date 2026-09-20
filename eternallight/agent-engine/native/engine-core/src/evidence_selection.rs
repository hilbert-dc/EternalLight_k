use crate::{Result, canonical::array, evidence, proof};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn select(input: &Value) -> Result<Value> {
    let active = evidence::dispatch("evidence.active", input)?;
    let records: Vec<_> = array(&active)
        .iter()
        .filter(|r| {
            input["requiredScope"].is_null()
                || evidence::covers(&r["scope"], &input["requiredScope"])
        })
        .cloned()
        .collect();
    let summary = evidence::dispatch("evidence.summary", &json!(records))?;
    let mut decisions = Vec::new();
    let mut qualified = Vec::new();
    for record in summary
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::values)
    {
        let capability = record["capability"]
            .as_str()
            .ok_or("invalid evidence capability")?;
        let expectations = input["expectedDependencies"]
            .get(capability)
            .unwrap_or(&input["expectedDependencies"]);
        let mut decision = proof::dispatch(
            "proof.evaluate",
            &json!({"record":record,"expectedDependencies":expectations}),
        )?;
        if decision["status"] == "qualified" {
            qualified.push(record.clone());
        }
        decision["evidenceRef"] = record["evidenceRef"].clone();
        decision["capability"] = capability.into();
        decisions.push(decision);
    }
    Ok(
        json!({"records":records,"summary":evidence::dispatch("evidence.summary",&json!(qualified))?,"decisions":decisions}),
    )
}

pub(crate) fn credential_failure(input: &Value) -> Result<Value> {
    let cutoff = input["now"].as_i64().ok_or("evidence clock is invalid")?;
    let records = array(&input["records"])
        .iter()
        .filter(|r| {
            r["credentialRealm"] == input["credentialRealm"]
                && evidence::timestamp(&r["expiresAt"]).is_ok_and(|time| time > cutoff)
        })
        .cloned()
        .collect();
    let effective = evidence::dispatch("evidence.effective", &Value::Array(records))?;
    Ok(array(&effective)
        .iter()
        .any(|r| r["status"] == "failed" && r["sample"]["category"] == "authentication")
        .into())
}

pub(crate) fn prune(enabled: &mut BTreeSet<String>, dependencies: &Value) -> Value {
    let mut removed = serde_json::Map::new();
    loop {
        let mut changed = false;
        for capability in enabled.clone() {
            let missing: Vec<_> = array(&dependencies[&capability])
                .iter()
                .filter(|d| d.as_str().is_none_or(|d| !enabled.contains(d)))
                .cloned()
                .collect();
            if !missing.is_empty() {
                enabled.remove(&capability);
                removed.insert(capability, missing.into());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    removed.into()
}
