use super::*;

impl HostCoordinator {
    pub(super) fn callback(&mut self, input: &Value) -> Result<()> {
        let Some(pending) = input["id"].as_u64().and_then(|id| self.pending.remove(&id)) else {
            return Ok(());
        };
        let result = self.accept(&pending, &input["result"], &input["error"]);
        if let Err(error) = result {
            self.fail_callback(&pending, &error)?;
        }
        Ok(())
    }
    fn accept(&mut self, p: &Pending, result: &Value, error: &Value) -> Result<()> {
        let sid = &p.session;
        if matches!(p.purpose, Purpose::Turn) {
            return self.turn_result(p, result, error);
        }
        if let Some(message) = error["message"].as_str() {
            return Err(if error["code"] == "TASK_CANCELLED" {
                format!("TASK_CANCELLED:{message}")
            } else {
                message.into()
            });
        }
        match p.purpose {
            Purpose::Recovery => {
                let session = self.sessions.get_mut(sid).ok_or("unknown session")?;
                session.state.apply("expect", &"recovering".into())?;
                session.recovery = result.clone();
                if result["canContinue"] != true {
                    session.state.apply("block", &Value::Null)?;
                    let id = session.start;
                    let value = json!({"sessionId":sid,"threadId":session.resume,"planId":session.plan["id"],"recovery":result});
                    self.resolve(id, value);
                } else {
                    let plan = session.plan.clone();
                    self.effect(
                        sid,
                        None,
                        Purpose::Factory,
                        "runtimeFactory",
                        json!({"sessionPlan":plan}),
                    )?;
                }
            }
            Purpose::Factory => {
                let session = self.sessions.get_mut(sid).ok_or("unknown session")?;
                session.runtime = true;
                session.state.apply(
                    "expect",
                    &if session.resume.is_null() {
                        "preparing"
                    } else {
                        "recovering"
                    }
                    .into(),
                )?;
                self.effect(sid, None, Purpose::Subscribe, "subscribe", json!({}))?;
            }
            Purpose::Subscribe => {
                let session = self.sessions.get_mut(sid).ok_or("unknown session")?;
                session.state.apply(
                    "expect",
                    &if session.resume.is_null() {
                        "preparing"
                    } else {
                        "recovering"
                    }
                    .into(),
                )?;
                let thread = session.resume.clone();
                self.effect(
                    sid,
                    None,
                    if thread.is_null() {
                        Purpose::Ensure
                    } else {
                        Purpose::Resume
                    },
                    if thread.is_null() {
                        "ensureSession"
                    } else {
                        "resumeSession"
                    },
                    json!({"threadId":thread}),
                )?;
            }
            Purpose::Ensure | Purpose::Resume => {
                let session = self.sessions.get_mut(sid).ok_or("unknown session")?;
                session.state.apply("ready", result)?;
                let id = session.start;
                let mut result =
                    json!({"sessionId":sid,"threadId":result,"planId":session.plan["id"]});
                if !session.resume.is_null() {
                    result["recovery"] = session.recovery.clone();
                }
                self.resolve(id, result);
            }
            Purpose::Bind => {
                let rid = p.run.as_deref().ok_or("missing run")?;
                let run = self.runs.get_mut(rid).ok_or("unknown run")?;
                run.state.apply("start", &Value::Null)?;
                let id = run.start.take().ok_or("run already started")?;
                let lease = run.lease.clone();
                let params = json!({"input":run.input,"options":run.options});
                let thread = self
                    .sessions
                    .get_mut(sid)
                    .ok_or("unknown session")?
                    .state
                    .apply("snapshot", &Value::Null)?["threadId"]
                    .clone();
                self.record(
                    rid,
                    json!({"method":"engine/run/started","params":{"threadId":thread}}),
                )?;
                self.effects.push(json!({"kind":"started","id":id,"runId":rid,"result":{"sessionId":sid,"runId":rid,"threadId":thread,"lease":lease}}));
                self.callers.remove(&id);
                self.effect(sid, Some(rid), Purpose::Turn, "sendTurn", params)?;
            }
            Purpose::Revoke => {
                let rid = p.run.as_deref().ok_or("missing run")?;
                let authority = self.authority.apply("snapshot", json!({"runId":rid}))?;
                if result["revokedEpoch"].is_u64() && result["revokedEpoch"] != authority["epoch"] {
                    return Err(
                        "runtime revocation epoch does not match execution authority".into(),
                    );
                }
                let run = self.runs.get_mut(rid).ok_or("unknown run")?;
                run.state.apply("epoch", &authority["epoch"])?;
                let turn = run.state.apply("snapshot", &Value::Null)?["turn"]["id"].clone();
                self.record(rid,json!({"method":"engine/run/execution-revoked","params":{"revokedEpoch":authority["epoch"]}}))?;
                self.effect(
                    sid,
                    Some(rid),
                    Purpose::Interrupt,
                    "interruptTurn",
                    json!({"runId":rid,"turnId":turn}),
                )?;
            }
            Purpose::Interrupt => {
                let rid = p.run.as_deref().ok_or("missing run")?;
                self.runs.get_mut(rid).ok_or("unknown run")?.interrupted = result == true;
                self.authority.apply("drained", json!({"runId":rid}))?;
                self.effect(
                    sid,
                    Some(rid),
                    Purpose::Reconcile,
                    "reconcileRun",
                    json!({"runId":rid}),
                )?;
            }
            Purpose::Reconcile => {
                let rid = p.run.as_deref().ok_or("missing run")?;
                self.authority.apply("reconciled", json!({"runId":rid}))?;
                let unknown = result["outcome"] == "outcome-unknown"
                    || result["unresolvedOperations"]
                        .as_array()
                        .is_some_and(|v| !v.is_empty());
                let status = if unknown {
                    "outcome-unknown"
                } else {
                    "cancelled"
                };
                self.terminalize(rid, status, None)?;
                let run = self.runs.get_mut(rid).ok_or("unknown run")?;
                let snapshot = run.state.apply("snapshot", &Value::Null)?;
                let result = json!({"runId":rid,"status":status,"revoked":true,"revokedEpoch":snapshot["epoch"],"interrupted":run.interrupted,"reconciliationRequired":unknown});
                let callers = std::mem::take(&mut run.cancels);
                for id in callers {
                    self.resolve(id, result.clone());
                }
            }
            Purpose::Close => {
                let session = self.sessions.get_mut(sid).ok_or("unknown session")?;
                session.runtime = false;
                session.visible = false;
                session.closing_runtime = false;
                session.state.apply("closed", &Value::Null)?;
                let start = session.start;
                let failure = session.failure.take();
                let closes = std::mem::take(&mut session.closes);
                if let Some(error) = failure {
                    self.reject(start, &error);
                }
                for id in closes {
                    self.resolve(id, Value::Null);
                }
            }
            Purpose::Turn => unreachable!(),
        }
        Ok(())
    }
    fn turn_result(&mut self, p: &Pending, result: &Value, error: &Value) -> Result<()> {
        let rid = p.run.as_deref().ok_or("missing run")?;
        let Some(run) = self.runs.get_mut(rid) else {
            return Ok(());
        };
        if error.is_null() {
            run.completion = Some(result.clone());
            run.state.apply("turn", &result["turn"])?;
        } else {
            run.completion_error = Some(error.clone());
        }
        let state = run.state.apply("snapshot", &Value::Null)?;
        if state["terminal"] == true {
            return self.deliver_completion(rid);
        }
        if state["state"] == "cancelling" {
            return Ok(());
        }
        if result["turn"]["status"] == "interrupted" || error["code"] == "TASK_CANCELLED" {
            let authority=self.authority.apply("revoke",json!({"runId":rid,"leaseId":run.lease["leaseId"],"expectedEpoch":state["epoch"],"reason":"runtime-interrupted"}))?;
            run.state.apply("epoch", &authority["epoch"])?;
            self.authority.apply("drained", json!({"runId":rid}))?;
            self.authority.apply("reconciled", json!({"runId":rid}))?;
            self.terminalize(rid, "cancelled", None)
        } else {
            self.terminalize(
                rid,
                if error.is_null() {
                    "completed"
                } else {
                    "failed"
                },
                error["message"].as_str(),
            )
        }
    }
    fn fail_callback(&mut self, p: &Pending, error: &str) -> Result<()> {
        if let Some(rid) = &p.run {
            let Some(run) = self.runs.get_mut(rid) else {
                return Ok(());
            };
            let snapshot = run.state.apply("snapshot", &Value::Null)?;
            if matches!(
                p.purpose,
                Purpose::Revoke | Purpose::Interrupt | Purpose::Reconcile
            ) {
                run.state.apply("cancelFailed", &Value::Null)?;
                let callers = std::mem::take(&mut run.cancels);
                let session = self.sessions.get_mut(&p.session).ok_or("unknown session")?;
                session.state.apply("block", &Value::Null)?;
                let closes = std::mem::take(&mut session.closes);
                for id in closes {
                    self.reject(id, error);
                }
                for id in callers {
                    self.reject(id, error);
                }
                self.record(rid,json!({"method":"engine/run/cancellation-failed","params":{"message":error.chars().take(500).collect::<String>()}}))?;
            } else {
                if let Some(id) = run.start.take() {
                    self.reject(id, error);
                }
                if snapshot["state"] != "cancelling"
                    && snapshot["state"] != "cancellation-failed"
                    && snapshot["terminal"] != true
                {
                    self.terminalize(rid, "failed", Some(error))?;
                } else if matches!(p.purpose, Purpose::Turn) {
                    self.effects
                        .push(json!({"kind":"completion","runId":rid,"error":{"message":error}}));
                }
            }
        } else {
            let session = self.sessions.get_mut(&p.session).ok_or("unknown session")?;
            if matches!(p.purpose, Purpose::Close) {
                session.closing_runtime = false;
                let start = session.start;
                let callers = std::mem::take(&mut session.closes);
                self.reject(start, error);
                for id in callers {
                    self.reject(id, error);
                }
                return Ok(());
            }
            session.failure = Some(error.into());
            session.state.apply("fail", &Value::Null)?;
            if matches!(p.purpose, Purpose::Factory) {
                session.runtime = true;
            }
            self.close_ready(&p.session)?;
            if !self.sessions[&p.session].runtime {
                let id = self.sessions[&p.session].start;
                self.reject(id, error);
            }
        }
        Ok(())
    }
}
