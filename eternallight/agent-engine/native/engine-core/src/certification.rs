use crate::{
    Result, behavior,
    canonical::{array, string},
    contracts, evidence, proof,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn engine(capability: &str) -> bool {
    contracts::definitions()["ENGINE_CAPABILITIES"]
        .as_object()
        .unwrap()
        .values()
        .any(|v| v == capability)
}
fn delegation(capability: &str) -> bool {
    array(&contracts::definitions()["DELEGATION_EXECUTION_CAPABILITIES"])
        .iter()
        .any(|v| v == capability)
}

fn suite_for(capability: &str) -> &'static str {
    if capability == "toolLoop" {
        "codex-engine-core-integration-v1"
    } else if delegation(capability) {
        "codex-engine-delegation-execution-v1"
    } else if engine(capability) {
        "codex-engine-delegation-lifecycle-v1"
    } else {
        "codex-engine-model-certification-v1"
    }
}

pub(crate) fn define(input: &Value) -> Result<Value> {
    let value = &input["value"];
    if !value.is_object() {
        return Err("model certification must be an object".into());
    }
    for field in [
        "connectionId",
        "connectionDigest",
        "protocol",
        "codecVersion",
        "engineContractHash",
        "qualificationContextDigest",
        "profileId",
        "checkedAt",
        "expiresAt",
        "reportDigest",
        "outcome",
    ] {
        if value[field].as_str().is_none_or(str::is_empty) {
            return Err(format!("model certification {field} is required"));
        }
    }
    for field in [
        "connectionDigest",
        "engineContractHash",
        "qualificationContextDigest",
        "reportDigest",
    ] {
        let hash = string(&value[field]);
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(format!("{field} must be a SHA-256 digest"));
        }
    }
    if !value["report"].is_object() || value["reportDigest"] != input["computedReportDigest"] {
        return Err("model certification report digest does not match its contents".into());
    }
    if !array(&contracts::definitions()["PROTOCOLS"]).contains(&value["protocol"]) {
        return Err("unsupported model certification protocol".into());
    }
    let checked = evidence::timestamp(&value["checkedAt"])
        .map_err(|_| "model certification validity window is invalid")?;
    let expires = evidence::timestamp(&value["expiresAt"])
        .map_err(|_| "model certification validity window is invalid")?;
    if expires <= checked {
        return Err("model certification validity window is invalid".into());
    }
    if value["outcome"] != "passed" {
        return Err("only passed model certifications may be published".into());
    }
    let capabilities = value["capabilities"]
        .as_array()
        .filter(|c| !c.is_empty())
        .ok_or("model certification capabilities are invalid")?;
    for capability in capabilities {
        contracts::domain(string(capability))
            .map_err(|_| "model certification capabilities are invalid")?;
    }
    let capabilities: BTreeSet<_> = capabilities.iter().map(string).collect();
    let profile = contracts::profile(string(&value["profileId"]))?;
    let missing: Vec<_> = array(&profile["requiredCapabilities"])
        .iter()
        .map(string)
        .filter(|c| !capabilities.contains(c))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "model certification does not satisfy profile: {}",
            missing.join(", ")
        ));
    }
    let suites = value["suiteResults"]
        .as_array()
        .filter(|s| !s.is_empty())
        .ok_or("model certification suite results are required")?
        .iter()
        .map(behavior::suite)
        .collect::<Result<Vec<_>>>()?;
    for kind in ["protocol", "core-integration"].into_iter().chain(
        capabilities
            .iter()
            .any(|c| delegation(c))
            .then_some("execution-conformance"),
    ) {
        if !suites
            .iter()
            .any(|s| s["kind"] == kind && s["status"] == "passed")
        {
            return Err(format!(
                "model certification requires a passed {kind} suite"
            ));
        }
    }
    for capability in capabilities
        .iter()
        .filter(|c| engine(c) && **c != "toolLoop")
    {
        let suite = suite_for(capability);
        if !suites
            .iter()
            .any(|s| s["suiteId"] == suite && s["status"] == "passed")
        {
            return Err(format!(
                "model certification requires a passed {suite} suite"
            ));
        }
    }
    let report = &value["report"];
    if report["outcome"] != "passed" {
        return Err("model certification report did not pass".into());
    }
    for field in [
        "connectionId",
        "protocol",
        "profileId",
        "codecVersion",
        "qualificationContextDigest",
    ] {
        if report[field] != value[field] {
            return Err(format!(
                "model certification report identity mismatch: {field}"
            ));
        }
    }
    let reported_suites = array(&report["suiteResults"])
        .iter()
        .map(behavior::suite)
        .collect::<Result<Vec<_>>>()?;
    if suites != reported_suites {
        return Err("model certification suites are not bound to the report".into());
    }
    for capability in &capabilities {
        if !array(&report["capabilities"])
            .iter()
            .any(|c| c == capability)
        {
            return Err(format!(
                "capability is absent from the report: {capability}"
            ));
        }
        if !engine(capability) {
            if !suites.iter().any(|s| {
                s["kind"] == "protocol"
                    && s["status"] == "passed"
                    && array(&s["metrics"]["capabilities"])
                        .iter()
                        .any(|c| c == capability)
            }) {
                return Err(format!("capability lacks protocol proof: {capability}"));
            }
        } else if *capability != "toolLoop" {
            let suite = suites
                .iter()
                .find(|s| s["suiteId"] == suite_for(capability) && s["status"] == "passed")
                .ok_or("capability execution suite is missing")?;
            let result = array(&suite["metrics"]["capabilityResults"])
                .iter()
                .find(|r| r["capability"] == *capability && r["status"] == "verified");
            let required = proof::dispatch(
                "proof.assertions",
                &json!({"capability":capability,"scope":{"suite":suite_for(capability)},"assertions":[]}),
            )?;
            if result.is_none_or(|r| {
                array(&required).is_empty()
                    || array(&required).iter().any(|id| {
                        !array(&r["assertions"])
                            .iter()
                            .any(|a| a["id"] == *id && a["result"] == "passed")
                    })
            }) {
                return Err(format!("capability lacks execution proof: {capability}"));
            }
        }
    }
    let routes = value.get("routes").cloned().unwrap_or_else(|| json!([]));
    if !routes.is_array()
        || array(&routes).iter().any(|r| {
            r["parentConnectionId"] != value["connectionId"]
                || r["childConnectionId"] != value["connectionId"]
        })
        || (capabilities.contains("childDirectedRoute") && array(&routes).len() != 1)
    {
        return Err("certification route is not bound to the tested connection".into());
    }
    let mut body = json!({});
    for field in [
        "connectionId",
        "connectionDigest",
        "protocol",
        "codecVersion",
        "engineContractHash",
        "qualificationContextDigest",
        "profileId",
        "checkedAt",
        "expiresAt",
        "report",
        "reportDigest",
        "outcome",
    ] {
        body[field] = value[field].clone();
    }
    body["capabilities"] = json!(capabilities);
    body["routes"] = routes;
    body["suiteResults"] = suites.into();
    Ok(body)
}

