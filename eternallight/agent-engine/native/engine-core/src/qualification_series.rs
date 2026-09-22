use crate::{
    Result,
    canonical::{array, string},
    contracts, delegation,
    digest::digest,
    session_identity,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const SCENARIOS: [&str; 3] = ["spawn-route", "message-running", "wait"];
const SUITE: &str = "codex-engine-delegation-execution-v1";
pub(crate) fn bootstrap(plan: &Value) -> Value {
    let mut plan = plan.clone();
    plan["id"] = format!("bootstrap-{}", string(&plan["id"])).into();
    if array(&plan["allowedChildModels"]).is_empty() {
        plan["allowedChildModels"] = json!([plan["rootRoute"]]);
    }
    if array(&plan["allowedChildEdges"]).is_empty() {
        plan["allowedChildEdges"] =
            json!([{"parentConnectionId":plan["rootRoute"],"childConnectionId":plan["rootRoute"]}]);
    }
    if let Some(routes) = plan["routes"].as_array_mut() {
        for route in routes {
            for field in ["enabledExtensions", "authorizedExtensions"] {
                let mut values: BTreeSet<String> = array(&route[field])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                values.extend(
                    array(&contracts::definitions()["DELEGATION_EXECUTION_CAPABILITIES"])
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
                if field == "enabledExtensions" {
                    values.insert("toolLoop".into());
                }
                route[field] = json!(values);
            }
        }
    }
    plan
}

/// Executes a fixed, bounded qualification series. Results are attributed to
/// their exact scenario; one successful probe cannot certify another scenario.
pub struct QualificationSeries {
    input: Value,
    plan: Value,
    index: usize,
    stages: Vec<Value>,
    observations: Value,
    digests: Vec<Value>,
    awaiting: bool,
}
impl QualificationSeries {
    pub fn new(input: Value) -> Result<Self> {
        chrono::DateTime::parse_from_rfc3339(string(&input["now"]))
            .map_err(|_| "invalid qualification timestamp")?;
        if input["connection"]["id"].as_str().is_none_or(str::is_empty) {
            return Err("qualification connection is required".into());
        }
        let plan = bootstrap(&input["sessionPlan"]);
        Ok(Self {
            input,
            plan,
            index: 0,
            stages: Vec::new(),
            observations: json!({}),
            digests: Vec::new(),
            awaiting: false,
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "next" => {
                if self.index >= SCENARIOS.len() {
                    return Ok(Value::Null);
                }
                self.awaiting = true;
                Ok(json!({"scenario":SCENARIOS[self.index],"sessionPlan":self.plan}))
            }
            "record" => {
                if !self.awaiting
                    || self.index >= SCENARIOS.len()
                    || input["scenario"] != SCENARIOS[self.index]
                {
                    return Err("qualification result is not bound to the active scenario".into());
                }
                if input.to_string().len() > 8 * 1024 * 1024 {
                    return Err("qualification scenario result exceeds limit".into());
                }
                let scenario = SCENARIOS[self.index];
                let result = &input["result"];
                let error = &input["error"];
                let stage = if error.is_null() {
                    self.observations[scenario] =
                        result.get("observations").cloned().unwrap_or(json!({}));
                    self.digests.push(result["eventDigest"].clone());
                    json!({"stageId":scenario,"status":"passed","eventDigest":result["eventDigest"],"budget":result["budget"]})
                } else {
                    self.observations[scenario] = error
                        .get("conformanceObservations")
                        .cloned()
                        .unwrap_or(json!({}));
                    json!({"stageId":scenario,"status":"failed","category":error["conformanceDiagnostics"]["category"].as_str().unwrap_or("conformance"),"message":string(&error["message"]).chars().take(500).collect::<String>(),"budget":error["budgetSnapshot"],"diagnostics":error["conformanceDiagnostics"]})
                };
                self.stages.push(stage);
                self.index += 1;
                self.awaiting = false;
                Ok(Value::Null)
            }
            "finish" => {
                if self.index != SCENARIOS.len() || self.awaiting {
                    return Err("qualification series is incomplete".into());
                }
                self.finish()
            }
            _ => Err("unknown qualification series operation".into()),
        }
    }
    fn finish(&self) -> Result<Value> {
        let connection = &self.input["connection"];
        let id = string(&connection["id"]);
        let protocol = string(&self.input["protocol"]);
        let now = string(&self.input["now"]);
        let parsed = chrono::DateTime::parse_from_rfc3339(now)
            .map_err(|_| "invalid qualification timestamp")?;
        let expires = (parsed + chrono::Duration::days(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let realm = session_identity::dispatch(
            "connection.credentialRealm",
            &self.input["credentialScope"],
        )?;
        let evidence_ref = format!("conformance:{id}:{protocol}:{now}:delegation");
        let failed: Vec<_> = self
            .stages
            .iter()
            .filter(|stage| stage["status"] != "passed")
            .collect();
        let event_digest = digest(&json!(self.digests));
        let forced = self
            .observations
            .as_object()
            .into_iter()
            .flatten()
            .any(|(_, o)| o["forcedChildToolChoice"] == true);
        let mut records = Vec::new();
        for capability in array(&contracts::definitions()["DELEGATION_EXECUTION_CAPABILITIES"]) {
            let name = string(capability);
            let scenario = match name {
                "childWait" => "wait",
                "childMessageRunning" => "message-running",
                _ => "spawn-route",
            };
            let mut observations = self.observations[scenario].clone();
            if !observations.is_object() {
                observations = json!({});
            }
            observations["terminated"] = true.into();
            let evaluated = delegation::dispatch(
                "delegation.evaluate",
                &json!({"capability":capability,"observations":observations,"traceRef":event_digest}),
            )?;
            let mut scope = json!({"protocol":protocol,"suite":SUITE,"mode":"app-server-mcp","toolTypes":["function"],"behaviors":[capability]});
            let edge = json!({"parentConnectionId":id,"childConnectionId":id});
            if name == "childMessageRunning" && forced {
                scope["parameters"] = json!({"selectionMode":"forced-child-tool-choice"});
            }
            if name == "childDirectedRoute" {
                scope["routes"] = json!([edge]);
            }
            let context = self
                .input
                .get("qualificationContextDigest")
                .cloned()
                .unwrap_or("default".into());
            let mut dependencies = json!({"modelProtocol":{"digest":digest(&json!({"connectionDigest":session_identity::definition(connection),"protocol":protocol,"codecVersion":"3"})),"ref":format!("{id}:{protocol}")},"engineBuild":{"digest":digest(&json!({"engineContractHash":digest(&contracts::definitions()["ENGINE_CONTRACT"]),"qualificationContextDigest":context})),"ref":context},"access":{"digest":realm,"ref":format!("{id}:credential-realm")}});
            if name == "childDirectedRoute" {
                dependencies["route"] =
                    json!({"digest":digest(&json!([edge])),"ref":format!("{id}->{id}")});
            }
            let status = &evaluated["status"];
            records.push(json!({"capability":capability,"status":status,"scope":scope,"source":"conformance-suite","credentialRealm":realm,"environmentDigest":self.input["sessionPlan"]["routes"][0]["environmentDigest"],"evidenceRef":format!("{evidence_ref}:{name}"),"checkedAt":now,"expiresAt":expires,"sample":{"suiteId":SUITE,"suiteVersion":2,"outcome":if status=="verified"{"passed"}else if status=="failed"{"failed"}else{"inconclusive"},"eventDigest":event_digest},"assertions":evaluated["assertions"],"dependencies":dependencies,"supersedes":[]}));
        }
        let status = if failed.is_empty() && records.iter().all(|r| r["status"] == "verified") {
            "verified"
        } else {
            "failed"
        };
        let metrics:Vec<_>=records.iter().map(|record|json!({"capability":record["capability"],"status":record["status"],"assertions":array(&record["assertions"]).iter().map(|a|json!({"id":a["id"],"result":a["result"]})).collect::<Vec<_>>()})).collect();
        let mut attempt = json!({"protocol":protocol,"suite":SUITE,"status":status,"diagnostics":{"stages":self.stages},"metrics":{"capabilityResults":metrics}});
        if let Some(first) = failed.first() {
            attempt["category"] = first["category"].clone();
            attempt["message"] = failed
                .iter()
                .map(|s| format!("{}:{}", string(&s["stageId"]), string(&s["message"])))
                .collect::<Vec<_>>()
                .join("; ")
                .chars()
                .take(500)
                .collect::<String>()
                .into();
        }
        Ok(json!({"attempt":attempt,"records":records}))
    }
}
