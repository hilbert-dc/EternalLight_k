use super::*;

impl HostCoordinator {
    pub(super) fn start_run(&mut self, id: u64, input: &Value) -> Result<()> {
        let rid = string(&input["runId"]);
        let sid = string(&input["sessionId"]);
        if self.runs.contains_key(rid) {
            return Err(format!("run already exists: {rid}"));
        }
        if self.runs.len() >= 100_000 {
            return Err("host run capacity exceeded".into());
        }
        let session = self
            .sessions
            .get_mut(sid)
            .filter(|s| s.visible)
            .ok_or_else(|| format!("unknown session: {sid}"))?;
        let state = session.state.apply("snapshot", &Value::Null)?;
        if state["state"] != "idle" || !state["activeRunId"].is_null() {
            return Err(format!("session is not idle: {sid}"));
        }
        let now = chrono::DateTime::parse_from_rfc3339(&self.now)
            .map_err(|_| "invalid host clock")?
            .timestamp_millis();
        let expiry = chrono::DateTime::parse_from_rfc3339(string(&session.plan["expiresAt"]))
            .map_err(|_| "invalid plan expiry")?
            .timestamp_millis();
        let mut lease = lease::create(
            &json!({"sessionPlan":session.plan,"runId":rid,"budgetId":input["budgetId"],"epoch":1,"mode":input["mode"].as_str().unwrap_or("interactive"),"now":now,"planExpiry":expiry,"maxDurationMs":input["maxDurationMs"],"deadlineAtMs":input["deadlineAtMs"]}),
        )?;
        for field in ["issuedAt", "deadline"] {
            let millis = lease[field].as_f64().ok_or("invalid lease timestamp")? as i64;
            lease[field] = chrono::DateTime::from_timestamp_millis(millis)
                .ok_or("invalid lease timestamp")?
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                .into();
        }
        self.authority.apply("issue", json!({"lease":lease}))?;
        let run = Run {
            session: sid.into(),
            thread: state["threadId"].clone(),
            state: HostRun::new(
                &json!({"runId":rid,"threadId":state["threadId"],"epoch":lease["epoch"],"maximum":self.maximum}),
            )?,
            lease: lease.clone(),
            input: input["input"].clone(),
            options: input["turnOptions"].clone(),
            start: Some(id),
            cancels: Vec::new(),
            interrupted: false,
            completion: None,
            completion_error: None,
            completion_delivered: false,
            renewal: None,
            renewal_failed: false,
        };
        session.state.apply("attach", &rid.into())?;
        let plan = session.plan.clone();
        self.runs.insert(rid.into(), run);
        self.effect(
            sid,
            Some(rid),
            Purpose::Bind,
            "bindExecution",
            json!({"lease":lease,"sessionPlan":plan}),
        )
    }
    pub(super) fn cancel(&mut self, caller: u64, rid: &str, reason: &str) -> Result<()> {
        let run = self
            .runs
            .get_mut(rid)
            .ok_or_else(|| format!("unknown run: {rid}"))?;
        let state = run.state.apply("snapshot", &Value::Null)?;
        if state["terminal"] == true {
            self.resolve(caller,json!({"runId":rid,"status":state["state"],"revoked":state["state"]=="cancelled","interrupted":false,"reconciliationRequired":state["state"]=="outcome-unknown"}));
            return Ok(());
        }
        if caller != 0 {
            run.cancels.push(caller);
        }
        if state["state"] == "cancelling" {
            return Ok(());
        }
        run.state.apply("cancel", &Value::Null)?;
        let reason: String = reason.chars().take(240).collect();
        let authority=self.authority.apply("revoke",json!({"runId":rid,"leaseId":run.lease["leaseId"],"expectedEpoch":state["epoch"],"reason":reason}))?;
        let request = json!({"runId":rid,"leaseId":run.lease["leaseId"],"expectedEpoch":state["epoch"],"revokedEpoch":authority["epoch"],"reason":reason});
        let sid = run.session.clone();
        self.record(
            rid,
            json!({"method":"engine/run/cancellation-requested","params":{"reason":reason}}),
        )?;
        self.effect(&sid, Some(rid), Purpose::Revoke, "revokeExecution", request)
    }
    pub(super) fn terminalize(
        &mut self,
        rid: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let run = self.runs.get_mut(rid).ok_or("unknown run")?;
        if run.state.apply("checkTerminal", &state.into())? != true {
            return Ok(());
        }
        self.authority
            .apply("terminalize", json!({"runId":rid,"outcome":state}))?;
        run.state.apply("terminal", &state.into())?;
        let sid = run.session.clone();
        let turn = run.state.apply("snapshot", &Value::Null)?["turn"].clone();
        let session = self.sessions.get_mut(&sid).ok_or("unknown session")?;
        session
            .state
            .apply("finish", &json!({"runId":rid,"state":state}))?;
        if run.renewal_failed {
            session.state.apply("block", &Value::Null)?;
        }
        let snapshot = session.state.apply("snapshot", &Value::Null)?;
        let mut params = json!({"threadId":snapshot["threadId"],"turnId":turn["id"]});
        if let Some(error) = error {
            params["message"] = error.chars().take(500).collect::<String>().into();
        }
        self.record(
            rid,
            json!({"method":format!("engine/run/{state}"),"params":params}),
        )?;
        self.deliver_completion(rid)?;
        if snapshot["state"] == "closing" {
            self.close_ready(&sid)?;
        }
        Ok(())
    }
    pub(super) fn deliver_completion(&mut self, rid: &str) -> Result<()> {
        let run = self.runs.get_mut(rid).ok_or("unknown run")?;
        let snapshot = run.state.apply("snapshot", &Value::Null)?;
        if snapshot["terminal"] != true || run.completion_delivered || run.renewal.is_some() {
            return Ok(());
        }
        if run.completion.is_none() && run.completion_error.is_none() {
            return Ok(());
        }
        run.completion_delivered = true;
        let mut result = run
            .completion
            .clone()
            .unwrap_or_else(|| json!({"turn":snapshot["turn"]}));
        if !result.is_object() {
            result = json!({});
        }
        result["sessionId"] = run.session.clone().into();
        result["runId"] = rid.into();
        result["status"] = snapshot["state"].clone();
        let error = if snapshot["state"] == "cancelled" {
            Value::Null
        } else {
            run.completion_error.clone().unwrap_or(Value::Null)
        };
        self.effects
            .push(json!({"kind":"completion","runId":rid,"result":result,"error":error}));
        Ok(())
    }
}
