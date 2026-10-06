use crate::{
    Result,
    canonical::{array, string},
    contracts, evidence, evidence_selection, extensions,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn scope(protocol: &str, capability: &str) -> Value {
    let engine = contracts::definitions()["ENGINE_CAPABILITIES"]
        .as_object()
        .unwrap()
        .values()
        .any(|v| v == capability);
    json!({"protocol":protocol,"mode":if capability == "streaming" { "native-sse" } else if engine { "app-server-mcp" } else { "buffered" },"toolTypes":if capability == "structuredToolCalls" || engine { vec!["function"] } else { vec![] }})
}

fn select(input: &Value, context: &Value, required_scope: Value) -> Result<Value> {
    evidence_selection::select(
        &json!({"records":input["records"],"now":input["now"],"environmentDigest":context["environmentDigest"],"expectedDependencies":context["expectedDependencies"],"requiredScope":required_scope}),
    )
}

fn required(input: &Value) -> Result<Vec<String>> {
    let profile = contracts::profile(
        input["policy"]["capabilityProfileId"]
            .as_str()
            .unwrap_or("base-tools-v1"),
    )?;
    let mut result = Vec::new();
    for value in array(&profile["requiredCapabilities"])
        .iter()
        .chain(array(&input["engineContract"]["requiredCapabilities"]))
        .chain(array(&input["policy"]["requiredCapabilities"]))
    {
        let name = value
            .as_str()
            .ok_or("invalid required capability")?
            .to_owned();
        contracts::domain(&name)?;
        if !result.contains(&name) {
            result.push(name);
        }
    }
    Ok(result)
}

fn revoked(input: &Value, context: &Value) -> Result<bool> {
    Ok(evidence_selection::credential_failure(
        &json!({"records":input["records"],"credentialRealm":context["credentialRealm"],"now":input["now"]}),
    )? == true)
}

pub(crate) fn protocols(input: &Value) -> Result<Value> {
    let required = array(&input["requiredCapabilities"]);
    let mut result = json!({});
    for connection in array(&input["connections"])
        .iter()
        .filter(|c| c["protocol"] == "auto")
    {
        let id = string(&connection["id"]);
        for protocol in array(&contracts::definitions()["PROTOCOLS"]) {
            let protocol = string(protocol);
            let context = &input["contexts"][id][protocol];
            if revoked(input, context)? {
                break;
            }
            let mut qualified = true;
            for capability in required {
                let selection = select(input, context, scope(protocol, string(capability)))?;
                if selection["summary"].get(string(capability)).is_none() {
                    qualified = false;
                    break;
                }
            }
            if qualified {
                result[id] = protocol.into();
                break;
            }
        }
    }
    Ok(result)
}

pub(crate) fn compile(input: &Value) -> Result<Value> {
    let connections = array(&input["connections"]);
    if connections.is_empty() {
        return Err("at least one connection is required".into());
    }
    let by_id: BTreeMap<_, _> = connections.iter().map(|c| (string(&c["id"]), c)).collect();
    if by_id.len() != connections.len() {
        return Err("connection ids must be unique".into());
    }
    let policy = &input["policy"];
    let root = policy["defaultConnectionId"]
        .as_str()
        .unwrap_or(string(&connections[0]["id"]));
    if !by_id.contains_key(root) {
        return Err("default connection does not exist".into());
    }
    let children = policy
        .get("allowedChildConnectionIds")
        .cloned()
        .unwrap_or_else(|| json!([root]));
    let edges = policy.get("allowedChildEdges").cloned().unwrap_or_else(|| {
        let parents: BTreeSet<_> = std::iter::once(root)
            .chain(array(&children).iter().map(string))
            .collect();
        parents
            .into_iter()
            .flat_map(|parent| {
                array(&children).iter().map(
                    move |child| json!({"parentConnectionId":parent,"childConnectionId":child}),
                )
            })
            .collect()
    });
    let mut active: BTreeSet<_> = std::iter::once(root)
        .chain(array(&children).iter().map(string))
        .collect();
    for edge in array(&edges) {
        active.insert(string(&edge["parentConnectionId"]));
        active.insert(string(&edge["childConnectionId"]));
    }
    for id in &active {
        if !by_id.contains_key(id) {
            return Err(format!("unknown routed connection: {id}"));
        }
    }
    let required = required(input)?;
    let mut protocol_input = input.clone();
    protocol_input["requiredCapabilities"] = json!(required);
    let selections = protocols(&protocol_input)?;
    let mut routes = Vec::new();
    let mut adopted = json!({});
    let mut route_contexts = json!({});
    for connection in connections
        .iter()
        .filter(|c| active.contains(string(&c["id"])))
    {
        let id = string(&connection["id"]);
        let protocol = if connection["protocol"] == "auto" {
            policy["protocolSelections"]
                .get(id)
                .unwrap_or(&selections[id])
        } else {
            &connection["protocol"]
        };
        if protocol.is_null() || protocol == "auto" {
            return Err(format!(
                "connection {id} has no verified protocol selection"
            ));
        }
        let protocol = string(protocol);
        let context = &input["contexts"][id][protocol];
        if context.is_null() {
            return Err(format!("unsupported protocol: {protocol}"));
        }
        if revoked(input, context)? {
            return Err(format!(
                "connection {id} credential realm is revoked by authentication evidence"
            ));
        }
        let mut summary = BTreeMap::new();
        for capability in contracts::definitions()["CAPABILITIES"]
            .as_object()
            .unwrap()
            .values()
            .map(string)
        {
            let selection = select(input, context, scope(protocol, capability))?;
            if let Some(record) = selection["summary"].get(capability) {
                summary.insert(capability.to_owned(), record.clone());
            }
        }
        let mut enabled: BTreeSet<String> = summary
            .keys()
            .filter(|c| {
                !matches!(
                    connection["capabilitySupport"][c.as_str()].as_str(),
                    Some("unsupported" | "unknown")
                )
            })
            .cloned()
            .collect();
        let unavailable = evidence_selection::prune(
            &mut enabled,
            &contracts::definitions()["CAPABILITY_DEPENDENCIES"],
        );
        let missing: Vec<_> = required
            .iter()
            .filter(|c| !enabled.contains(*c))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "connection {id} lacks verified capabilities: {}",
                missing.join(", ")
            ));
        }
        let context_tokens = connection["declaredLimits"]["contextTokens"]
            .as_u64()
            .filter(|n| *n > 0);
        let output_tokens = connection["declaredLimits"]["outputTokens"]
            .as_u64()
            .filter(|n| *n > 0);
        let (Some(context_tokens), Some(output_tokens)) = (context_tokens, output_tokens) else {
            return Err(format!("connection {id} requires verified token limits"));
        };
        let reserved = policy
            .get("reservedInputTokens")
            .unwrap_or(&json!(16384))
            .as_u64()
            .ok_or("invalid reserved input tokens")?;
        let Some(input_budget) = context_tokens
            .checked_sub(output_tokens)
            .and_then(|n| n.checked_sub(reserved))
            .filter(|n| *n > 0)
        else {
            return Err(format!("connection {id} has no usable input budget"));
        };
        let mut route = json!({"logicalModel":id,"connectionId":id,"provider":connection["provider"],"model":connection["model"],"endpoint":connection["endpoint"],"credentialRef":connection["credentialRef"],"protocol":protocol,"transport":if protocol == "responses" { "native-passthrough" } else { "protocol-conversion" },"codecId":protocol,"environmentDigest":context["environmentDigest"],"enabledExtensions":enabled,"unavailableDependencies":unavailable,"inputBudget":input_budget,"outputBudget":output_tokens});
        route["authorizedExtensions"] = extensions::authorize(
            &route,
            &policy["productExtensions"],
            &policy["requiredExtensions"],
        )?;
        routes.push(route);
        adopted[id] = enabled
            .iter()
            .filter_map(|c| summary.get(c).cloned())
            .collect();
        route_contexts[id] = context.clone();
    }
    Ok(
        json!({"routes":routes,"adopted":adopted,"contexts":route_contexts,"requestedEdges":edges,"root":root}),
    )
}

