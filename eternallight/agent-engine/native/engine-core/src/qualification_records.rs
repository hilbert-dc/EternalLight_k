use crate::{
    Result, behavior, canonical::string, contracts, delegation, digest::digest,
    qualification_series, session_identity,
};
use serde_json::{Value, json};

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "qualification.edgePlan" => {
            let parent = &input["parentPlan"];
            let child = &input["childPlan"];
            if parent["rootRoute"] == child["rootRoute"] {
                return Err("directed edge conformance requires distinct connections".into());
            }
            let edge = json!({"parentConnectionId":parent["rootRoute"],"childConnectionId":child["rootRoute"]});
            let mut plan = parent.clone();
            plan["id"] = format!(
                "bootstrap-edge-{}",
                digest(&json!([parent["id"], child["id"], edge]))
            )
            .into();
            plan["routes"] = json!([parent["routes"][0], child["routes"][0]]);
            plan["allowedChildModels"] = json!([child["rootRoute"]]);
            plan["allowedChildEdges"] = json!([edge]);
            let p = chrono::DateTime::parse_from_rfc3339(string(&parent["expiresAt"]))
                .map_err(|_| "invalid parent expiry")?;
            let c = chrono::DateTime::parse_from_rfc3339(string(&child["expiresAt"]))
                .map_err(|_| "invalid child expiry")?;
            plan["expiresAt"] = p
                .min(c)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                .into();
            Ok(qualification_series::bootstrap(&plan))
        }
        "qualification.edgeResult" => edge_result(input),
        "qualification.coreResult" => core_result(input),
        _ => Err("unknown qualification record operation".into()),
    }
}