pub(crate) fn materialize(input: &Value) -> Result<Value> {
    let cert = &input["certification"];
    let connection = &input["connection"];
    if cert["connectionId"] != connection["id"]
        || cert["connectionDigest"] != input["connectionDigest"]
    {
        return Err("model certification does not match connection definition".into());
    }
    if cert["protocol"] != connection["protocol"] || cert["codecVersion"] != input["codecVersion"] {
        return Err("model certification does not match the active protocol codec".into());
    }
    if cert["engineContractHash"] != input["contractDigest"] {
        return Err("model certification does not match the engine contract".into());
    }
    if cert["qualificationContextDigest"] != input["qualificationContextDigest"] {
        return Err("model certification does not match the qualification context".into());
    }
    if evidence::timestamp(&cert["expiresAt"])?
        <= input["now"].as_i64().ok_or("invalid certification clock")?
    {
        return Err("model certification has expired".into());
    }
    let mut records = Vec::new();
    for capability in array(&cert["capabilities"]).iter().map(string) {
        let suite = suite_for(capability);
        let mut scope = json!({"protocol":cert["protocol"],"suite":suite,"mode":if capability == "streaming" { "native-sse" } else if engine(capability) { "app-server-mcp" } else { "buffered" },"toolTypes":if capability == "structuredToolCalls" || engine(capability) { vec!["function"] } else { vec![] }});
        let mut dependencies = json!({"modelProtocol":{"digest":input["modelProtocolDigest"],"ref":format!("{}:{}:{}",string(&connection["id"]),string(&cert["protocol"]),string(&cert["codecVersion"]))},"access":{"digest":input["credentialRealm"],"ref":format!("{}:credential-realm",string(&connection["id"]))},"report":{"digest":cert["reportDigest"],"ref":cert["certificationId"]}});
        if engine(capability) {
            dependencies["engineBuild"] = json!({"digest":input["engineBuildDigest"],"ref":input["qualificationContextDigest"]});
        }
        if capability == "childDirectedRoute" {
            scope["routes"] = cert["routes"].clone();
            let mut refs: Vec<_> = array(&cert["routes"])
                .iter()
                .map(|r| {
                    format!(
                        "{}->{}",
                        string(&r["parentConnectionId"]),
                        string(&r["childConnectionId"])
                    )
                })
                .collect();
            refs.sort();
            dependencies["route"] = json!({"digest":input["routeDigest"],"ref":refs.join(",")});
        }
        records.push(evidence::normalize(&json!({"capability":capability,"status":"verified","scope":scope,"source":"conformance-suite","credentialRealm":input["credentialRealm"],"environmentDigest":input["environmentDigest"],"evidenceRef":format!("certification:{}:{capability}",string(&cert["certificationId"])),"checkedAt":cert["checkedAt"],"expiresAt":cert["expiresAt"],"reportRef":cert["reportDigest"],"reportDigest":cert["reportDigest"],"sample":{"suiteId":suite,"suiteVersion":1,"outcome":"passed","certificationId":cert["certificationId"],"profileId":cert["profileId"]},"assertions":[{"id":format!("certification:{capability}"),"result":"passed","expected":"capability present in a passed signed-off certification report","observed":{"certificationId":cert["certificationId"],"profileId":cert["profileId"]},"traceRef":cert["reportDigest"]}],"dependencies":dependencies,"supersedes":[]}),evidence::RecordMode::Bound)?);
    }
    Ok(records.into())
}
