use crate::{
    Result,
    canonical::{array, string},
    contracts,
};
use serde_json::{Value, json};

fn contract(capability: &str) -> Result<Value> {
    let domain = contracts::domain(capability)?;
    let (suites, dependencies, fields) = if domain == "model" {
        (
            vec![
                "synthetic-tool-probe-v1",
                "codex-engine-model-certification-v1",
                "test-baseline-v1",
            ],
            vec!["modelProtocol", "access"],
            vec!["protocol", "suite", "mode"],
        )
    } else if capability == "toolLoop" {
        (
            vec!["codex-engine-core-integration-v1", "test-baseline-v1"],
            vec!["modelProtocol", "engineBuild", "access"],
            vec!["protocol", "suite", "mode", "toolTypes"],
        )
    } else {
        let mut dependencies = vec!["modelProtocol", "engineBuild", "access"];
        if capability == "childDirectedRoute" {
            dependencies.push("route");
        }
        (
            vec![
                "codex-engine-delegation-execution-v1",
                "codex-engine-directed-edge-v1",
                "codex-engine-delegation-lifecycle-v1",
                "test-baseline-v1",
            ],
            dependencies,
            vec!["protocol", "suite", "mode", "toolTypes"],
        )
    };
    Ok(
        json!({"capability":capability,"allowedSuites":suites,"requiredDependencies":dependencies,"requiredScopeFields":fields}),
    )
}

fn assertions(record: &Value) -> Vec<String> {
    let capability = string(&record["capability"]);
    let suite = string(&record["scope"]["suite"]);
    let certification = format!("certification:{capability}");
    let has_certification = array(&record["assertions"])
        .iter()
        .any(|a| a["id"] == certification);
    match suite {
        "test-baseline-v1" => vec![format!("fixture:{capability}")],
        "codex-engine-model-certification-v1" => vec![certification],
        "synthetic-tool-probe-v1" => vec![format!("model:{capability}:observed")],
        "codex-engine-core-integration-v1" => vec![if has_certification {
            certification
        } else {
            "tool-loop-completion".into()
        }],
        "codex-engine-directed-edge-v1" => vec![
            "directed-child-model-observed".into(),
            "trusted-child-identity".into(),
        ],
        "codex-engine-delegation-execution-v1" | "codex-engine-delegation-lifecycle-v1"
            if has_certification =>
        {
            vec![certification]
        }
        _ => {
            let ids: &[&str] = match (suite, capability) {
                ("codex-engine-delegation-execution-v1", "childSpawn") => {
                    &["childSpawn", "trusted-child-identity"]
                }
                ("codex-engine-delegation-execution-v1", "childMessageRunning") => &[
                    "childMessageRunning",
                    "child-running-gate-observed",
                    "parent-followup-token-queued",
                    "child-followup-visible",
                    "child-followup-consumed",
                ],
                ("codex-engine-delegation-execution-v1", "childWait") => {
                    &["childWait", "child-terminal-result-observed"]
                }
                ("codex-engine-delegation-execution-v1", "childDirectedRoute") => {
                    &["directed-child-model-observed", "trusted-child-identity"]
                }
                ("codex-engine-delegation-lifecycle-v1", "childFailure") => {
                    &["child-failure-propagated", "trusted-child-identity"]
                }
                ("codex-engine-delegation-lifecycle-v1", "childFollowupIdle") => {
                    &["idle-child-resumed", "trusted-child-identity"]
                }
                ("codex-engine-delegation-lifecycle-v1", "childCancelTree") => &[
                    "root-revoked",
                    "child-tree-stopped",
                    "no-request-after-revocation",
                ],
                ("codex-engine-delegation-lifecycle-v1", "childRecovery") => &[
                    "recovery-boundary-preserved",
                    "no-duplicate-child-side-effect",
                ],
                ("codex-engine-delegation-lifecycle-v1", "contextLifecycle") => {
                    &["bounded-context-preserved", "child-lineage-preserved"]
                }
                _ => &[],
            };
            ids.iter().map(|s| (*s).to_owned()).collect()
        }
    }
}

fn status(name: &str) -> Value {
    json!({"status":name,"missingAssertions":[],"mismatchedDependencies":[]})
}

fn evaluate(record: &Value, expected: &Value) -> Value {
    let Ok(contract) = contract(string(&record["capability"])) else {
        return status("unsupported-capability");
    };
    if !array(&contract["allowedSuites"]).contains(&record["scope"]["suite"]) {
        return status("unsupported-suite");
    }
    if record["measurementStatus"] != "passed" {
        return status(if record["measurementStatus"] == "failed" {
            "failed"
        } else {
            "not-qualified"
        });
    }
    let missing_scope: Vec<_> = array(&contract["requiredScopeFields"])
        .iter()
        .filter(|field| {
            let value = &record["scope"][string(field)];
            value.is_null() || value == "" || value.as_array().is_some_and(Vec::is_empty)
        })
        .cloned()
        .collect();
    if !missing_scope.is_empty() {
        let mut result = status("incomplete-scope");
        result["missingScopeFields"] = missing_scope.into();
        return result;
    }
    let required = assertions(record);
    if required.is_empty() {
        return status("unsupported-suite");
    }
    let missing: Vec<_> = required
        .iter()
        .filter(|id| {
            array(&record["assertions"])
                .iter()
                .rev()
                .find(|a| a["id"] == **id)
                .is_none_or(|a| a["result"] != "passed")
        })
        .cloned()
        .collect();
    if !missing.is_empty() {
        let mut result = status("incomplete-assertions");
        result["missingAssertions"] = missing.into();
        return result;
    }
    let missing: Vec<_> = array(&contract["requiredDependencies"])
        .iter()
        .filter(|kind| record["dependencies"][string(kind)].is_null())
        .cloned()
        .collect();
    if !missing.is_empty() {
        let mut result = status("incomplete-dependencies");
        result["missingDependencies"] = missing.into();
        return result;
    }
    let mismatched: Vec<_> = expected
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::iter)
        .filter(|(kind, expected)| {
            if expected.is_null()
                || !array(&contract["requiredDependencies"])
                    .iter()
                    .any(|v| v == kind.as_str())
            {
                return false;
            }
            let actual = &record["dependencies"][kind.as_str()]["digest"];
            if let Some(accepted) = expected.as_array() {
                !accepted.contains(actual)
            } else {
                actual != *expected
            }
        })
        .map(|(kind, _)| kind.clone())
        .collect();
    if !mismatched.is_empty() {
        let mut result = status("dependency-mismatch");
        result["mismatchedDependencies"] = mismatched.into();
        return result;
    }
    if record["source"] == "conformance-suite"
        && (string(&record["reportRef"]).is_empty()
            || record["reportRef"] != record["reportDigest"])
    {
        return status("untrusted-report");
    }
    let mut result = status("qualified");
    result["requiredAssertions"] = required.into();
    result["requiredDependencies"] = contract["requiredDependencies"].clone();
    result
}

pub(crate) fn dispatch(method: &str, input: &Value) -> Result<Value> {
    match method {
        "proof.contracts" => contracts::definitions()["CAPABILITIES"]
            .as_object()
            .unwrap()
            .values()
            .map(|value| {
                contract(string(value)).map(|definition| (string(value).to_owned(), definition))
            })
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(Value::Object),
        "proof.assertions" => Ok(assertions(input).into()),
        "proof.evaluate" => Ok(evaluate(&input["record"], &input["expectedDependencies"])),
        _ => Err("unknown proof operation".into()),
    }
}