fn edge_result(input: &Value) -> Result<Value> {
    let parent = &input["parentConnection"];
    let child = &input["childConnection"];
    let id = string(&parent["id"]);
    let child_id = string(&child["id"]);
    let protocol = string(&input["parentProtocol"]);
    let child_protocol = string(&input["childProtocol"]);
    let now = string(&input["now"]);
    let checked =
        chrono::DateTime::parse_from_rfc3339(now).map_err(|_| "invalid edge timestamp")?;
    let realm = session_identity::dispatch(
        "connection.credentialRealm",
        &input["parentCredentialScope"],
    )?;
    let child_realm =
        session_identity::dispatch("connection.credentialRealm", &input["childCredentialScope"])?;
    let context = &input["qualificationContextDigest"];
    let evidence_ref = format!("conformance:{id}:{protocol}:{now}:edge:{child_id}");
    let error = &input["error"];
    let result = &input["result"];
    let mut observations = if error.is_null() {
        result["observations"].clone()
    } else {
        error["conformanceObservations"].clone()
    };
    if !observations.is_object() {
        observations = json!({});
    }
    if error.is_null() {
        observations["terminated"] = true.into();
    }
    let evaluated = delegation::dispatch(
        "delegation.evaluate",
        &json!({"capability":"childDirectedRoute","observations":observations,"traceRef":if error.is_null(){result["eventDigest"].clone()}else{json!(evidence_ref)}}),
    )?;
    let status = if error.is_null() && evaluated["status"] == "verified" {
        "verified"
    } else {
        "failed"
    };
    let message = string(&error["message"])
        .chars()
        .take(500)
        .collect::<String>();
    let mut attempt =
        json!({"protocol":protocol,"suite":"codex-engine-directed-edge-v1","status":status});
    let mut sample = json!({"suiteId":"codex-engine-directed-edge-v1","suiteVersion":1,"outcome":if status=="verified"{"passed"}else{"failed"}});
    if error.is_null() {
        sample["eventDigest"] = result["eventDigest"].clone();
        sample["parentSynthesizedReceipt"] = result["parentSynthesizedReceipt"].clone();
    } else {
        attempt["category"] = "conformance".into();
        attempt["message"] = message.clone().into();
        sample["message"] = message.into();
    }
    Ok(
        json!({"attempt":attempt,"record":{"capability":"childDirectedRoute","status":status,"scope":{"protocol":protocol,"suite":"codex-engine-directed-edge-v1","mode":"app-server-mcp","toolTypes":["function"],"behaviors":["childDirectedRoute"],"routes":[{"parentConnectionId":id,"childConnectionId":child_id}],"parameters":{"parentCredentialRealm":realm,"childCredentialRealm":child_realm}},"source":"conformance-suite","credentialRealm":realm,"environmentDigest":input["parentPlan"]["routes"][0]["environmentDigest"],"evidenceRef":evidence_ref,"checkedAt":now,"expiresAt":(checked+chrono::Duration::days(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"sample":sample,"assertions":evaluated["assertions"],"dependencies":{"modelProtocol":{"digest":digest(&json!({"connectionDigest":session_identity::definition(parent),"protocol":protocol,"codecVersion":"3"})),"ref":format!("{id}:{protocol}")},"engineBuild":{"digest":digest(&json!({"engineContractHash":digest(&contracts::definitions()["ENGINE_CONTRACT"]),"qualificationContextDigest":context})),"ref":context},"route":{"digest":session_identity::dispatch("connection.directedDigest",input)?,"ref":format!("{id}:{protocol}->{child_id}:{child_protocol}")},"access":{"digest":realm,"ref":format!("{id}:credential-realm")}},"supersedes":[]}}),
    )
}

fn core_result(input: &Value) -> Result<Value> {
    let connection = &input["connection"];
    let id = string(&connection["id"]);
    let protocol = string(&input["protocol"]);
    let now = string(&input["now"]);
    let checked =
        chrono::DateTime::parse_from_rfc3339(now).map_err(|_| "invalid core timestamp")?;
    let passed = input["passed"] == true;
    let error = input["error"].as_str();
    let status = if passed { "verified" } else { "failed" };
    let realm =
        session_identity::dispatch("connection.credentialRealm", &input["credentialScope"])?;
    let context = &input["qualificationContextDigest"];
    let assertions = if passed {
        vec![
            "tool-loop-completion",
            "root-tool-selected",
            "root-tool-executed",
            "tool-result-consumed",
            "root-turn-completed",
        ]
    } else {
        vec!["tool-loop-completion"]
    };
    let mut suite = behavior::suite(
        &json!({"kind":"core-integration","suiteId":"codex-engine-core-integration-v1","suiteVersion":1,"status":if passed{"passed"}else{"failed"},"blockingFailures":if passed{vec![]}else if error.is_some(){vec!["core-tool-loop-error"]}else{vec!["core-tool-loop-incomplete"]},"metrics":error.map(|message|json!({"message":message.chars().take(300).collect::<String>()})).unwrap_or_else(||json!({}))}),
    )?;
    suite["record"] = json!({"capability":"toolLoop","status":status,"scope":{"protocol":protocol,"suite":"codex-engine-core-integration-v1","mode":"app-server-mcp","toolTypes":["function"]},"source":"conformance-suite","credentialRealm":realm,"environmentDigest":input["sessionPlan"]["routes"][0]["environmentDigest"],"evidenceRef":format!("core-conformance:{id}:{protocol}:{now}:tool-loop"),"checkedAt":now,"expiresAt":(checked+chrono::Duration::days(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"sample":{"suiteId":"codex-engine-core-integration-v1","suiteVersion":1,"outcome":if passed{"passed"}else{"failed"}},"assertions":assertions.iter().map(|id|json!({"id":id,"result":if passed{"passed"}else{"failed"},"expected":true,"observed":passed,"traceRef":input["proofPath"]})).collect::<Vec<_>>(),"dependencies":{"modelProtocol":{"digest":digest(&json!({"connectionDigest":session_identity::definition(connection),"protocol":protocol,"codecVersion":"3"})),"ref":format!("{id}:{protocol}")},"engineBuild":{"digest":digest(&json!({"engineContractHash":digest(&contracts::definitions()["ENGINE_CONTRACT"]),"qualificationContextDigest":context})),"ref":context},"access":{"digest":realm,"ref":format!("{id}:credential-realm")}},"supersedes":[]});
    Ok(suite)
}
