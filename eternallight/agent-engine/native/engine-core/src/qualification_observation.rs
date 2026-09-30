use super::*;

impl QualificationRun {
    pub(super) fn observe(&mut self, kind: &str, value: &Value) -> Result<()> {
        if self.finished || self.cleaning {
            return Ok(());
        }
        if self.observed_bytes + value.to_string().len() > 32 * 1024 * 1024
            || self.events.len() + self.requests.len() + self.responses.len() >= 10_000
        {
            return Err("qualification observation capacity exceeded".into());
        }
        self.observed_bytes += value.to_string().len();
        let tool_names = |field: &Value| -> Vec<Value> {
            array(field)
                .iter()
                .filter_map(|t| {
                    t["name"].as_str().map(|name| {
                        json!(
                            t["namespace"]
                                .as_str()
                                .map_or_else(|| name.into(), |ns| format!("{ns}.{name}"))
                        )
                    })
                })
                .collect()
        };
        match kind {
            "request" => {
                let mut request = value.clone();
                for key in ["tools", "encodedTools"] {
                    request[key] = array(&value[key])
                        .iter()
                        .take(80)
                        .map(|t| {
                            t.get("name")
                                .or_else(|| t.get("type"))
                                .cloned()
                                .unwrap_or("unknown".into())
                        })
                        .collect();
                }
                self.requests.push(request);
                self.trace.record(&json!({"source":"bridge","type":"bridge.request.prepared","details":{"requestId":value["requestId"],"connectionId":value["connectionId"],"logicalModel":value["logicalModel"],"threadId":value["threadId"],"parentThreadId":value["parentThreadId"],"turnId":value["turnId"],"rootTurnId":value["rootTurnId"],"tools":tool_names(&value["tools"]),"encodedTools":tool_names(&value["encodedTools"]),"toolResultCallIds":value["toolResultCallIds"],"visibleTextDigests":value["visibleTextDigests"]}}))?;
            }
            "response" => {
                self.responses.push(value.clone());
                self.trace.record(&json!({"source":"bridge","type":if value["status"]=="failed"{"bridge.response.failed"}else{"bridge.response.completed"},"details":{"requestId":value["requestId"],"connectionId":value["connectionId"],"threadId":value["threadId"],"parentThreadId":value["parentThreadId"],"turnId":value["turnId"],"status":value["status"],"errorCategory":value["errorCategory"],"errorMessage":value["errorMessage"],"errorStatusCode":value["errorStatusCode"],"toolCalls":tool_names(&value["toolCalls"]),"outputTypes":value["outputTypes"],"textLooksLikeToolCall":value["textLooksLikeToolCall"]}}))?;
            }
            "event" => {
                self.events.push(value.clone());
                let item = &value["params"]["item"];
                self.trace.record(&json!({"source":"codex","type":"agent.event","details":{"method":value["method"],"itemType":item["type"],"itemId":item["id"],"status":item.get("status").unwrap_or(&value["params"]["status"]["type"]),"tool":item.get("tool").unwrap_or(&item["action"]),"threadId":value["params"]["threadId"],"turnId":value["params"]["turnId"],"errorMessage":value["params"]["error"]["message"],"errorInfo":value["params"]["error"]["codexErrorInfo"],"willRetry":value["params"]["willRetry"]}}))?;
            }
            _ => return Err("unknown qualification observation".into()),
        }
        if self.gate_release.is_none()
            && !self.workspace.as_ref().unwrap().release.exists()
            && !self.acknowledged()?.is_null()
        {
            self.gate_release = Some(
                Instant::now()
                    + Duration::from_millis(self.input["timeoutMs"].as_u64().unwrap().min(5000)),
            );
        }
        Ok(())
    }
    fn acknowledged(&self) -> Result<Value> {
        let hash = message_evidence::dispatch(
            "message.digest",
            &format!(
                "FOLLOWUP_TOKEN={}",
                self.workspace.as_ref().unwrap().followup
            )
            .into(),
        )?;
        message_evidence::dispatch(
            "message.acknowledged",
            &json!({"events":self.events,"responses":self.responses,"expectedDigest":hash}),
        )
    }
    pub(super) fn observations(&self) -> Result<Value> {
        let workspace = self.workspace.as_ref().unwrap();
        let proof = read_json(&workspace.proof);
        let gate = read_json(&workspace.gate);
        let mut result = delegation::dispatch(
            "delegation.summary",
            &json!({"events":self.events,"responses":self.responses}),
        )?;
        let children: Vec<_> = self
            .requests
            .iter()
            .filter(|r| r["parentThreadId"].as_str().is_some_and(|s| !s.is_empty()))
            .collect();
        let ack = self.acknowledged()?;
        let delivery = self.events.iter().find(|e| {
            !ack.is_null()
                && e["params"]["item"]["id"] == ack["callId"]
                && e["method"] == "item/completed"
                && e["params"]["threadId"] == ack["threadId"]
        });
        let hash = message_evidence::dispatch(
            "message.digest",
            &format!("FOLLOWUP_TOKEN={}", workspace.followup).into(),
        )?;
        let identity = !children.is_empty()
            || [&proof, &gate].iter().any(|v| {
                v["callerThreadId"].is_string() && v["callerThreadId"] != v["rootThreadId"]
            });
        let target = self.input["childModel"]
            .as_str()
            .unwrap_or(string(&self.input["sessionPlan"]["rootRoute"]));
        let terminal_evidence = delegation::dispatch(
            "delegation.terminalWaitEvidence",
            &json!({"events":self.events,"childRequests":children,"expectedText":format!("CHILD_TERMINAL:{}",workspace.receipt),"responses":self.responses,"requests":self.requests}),
        )?;
        result.as_object_mut().unwrap().extend(json!({"childIdentityObserved":identity,"childGateObserved":!gate.is_null(),"childResultObserved":identity&&proof["receipt"]==workspace.receipt,"childTerminalResultObserved":terminal_evidence["observed"],"terminalWaitEvidence":terminal_evidence,"directedRouteObserved":children.iter().any(|r|r["connectionId"]==target),"parentFollowupTokenQueued":!ack.is_null(),"childFollowupVisible":delivery.is_some_and(|event|children.iter().any(|r|r["threadId"]==event["params"]["item"]["agentThreadId"]&&array(&r["visibleTextDigests"]).contains(&hash))),"forcedChildToolChoice":workspace.bridge_options(&self.input)["toolPolicy"]["childToolChoiceRequired"]}).as_object().unwrap().clone());
        Ok(result)
    }
    pub(super) fn proof_ready(&self) -> bool {
        let workspace = self.workspace.as_ref().unwrap();
        let proof = read_json(&workspace.proof);
        let tools: BTreeSet<_> = self
            .responses
            .iter()
            .flat_map(|r| array(&r["toolNames"]).iter().filter_map(Value::as_str))
            .collect();
        proof["receipt"] == workspace.receipt
            && proof["callerThreadId"] != proof["rootThreadId"]
            && ["spawn_agent", "send_message", "wait_agent"]
                .iter()
                .all(|t| tools.contains(t))
            && self.events.iter().any(|e| {
                e["method"] == "item/completed"
                    && e["params"]["item"]["type"] == "collabAgentToolCall"
                    && e["params"]["item"]["tool"] == "wait"
            })
    }
    pub(super) fn success(&mut self) -> Result<()> {
        let workspace = self.workspace.as_ref().unwrap();
        let proof = read_json(&workspace.proof);
        let scenario = string(&self.input["scenario"]);
        let child_receipt = proof["receipt"] == workspace.receipt
            && proof["callerThreadId"].is_string()
            && proof["callerThreadId"] != proof["rootThreadId"];
        if scenario == "message-running" && !self.proof_ready() {
            return Err("Codex conformance proof was not complete after the turn ended".into());
        }
        if scenario == "message-running" && !child_receipt {
            return Err("delegated child MCP proof was not recorded".into());
        }
        if scenario == "message-running" && !workspace.gate.exists() {
            return Err("delegated child gate proof was not recorded".into());
        }
        let observations = self.observations()?;
        if observations["childIdentityObserved"] != true {
            return Err("Codex did not establish a trusted child lineage".into());
        }
        let mut operations: BTreeSet<_> = array(&observations["completedOperations"])
            .iter()
            .filter_map(Value::as_str)
            .collect();
        if child_receipt {
            operations.insert("spawn_agent");
            operations.insert("send_message");
        }
        if matches!(scenario, "message-running" | "wait") && !operations.contains("wait_agent") {
            return Err("Codex did not complete required wait_agent operation".into());
        }
        if self.input["childModel"].is_string() && observations["directedRouteObserved"] != true {
            return Err(format!(
                "directed child route was not observed: {}",
                string(&self.input["childModel"])
            ));
        }
        let synthesized = scenario == "message-running"
            && string(&self.result["message"])
                .contains(&format!("CONFORMANCE_RECEIPT:{}", workspace.receipt));
        let collaboration:Vec<_>=self.events.iter().filter(|e|e["method"]=="item/completed"&&e["params"]["item"]["type"]=="collabAgentToolCall").map(|e|json!({"status":e["params"]["item"]["status"],"tool":e["params"]["item"].get("tool").unwrap_or(&e["params"]["item"]["action"])})).collect();
        self.result = json!({"parentSynthesizedReceipt":synthesized,"eventDigest":digest(&json!({"collaboration":collaboration,"receiptVerified":true,"parentSynthesizedReceipt":synthesized,"operations":operations})),"trace":self.trace.snapshot(),"observations":observations,"budget":self.budget});
        Ok(())
    }
    pub(super) fn diagnostics(&self) -> Result<Value> {
        let workspace = self.workspace.as_ref().unwrap();
        let (phase, tool, scope) = match string(&self.input["scenario"]) {
            "spawn-route" => ("spawn", "spawn_agent", "root"),
            "wait" => ("wait", "wait_agent", "root"),
            _ if !workspace.gate.exists() => ("child-gate", "engine_conformance_gate", "child"),
            _ => ("child-echo", "engine_conformance_echo", "child"),
        };
        let trace = self.trace.snapshot();
        let category = trace::classify(
            &json!({"trace":trace,"options":{"phase":phase,"expectedTool":tool,"requestScope":scope,"timedOut":self.error.as_ref().is_some_and(|s|s.to_lowercase().contains("timed out")),"budgetExhausted":self.budget["overrun"]==true||self.budget["requests"].as_u64().zip(self.budget["maxRequests"].as_u64()).is_some_and(|(r,m)|r>=m)}}),
        );
        Ok(
            json!({"conformanceObservations":self.observations()?,"budgetSnapshot":self.budget,"conformanceDiagnostics":{"phase":phase,"expectedTool":tool,"category":category,"trace":trace,"requests":self.requests.iter().skip(self.requests.len().saturating_sub(24)).collect::<Vec<_>>(),"responses":self.responses.iter().skip(self.responses.len().saturating_sub(24)).collect::<Vec<_>>()}}),
        )
    }
}