pub(crate) fn finalize(input: &Value) -> Result<Value> {
    let draft = &input["draft"];
    let mut adopted = draft["adopted"].clone();
    let mut edges = Vec::new();
    for edge_input in array(&input["edges"]) {
        let edge = &edge_input["edge"];
        let parent = string(&edge["parentConnectionId"]);
        let mut context = draft["contexts"][parent].clone();
        if context.is_null() {
            return Err("child route edge names an unknown connection".into());
        }
        context["expectedDependencies"]["route"] = edge_input["dependencies"].clone();
        let route = array(&draft["routes"])
            .iter()
            .find(|r| r["connectionId"] == parent)
            .ok_or("missing parent route")?;
        let required_scope = json!({"protocol":route["protocol"],"mode":"app-server-mcp","toolTypes":["function"],"routes":[edge]});
        let selection = select(input, &context, required_scope)?;
        if let Some(record) = selection["summary"].get("childDirectedRoute") {
            edges.push(edge.clone());
            let records = adopted[parent]
                .as_array_mut()
                .ok_or("missing parent evidence")?;
            if !records.contains(record) {
                records.push(record.clone());
            }
        }
    }
    let root = array(&draft["routes"])
        .iter()
        .find(|r| r["connectionId"] == draft["root"])
        .ok_or("session plan has no default route")?;
    let profiles: BTreeSet<_> = contracts::definitions()["CAPABILITY_PROFILES"]
        .as_object()
        .unwrap()
        .values()
        .filter(|p| {
            array(&p["requiredCapabilities"])
                .iter()
                .all(|c| array(&root["enabledExtensions"]).contains(c))
        })
        .map(|p| string(&p["id"]))
        .collect();
    let records: Vec<Value> = array(&draft["routes"])
        .iter()
        .flat_map(|route| array(&adopted[string(&route["connectionId"])]))
        .cloned()
        .collect();
    let mut dependencies = json!({});
    for kind in ["modelProtocol", "engineBuild", "route", "access", "report"] {
        let values: BTreeSet<_> = records
            .iter()
            .filter_map(|r| r["dependencies"][kind]["digest"].as_str())
            .collect();
        if !values.is_empty() {
            dependencies[kind] = json!(values);
        }
    }
    let expiry = records
        .iter()
        .map(|r| evidence::timestamp(&r["expiresAt"]))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .min()
        .ok_or("session plan requires expiring evidence")?;
    let expiry = chrono::DateTime::from_timestamp_millis(expiry)
        .ok_or("invalid session expiry")?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let policy = &input["policy"];
    let sorted = |field: &str| -> Value {
        json!(
            array(&policy[field])
                .iter()
                .map(string)
                .collect::<BTreeSet<_>>()
        )
    };
    let plan = json!({"engineContractHash":input["contractDigest"],"evidenceRefs":records.iter().map(|r|string(&r["evidenceRef"])).collect::<BTreeSet<_>>(),"dependencyDigests":dependencies,"policyDigest":input["policyDigest"],"routes":draft["routes"],"defaultModel":draft["root"],"rootRoute":draft["root"],"capabilityProfileId":policy["capabilityProfileId"].as_str().unwrap_or("base-tools-v1"),"authorizedCapabilityProfiles":profiles,"productExtensions":sorted("productExtensions"),"requiredExtensions":sorted("requiredExtensions"),"allowedChildModels":edges.iter().map(|e|string(&e["childConnectionId"])).collect::<BTreeSet<_>>(),"allowedChildEdges":edges,"allowedMcpServerIds":sorted("allowedMcpServerIds"),"expiresAt":expiry,"budgetPolicyRef":policy["budgetPolicyRef"].as_str().unwrap_or("default"),"permissionsRef":policy["permissionsRef"].as_str().unwrap_or("default")});
    let mut plan = plan;
    let max_children = match policy.get("maxActiveChildren") {
        None => 16,
        Some(value) => value.as_u64().filter(|v| *v > 0 && *v <= 64)
            .ok_or("maxActiveChildren must be an integer from 1 to 64")?,
    };
    plan["maxActiveChildren"] = max_children.into();
    plan["resourcePolicy"] = crate::request_scheduler::policy(&policy["resourcePolicy"])?;
    Ok(json!({"plan":plan,"adoptedRecords":records}))
}
