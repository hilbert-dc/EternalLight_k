use crate::{
    Result,
    canonical::{array, string},
    contracts,
    digest::digest,
    session,
};
use serde_json::{Value, json};

fn subset(value: &Value, fields: &[&str]) -> Value {
    fields
        .iter()
        .filter_map(|key| value.get(*key).map(|value| ((*key).into(), value.clone())))
        .collect::<serde_json::Map<_, _>>()
        .into()
}
pub(crate) fn definition(connection: &Value) -> String {
    digest(&subset(
        connection,
        &[
            "id",
            "provider",
            "endpoint",
            "model",
            "credentialRef",
            "protocol",
            "protocolOptions",
            "capabilitySupport",
            "declaredLimits",
        ],
    ))
}
fn realm(value: &Value) -> Result<String> {
    if value.as_str().is_none_or(str::is_empty) {
        return Err("credential scope is required".into());
    }
    Ok(digest(&json!({"credentialScope":value})))
}
fn environment(connection: &Value, options: &Value) -> String {
    let mut value = subset(
        connection,
        &["endpoint", "model", "protocolOptions", "capabilitySupport"],
    );
    let map = value.as_object_mut().unwrap();
    map.insert(
        "codecVersion".into(),
        options.get("codecVersion").cloned().unwrap_or("1".into()),
    );
    map.insert(
        "credentialScope".into(),
        options
            .get("credentialScope")
            .cloned()
            .unwrap_or("default".into()),
    );
    if let Some(hash) = options.get("engineContractHash") {
        map.insert("engineContractHash".into(), hash.clone());
    }
    map.insert(
        "protocol".into(),
        options
            .get("protocol")
            .filter(|v| !v.is_null())
            .unwrap_or(&connection["protocol"])
            .clone(),
    );
    map.insert(
        "qualificationContextDigest".into(),
        options
            .get("qualificationContextDigest")
            .cloned()
            .unwrap_or("default".into()),
    );
    digest(&value)
}
fn directed(input: &Value) -> Result<String> {
    let parent = &input["parentConnection"];
    let child = &input["childConnection"];
    let parent_scope = input
        .get("parentCredentialScope")
        .filter(|v| !v.is_null())
        .unwrap_or(&parent["credentialRef"]);
    let child_scope = input
        .get("childCredentialScope")
        .filter(|v| !v.is_null())
        .unwrap_or(&child["credentialRef"]);
    Ok(digest(
        &json!({"edge":{"parentConnectionId":parent["id"],"childConnectionId":child["id"]},"parentProtocol":input["parentProtocol"],"childProtocol":input["childProtocol"],"childConnectionDigest":definition(child),"childCodecVersion":"3","parentCredentialRealm":realm(parent_scope)?,"childCredentialRealm":realm(child_scope)?}),
    ))
}
fn contexts(input: &Value, contract_hash: &str) -> Result<Value> {
    let mut contexts = json!({});
    let policy = &input["policy"];
    for connection in array(&input["connections"]) {
        let id = connection["id"].as_str().ok_or("invalid connection id")?;
        let credential_scope = policy["credentialScopes"]
            .get(id)
            .filter(|v| !v.is_null())
            .unwrap_or(&connection["credentialRef"]);
        let realm = realm(credential_scope)?;
        let qualification = policy
            .get("qualificationContextDigest")
            .cloned()
            .unwrap_or("default".into());
        let mut protocols = json!({});
        for protocol in array(&contracts::definitions()["PROTOCOLS"]) {
            let protocol = protocol.as_str().ok_or("invalid compiled protocol")?;
            protocols[protocol] = json!({"credentialRealm":realm,"environmentDigest":environment(connection,&json!({"codecVersion":"3","credentialScope":credential_scope,"engineContractHash":contract_hash,"protocol":protocol,"qualificationContextDigest":qualification})),"expectedDependencies":{"modelProtocol":digest(&json!({"connectionDigest":definition(connection),"protocol":protocol,"codecVersion":"3"})),"engineBuild":digest(&json!({"engineContractHash":contract_hash,"qualificationContextDigest":qualification})),"access":realm}});
        }
        contexts[id] = protocols;
    }
    Ok(contexts)
}
pub(crate) enum Compilation {
    Plan,
    Protocols,
}
pub(crate) fn compile(input: &Value, kind: Compilation) -> Result<Value> {
    let mut input = input.clone();
    if input["connections"].as_array().is_none_or(Vec::is_empty) {
        return Err("at least one connection is required".into());
    }
    if input["engineContract"].is_null() {
        input["engineContract"] = contracts::definitions()["ENGINE_CONTRACT"].clone();
    }
    let contract_digest = digest(&input["engineContract"]);
    input["contexts"] = contexts(&input, &contract_digest)?;
    if matches!(kind, Compilation::Protocols) {
        if input["requiredCapabilities"].is_null() {
            input["requiredCapabilities"] = input["engineContract"]["requiredCapabilities"].clone();
        }
        return session::protocols(&input);
    }
    let draft = session::compile(&input)?;
    let mut edges = Vec::new();
    for edge in array(&draft["requestedEdges"]) {
        let parent = array(&input["connections"])
            .iter()
            .find(|c| c["id"] == edge["parentConnectionId"])
            .ok_or("unknown parent route")?;
        let child = array(&input["connections"])
            .iter()
            .find(|c| c["id"] == edge["childConnectionId"])
            .ok_or("unknown child route")?;
        let parent_route = array(&draft["routes"])
            .iter()
            .find(|r| r["connectionId"] == parent["id"])
            .ok_or("missing parent route")?;
        let child_route = array(&draft["routes"])
            .iter()
            .find(|r| r["connectionId"] == child["id"])
            .ok_or("missing child route")?;
        let options = json!({"parentConnection":parent,"childConnection":child,"parentProtocol":parent_route["protocol"],"childProtocol":child_route["protocol"],"parentCredentialScope":input["policy"]["credentialScopes"][string(&parent["id"])],"childCredentialScope":input["policy"]["credentialScopes"][string(&child["id"])]});
        edges
            .push(json!({"edge":edge,"dependencies":[digest(&json!([edge])),directed(&options)?]}));
    }
    input["draft"] = draft;
    input["edges"] = edges.into();
    input["contractDigest"] = contract_digest.into();
    input["policyDigest"] = digest(&input["policy"]).into();
    let finalized = session::finalize(&input)?;
    let mut records: Vec<_> = array(&finalized["adoptedRecords"])
        .iter()
        .map(|record| {
            subset(
                record,
                &[
                    "capability",
                    "status",
                    "scope",
                    "source",
                    "evidenceRef",
                    "reportRef",
                    "reportDigest",
                    "checkedAt",
                    "expiresAt",
                    "dependencies",
                ],
            )
        })
        .collect();
    records.sort_by(|left, right| string(&left["evidenceRef"]).cmp(string(&right["evidenceRef"])));
    let mut plan = finalized["plan"].clone();
    plan["evidenceDigest"] = digest(&json!(records)).into();
    plan["id"] = digest(&plan).into();
    Ok(plan)
}
pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "connection.definitionDigest" => Ok(definition(input).into()),
        "connection.environmentDigest" => {
            Ok(environment(&input["connection"], &input["options"]).into())
        }
        "connection.credentialRealm" => Ok(realm(input)?.into()),
        "connection.directedDigest" => Ok(directed(input)?.into()),
        _ => Err("unknown connection identity operation".into()),
    }
}
