use serde_json::{Value, json};
pub(crate) fn request(
    name: &str,
    args: &Value,
) -> Result<(&'static str, String, Option<Value>), String> {
    let base = format!(
        "/api/projects/{}/tasks/{}/work-plan",
        super::percent_encode(args["projectId"].as_str().unwrap_or_default()),
        super::percent_encode(args["taskId"].as_str().unwrap_or_default())
    );
    if name == "read_work_plan" {
        if args.get("planVersion").is_some_and(|v| !v.is_null())
            && ["planWorkId", "beforePlanRowId"]
                .iter()
                .any(|field| args.get(*field).is_some_and(|v| !v.is_null()))
            || args.get("beforePlanRowId").is_some_and(|v| !v.is_null())
                && ["goalId", "planWorkId"]
                    .iter()
                    .any(|field| args.get(*field).is_some_and(|v| !v.is_null()))
        {
            return Err(
                "Select an exact current Work planVersion or a history directory cursor, not both"
                    .into(),
            );
        }
        let mut path = format!(
            "{base}?threadId={}&expectedRevision={}",
            super::percent_encode(args["threadId"].as_str().unwrap_or_default()),
            args["expectedRevision"]
        );
        if let Some(v) = args.get("goalId").filter(|v| !v.is_null()) {
            let id = v
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 240)
                .ok_or("Invalid goalId")?;
            path.push_str(&format!("&goalId={}", super::percent_encode(id)));
        }
        if let Some(id) = args.get("planWorkId").filter(|v| !v.is_null()) {
            let id = id
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 240)
                .ok_or("Invalid planWorkId")?;
            path.push_str(&format!("&planWorkId={}", super::percent_encode(id)));
        }
        if let Some(cursor) = args.get("beforePlanRowId").filter(|v| !v.is_null()) {
            let cursor = cursor
                .as_i64()
                .filter(|v| *v > 0)
                .ok_or("Invalid beforePlanRowId")?;
            path.push_str(&format!("&beforePlanRowId={cursor}"));
        }
        if let Some(version) = args.get("planVersion").filter(|v| !v.is_null()) {
            let version = version
                .as_i64()
                .filter(|v| *v > 0)
                .ok_or("Invalid planVersion")?;
            path.push_str(&format!("&planVersion={version}"));
        }
        Ok(("GET", path, None))
    } else {
        if args["expectedPlanVersion"].as_i64().is_none_or(|v| v < 0)
            || args["idempotencyKey"]
                .as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 160)
            || !args["plan"].is_object()
        {
            return Err("Use a versioned plan and idempotencyKey".into());
        }
        let mut body = args.clone();
        body.as_object_mut().unwrap().remove("projectId");
        Ok(("POST", base, Some(body)))
    }
}
pub(crate) fn specs() -> Vec<Value> {
    let short = json!({"type":"string","maxLength":500});
    let goal = json!({"type":"object","additionalProperties":false,"required":["id","title","acceptance","kind","status","dependsOn","readingQuestion","checkScope","approach","assessment","evidence","sourceIds","sourceChecks","continuation"],"properties":{
 "id":{"type":"string","minLength":1,"maxLength":80},"title":{"type":"string","minLength":1,"maxLength":100},"acceptance":{"type":"string","minLength":1,"maxLength":300,"description":"A model-owned method success condition, not the author contract. Refine it when correcting the method. Original author requirements, scope, budget and minimum independent reviews remain separate and cannot be reduced here."},"kind":{"type":"string","enum":["deliverable","check","continuation"],"description":"The method kind may change when replacing or merging goals; valid current evidence is still required for any satisfied claim."},"status":{"type":"string","enum":["pending","inProgress","satisfied","blocked"]},"dependsOn":{"type":"array","maxItems":12,"items":{"type":"string"},"description":"Current method dependencies may evolve or be removed with retired methods. Refer only to goals in this bounded window; dependencies must remain acyclic. Author prerequisites are not waived by editing this graph."},"readingQuestion":short,"checkScope":short,"approach":short,"assessment":short,
 "sourceChecks":{"description":"Checks cite the exact authorized sourceId, current revisionId, contentHash and original quote evidence. Directory sourceIds alone cannot certify a check. Source changes invalidate satisfied checks.","type":"array","maxItems":8,"items":crate::chapter_acceptance_schema::source_check()},
 "continuation":{"description":"Use null for ordinary methods. If a continuation method helps, name the exact existing run from conversationRuns, with proposalId:null until the real resumeRun handoff is registered. Copy that proposalId before claiming satisfaction. The handoff proves transfer, not completion of remaining chapters; a saved continuation method is not mandatory for requesting a handoff.","anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["runId","proposalId"],"properties":{"runId":{"type":"string","minLength":1,"maxLength":160},"proposalId":{"type":["string","null"],"minLength":1,"maxLength":160}}}]},
 "evidence":{"description":"Saved outputs created by completed tasks of this Work. Check goals require sourceChecks; continuation goals use their exact proposalId and empty revision evidence.","type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"required":["taskId","revisionId"],"properties":{"taskId":{"type":"string"},"revisionId":{"type":"string"}}}},"sourceIds":{"description":"Authorized source directory references for planning. These IDs alone do not prove a completed check; use sourceChecks with the actual current text and quote.","type":"array","maxItems":8,"items":{"type":"string"}}}});
    ["read_work_plan","update_work_plan"].iter().map(|name|{
 let mut props=json!({"projectId":{"type":"string"},"taskId":{"type":"string"},"threadId":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0}});
 let mut required=vec!["projectId","taskId","threadId","expectedRevision"];
 let description=if *name=="read_work_plan" {props["planWorkId"]=json!({"type":"string","minLength":1,"maxLength":240,"description":"Optional exact workTaskId selected from priorPlans. Read-only historical reference in this project/conversation; reassess evidence in current Work before carrying goals."});props["beforePlanRowId"]=json!({"type":"integer","minimum":1,"description":"Directory pagination only: copy nextBeforePlanRowId exactly; omit planWorkId."});props["planVersion"]=json!({"type":"integer","minimum":1,"description":"Exact immutable snapshot of this Work, including retired methods. May combine with goalId; omit planWorkId and beforePlanRowId. Read-only reference, never current completion evidence."});props["goalId"]=json!({"type":["string","null"],"maxLength":80});"Read the persisted bounded model-method window for this Work. It is not the author contract. Use planVersion to recover an exact earlier snapshot after methods are retired or replaced. Omit goalId for compact goal list/version; read one exact goalId for acceptance, reading questions, check scope, method and evidence. New work has version=0 and a paginated priorPlans directory. Select a relevant plan with planWorkId, then read exact goals; decide relevance from author intent. Historical goals and evidence must be reassessed against current authorized sources before updating the current plan. Reading history grants no execution authority."}
 else {required.extend(["expectedPlanVersion","idempotencyKey","plan"]);props["expectedPlanVersion"]=json!({"type":"integer","minimum":0});props["idempotencyKey"]=json!({"type":"string","minLength":1,"maxLength":160});props["plan"]=json!({"type":"object","additionalProperties":false,"required":["intentMode","goals"],"properties":{"intentMode":{"type":"string","enum":["planningOnly","executeGoals"],"description":"Describe whether this method window is currently planning or execution. Either mode may be corrected. This field does not grant or revoke execution, close the Work, cancel registered actions or alter author requirements."},"goals":{"type":"array","minItems":0,"maxItems":12,"items":goal}}});"Save a bounded 0–12 goal method window when a plan helps; simple actions need no saved plan. Methods can be refined, replaced, merged or retired, including acceptance, kind, dependencies and mode. Immutable plan versions retain the earlier decisions; explain meaningful changes in commentary. The plan is model-owned and never replaces the complete original author request. Editing it cannot change author scope, protected material, budgets, pending registered actions, or minimum independent verification, and cannot finish the Work. Use current sources and real saved evidence for satisfied claims; checks require exact sourceChecks and quotations, deliverables require completed current Work outputs, continuation requires its real saved resumeRun proposal. A handoff proves transfer, not completion of remaining chapters. At most one goal may be inProgress; Runtime derives activeGoalId. Ordinary goals use continuation:null. Final delivery is judged against original author requirements and native evidence ledgers, independent of this plan. CAS uses read_work_plan.version; reuse idempotencyKey only for exact retries. No execution, new budget, publication or author approval is granted."};
 json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":props}})
 }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_plan_contract_and_dispatch() {
        let specs = specs();
        assert_eq!(specs.len(), 2);
        let read = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1,"goalId":"a"});
        let (method, path, body) = request("read_work_plan", &read).unwrap();
        assert_eq!(method, "GET");
        assert!(path.ends_with("goalId=a"));
        assert!(body.is_none());
        let history = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1,"planWorkId":"saved-work","goalId":"outline"});
        assert!(
            request("read_work_plan", &history)
                .unwrap()
                .1
                .contains("planWorkId=saved-work")
        );
        let page = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1,"beforePlanRowId":42});
        assert!(
            request("read_work_plan", &page)
                .unwrap()
                .1
                .contains("beforePlanRowId=42")
        );
        assert!(request("read_work_plan", &json!({"beforePlanRowId":0})).is_err());
        let snapshot = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1,"planVersion":3,"goalId":"retired"});
        assert!(
            request("read_work_plan", &snapshot)
                .unwrap()
                .1
                .ends_with("planVersion=3")
        );
        for args in [
            json!({"planVersion":0}),
            json!({"planVersion":1,"planWorkId":"other"}),
            json!({"planVersion":1,"beforePlanRowId":42}),
            json!({"goalId":"current","beforePlanRowId":42}),
        ] {
            assert!(request("read_work_plan", &args).is_err());
        }
        assert!(request("update_work_plan", &read).is_err());
        let write = json!({"projectId":"p","taskId":"t","threadId":"root","expectedRevision":1,"expectedPlanVersion":0,"idempotencyKey":"first","plan":{"intentMode":"planningOnly","goals":[]}});
        let (method, _, body) = request("update_work_plan", &write).unwrap();
        assert_eq!(method, "POST");
        assert!(body.unwrap().get("projectId").is_none());
        assert_eq!(
            specs[1]["inputSchema"]["properties"]["plan"]["additionalProperties"],
            false
        );
        assert_eq!(
            specs[1]["inputSchema"]["properties"]["plan"]["required"],
            json!(["intentMode", "goals"])
        );
        assert_eq!(
            specs[1]["inputSchema"]["properties"]["plan"]["properties"]["goals"]["minItems"],
            0
        );
        assert_eq!(
            specs[1]["inputSchema"]["properties"]["plan"]["properties"]["goals"]["maxItems"],
            12
        );
        assert_eq!(
            specs[0]["inputSchema"]["properties"]["planVersion"]["minimum"],
            1
        );
        assert!(
            specs[1]["inputSchema"]["properties"]["plan"]["properties"]
                .get("activeGoalId")
                .is_none()
        );
        let goal = &specs[1]["inputSchema"]["properties"]["plan"]["properties"]["goals"]["items"];
        for field in ["continuation", "sourceChecks"] {
            assert!(
                goal["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == field)
            );
        }
        assert!(
            goal["properties"]["kind"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "continuation")
        );
        assert_eq!(
            goal["properties"]["sourceChecks"]["items"]["properties"]["evidence"]["required"],
            json!(["quote"])
        );
        assert_eq!(
            goal["properties"]["continuation"]["anyOf"][1]["required"],
            json!(["runId", "proposalId"])
        );
        let client = crate::RuntimeClient {
        request_dispatched: Default::default(),
            completion_receipt: Default::default(),
            argument_validation: Default::default(),
            publication_gate: Default::default(),
            read_progress: Default::default(),
            host: "127.0.0.1".into(),
            port: 1,
            candidate_token: "candidate".into(),
            capability: "cap".into(),
            operation: "advisory".into(),
        };
        for name in ["read_work_plan", "update_work_plan", "read_source_content"] {
            assert!(client.operation_tools().iter().any(|t| t["name"] == name));
        }
        assert!(crate::validate_execution_identity(&json!({"rootThreadId":"root","callerThreadId":"child","sessionId":"root","parentThreadId":"root"}),"root","update_work_plan").is_err());
    }
}
