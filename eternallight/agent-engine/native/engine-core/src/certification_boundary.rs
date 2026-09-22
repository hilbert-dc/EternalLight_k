use crate::{
    Result,
    canonical::{array, string},
    certification, contracts,
    digest::digest,
    session_identity,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn complete(input: &Value) -> Result<Value> {
    let mut body = certification::define(
        &json!({"value":input,"computedReportDigest":digest(&input["report"])}),
    )?;
    let id = digest(&body);
    if let Some(expected) = input.get("certificationId")
        && expected != &id
    {
        return Err("model certification digest does not match its contents".into());
    }
    body["certificationId"] = id.into();
    Ok(body)
}
pub(crate) fn document(input: &Value) -> Result<Value> {
    if input["version"] != 2 || !input["certifications"].is_array() {
        return Err("unsupported model certification document".into());
    }
    if array(&input["certifications"]).len() > 10_000 {
        return Err("model certification document exceeds limit".into());
    }
    let certifications = array(&input["certifications"])
        .iter()
        .map(complete)
        .collect::<Result<Vec<_>>>()?;
    let ids: BTreeSet<_> = certifications
        .iter()
        .map(|v| string(&v["certificationId"]))
        .collect();
    if ids.len() != certifications.len() {
        return Err("model certification ids must be unique".into());
    }
    Ok(json!({"version":2,"certifications":certifications}))
}
pub(crate) fn evidence(input: &Value) -> Result<Value> {
    let cert = complete(&input["certification"])?;
    let connection = &input["connection"];
    let context = &input["qualificationContextDigest"];
    if !array(&contracts::definitions()["PROTOCOLS"]).contains(&connection["protocol"]) {
        return Err("no codec for certification protocol".into());
    }
    let contract = input
        .get("engineContract")
        .unwrap_or(&contracts::definitions()["ENGINE_CONTRACT"]);
    let realm =
        session_identity::dispatch("connection.credentialRealm", &input["credentialScope"])?;
    let environment = session_identity::dispatch(
        "connection.environmentDigest",
        &json!({"connection":connection,"options":{"codecVersion":cert["codecVersion"],"credentialScope":input["credentialScope"],"engineContractHash":cert["engineContractHash"],"protocol":cert["protocol"],"qualificationContextDigest":context}}),
    )?;
    certification::materialize(
        &json!({"certification":cert,"connection":connection,"now":input["now"],"qualificationContextDigest":context,"connectionDigest":session_identity::definition(connection),"codecVersion":"3","contractDigest":digest(contract),"credentialRealm":realm,"environmentDigest":environment,"modelProtocolDigest":digest(&json!({"connectionDigest":cert["connectionDigest"],"protocol":cert["protocol"],"codecVersion":cert["codecVersion"]})),"engineBuildDigest":digest(&json!({"engineContractHash":cert["engineContractHash"],"qualificationContextDigest":context})),"routeDigest":digest(&cert["routes"])}),
    )
}
