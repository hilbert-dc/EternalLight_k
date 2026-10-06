use super::*;

impl HostCoordinator {
    pub(super) fn renew_run(&mut self, id: u64, input: &Value) -> Result<()> {
        let rid = string(&input["runId"]);
        let run = self.runs.get_mut(rid).ok_or("unknown run")?;
        if run.renewal.is_some() {
            return Err("execution lease renewal is already pending".into());
        }
        let state = run.state.apply("snapshot", &Value::Null)?;
        if state["terminal"] == true
            || matches!(
                string(&state["state"]),
                "preparing" | "cancelling" | "cancellation-failed"
            )
        {
            return Err("execution lease renewal requires a live running execution".into());
        }
        self.authority.apply(
            "validate",
            json!({"runId":rid,"leaseId":run.lease["leaseId"],"epoch":state["epoch"]}),
        )?;
        let sid = run.session.clone();
        let session = self.sessions.get_mut(&sid).ok_or("unknown session")?;
        let now = chrono::DateTime::parse_from_rfc3339(&self.now)
            .map_err(|_| "invalid host clock")?
            .timestamp_millis();
        let plan = input["sessionPlan"].clone();
        let lease = crate::renewal::execution(
            &json!({"currentLease":run.lease,"previousPlan":session.plan,"sessionPlan":plan,"epoch":state["epoch"],"now":now}),
        )?;
        run.renewal = Some((id, plan.clone(), lease.clone()));
        let result = self.effect(
            &sid,
            Some(rid),
            Purpose::Renew,
            "renewExecution",
            json!({"lease":lease,"sessionPlan":plan,"now":now}),
        );
        if result.is_err() {
            self.runs.get_mut(rid).ok_or("unknown run")?.renewal = None;
        }
        result
    }

    pub(super) fn finish_renewal(
        &mut self,
        rid: &str,
        acknowledgement: &Value,
        error: &Value,
    ) -> Result<()> {
        let run = self.runs.get_mut(rid).ok_or("unknown run")?;
        let Some((id, plan, lease)) = run.renewal.take() else {
            return Ok(());
        };
        let result = (|| -> Result<()> {
            if let Some(message) = error["message"].as_str() {
                return Err(message.into());
            }
            if acknowledgement != &lease {
                return Err("runtime did not acknowledge the renewed execution lease".into());
            }
            let state = run.state.apply("snapshot", &Value::Null)?;
            self.authority.apply(
                "validate",
                json!({"runId":rid,"leaseId":run.lease["leaseId"],"epoch":state["epoch"]}),
            )?;
            let session = self
                .sessions
                .get_mut(&run.session)
                .ok_or("unknown session")?;
            let now = chrono::DateTime::parse_from_rfc3339(&self.now)
                .map_err(|_| "invalid host clock")?
                .timestamp_millis();
            crate::renewal::execution(
                &json!({"currentLease":run.lease,"previousPlan":session.plan,"sessionPlan":plan,"lease":lease,"epoch":state["epoch"],"now":now}),
            )?;
            self.authority.apply("renew", json!({"runId":rid,"leaseId":run.lease["leaseId"],"epoch":state["epoch"],"planId":plan["id"]}))?;
            session.plan = plan.clone();
            run.lease = lease.clone();
            Ok(())
        })();
        match result {
            Ok(()) => self.resolve(id, json!({"runId":rid,"lease":lease,"planId":plan["id"]})),
            Err(error) => {
                let run = self.runs.get_mut(rid).ok_or("unknown run")?;
                run.renewal_failed = true;
                self.sessions
                    .get_mut(&run.session)
                    .ok_or("unknown session")?
                    .state
                    .apply("block", &Value::Null)?;
                self.reject(id, &error);
                // An IO acknowledgement may be lost after the bridge committed.
                // Revoke rather than leaving divergent owners usable.
                self.cancel(0, rid, "execution-renewal-unresolved")?;
            }
        }
        self.deliver_completion(rid)?;
        Ok(())
    }
}
