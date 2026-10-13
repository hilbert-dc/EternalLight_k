use super::*;

impl AppClient {
    pub(super) fn tick(&mut self) -> Result<()> {
        let expired = self.rpc.apply("expire", &json!({"now":self.now}))?;
        for response in expired.as_array().into_iter().flatten() {
            self.response(response)?;
        }
        if let Some(turn) = &mut self.turn
            && turn.deadline.is_some_and(|deadline| self.now >= deadline)
            && self.callers.contains(&turn.id)
        {
            turn.state.cancel();
            let id = turn.id;
            self.reject(id, "Codex App Server turn timed out", "TURN_TIMEOUT".into());
            self.interrupt()?;
        }
        Ok(())
    }
    pub(super) fn transport(&mut self, event: &Value) {
        if matches!(
            self.phase,
            Phase::Dormant | Phase::Closing | Phase::Retiring
        ) {
            return;
        }
        let result = match event["kind"].as_str() {
            Some("message") => self.message(&event["value"]),
            Some("stderr") => {
                self.effects.push(json!({"kind":"event","message":{"method":"engine/stderr","params":{"text":event["text"]}}}));
                Ok(())
            }
            Some("error") => Err(event["message"]
                .as_str()
                .unwrap_or("App Server transport failed")
                .into()),
            Some("exit") => Err(format!(
                "Codex App Server exited ({})",
                event["code"]
                    .as_i64()
                    .map_or_else(|| "signal".into(), |v| v.to_string())
            )),
            _ => Err("invalid App Server transport event".into()),
        };
        if let Err(error) = result {
            self.retire(&error);
        }
    }
    fn message(&mut self, message: &Value) -> Result<()> {
        if message.get("id").is_some() && message["method"].is_string() {
            self.effects.push(json!({"kind":"event","message":message}));
            self.effects
                .push(json!({"kind":"hostRequest","generation":self.generation,"message":message}));
            return Ok(());
        }
        let response = self.rpc.apply(
            "response",
            &json!({"generation":self.generation,"message":message}),
        )?;
        if !response.is_null() {
            self.response(&response)?;
        }
        if let Some(turn) = &mut self.turn {
            let observed = turn.state.observe(self.generation, message)?;
            if !observed["completion"].is_null() {
                let id = turn.id;
                self.turn = None;
                self.interrupting = false;
                for (waiter, result) in std::mem::take(&mut self.interrupts) {
                    self.resolve(waiter, result);
                }
                if let Some(error) = observed["completion"]["error"].as_str() {
                    self.reject(id, error, Value::Null);
                } else {
                    self.resolve(id, observed["completion"]["result"].clone());
                }
            }
            if observed["interrupt"] == true {
                self.interrupt()?;
            }
        }
        self.effects.push(json!({"kind":"event","message":message}));
        Ok(())
    }
    fn response(&mut self, response: &Value) -> Result<()> {
        let Some(purpose) = response["id"]
            .as_u64()
            .and_then(|id| self.requests.remove(&id))
        else {
            return Ok(());
        };
        if let Some(error) = response["error"].as_str() {
            match purpose {
                Rpc::Initialize => self.retire(error),
                Rpc::ThreadStart => {
                    self.creating = false;
                    for id in std::mem::take(&mut self.sessions) {
                        self.reject(id, error, Value::Null);
                    }
                    if let Some(turn) = self.turn.take() {
                        self.reject(turn.id, error, Value::Null);
                    }
                }
                Rpc::Resume(id, _) => {
                    self.resuming = false;
                    self.reject(id, error, Value::Null);
                    self.advance()?;
                }
                Rpc::TurnStart(id) => {
                    if self.turn.as_ref().is_some_and(|t| t.id == id) {
                        self.turn = None;
                        self.reject(id, error, Value::Null);
                    }
                }
                Rpc::Steer(id) if response["rpcCode"] == -32600 => self.resolve(
                    id,
                    json!({"steered":false,"delivery":"notSent","reason":"turnRejected"}),
                ),
                Rpc::Steer(id) => self.reject(id, error, Value::Null),
                Rpc::Name => {}
                Rpc::Interrupt(turn_id) => {
                    if self.turn.as_ref().is_some_and(|t| t.id == turn_id) {
                        self.interrupting = false;
                        for (id, _) in std::mem::take(&mut self.interrupts) {
                            self.reject(id, error, Value::Null);
                        }
                    }
                }
            }
            return Ok(());
        }
        let result = &response["result"];
        match purpose {
            Rpc::Initialize => {
                self.write(json!({"method":"initialized","params":{}}));
                self.phase = Phase::Ready;
                self.advance()?;
            }
            Rpc::ThreadStart => {
                self.creating = false;
                self.thread = Some(
                    result["thread"]["id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or("Codex App Server did not return a thread id")?
                        .into(),
                );
                self.advance()?;
            }
            Rpc::Resume(id, fallback) => {
                self.resuming = false;
                self.thread = Some(
                    result["thread"]["id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&fallback)
                        .into(),
                );
                self.resolve(id, json!(self.thread));
                self.advance()?;
            }
            Rpc::TurnStart(id) => {
                if let Some(turn) = &mut self.turn
                    && turn.id == id
                {
                    let interrupt = turn
                        .state
                        .bind_started(result["turn"]["id"].as_str().map(str::to_owned))?;
                    let name = turn.options["threadName"]
                        .as_str()
                        .unwrap_or("")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let rename = turn.created_thread
                        && !name.is_empty()
                        && turn.state.snapshot()["cancellationRequested"] != true;
                    if interrupt {
                        self.interrupt()?;
                    }
                    if rename {
                        self.request("thread/name/set",json!({"threadId":self.thread,"name":name.chars().take(240).collect::<String>()}),Rpc::Name)?;
                    }
                }
            }
            Rpc::Steer(id) => self.resolve(id, json!({"steered":true})),
            Rpc::Name => {}
            Rpc::Interrupt(turn_id) => {
                if self.turn.as_ref().is_some_and(|t| t.id == turn_id) {
                    for (id, result) in std::mem::take(&mut self.interrupts) {
                        self.resolve(id, result);
                    }
                }
            }
        }
        Ok(())
    }
    fn retire(&mut self, error: &str) {
        if matches!(self.phase, Phase::Closing | Phase::Retiring) {
            return;
        }
        self.phase = Phase::Retiring;
        self.retirement_error = Some(error.into());
        self.effects.push(
            json!({"kind":"event","message":{"method":"engine/error","params":{"message":error}}}),
        );
        self.effects
            .push(json!({"kind":"retire","generation":self.generation,"timeoutMs":1000}));
    }
    pub(super) fn fail_callers(&mut self, error: &str) {
        let callers: Vec<_> = self
            .callers
            .iter()
            .filter(|id| !self.closes.contains(id))
            .copied()
            .collect();
        for id in callers {
            self.reject(id, error, Value::Null);
        }
        self.rpc
            .apply("clear", &json!({}))
            .expect("clear is infallible");
        self.requests.clear();
        self.sessions.clear();
        self.resume = None;
        self.resuming = false;
        self.thread = None;
        self.creating = false;
        self.turn = None;
        self.interrupting = false;
        self.interrupts.clear();
    }
    pub(super) fn retired(&mut self) {
        if let Some(error) = self.retirement_error.take() {
            self.fail_callers(&error);
        }
        self.phase = Phase::Dormant;
        for id in std::mem::take(&mut self.closes) {
            self.resolve(id, Value::Null);
        }
    }
}
