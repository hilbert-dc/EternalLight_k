use super::*;

impl BridgeSupervisor {
    pub(super) fn callback(&mut self, input: &Value) -> Result<()> {
        let Some(purpose) = input["id"]
            .as_u64()
            .and_then(|id| self.callbacks.remove(&id))
        else {
            return Ok(());
        };
        let error = input["error"].as_str();
        let result = &input["result"];
        match purpose {
            Callback::Credentials => {
                if let Some(error) = error {
                    self.phase = Phase::Stopped;
                    for id in std::mem::take(&mut self.starts) {
                        self.reject(id, error);
                    }
                    self.closed();
                } else {
                    self.generation = self
                        .generation
                        .checked_add(1)
                        .ok_or("bridge generation exhausted")?;
                    self.phase = Phase::Starting;
                    self.effects
                        .push(json!({"kind":"spawn","generation":self.generation}));
                    let mut params = self.options.clone();
                    params["credentials"] = result.clone();
                    if self.address["token"].is_string() {
                        params["token"] = self.address["token"].clone();
                    }
                    if let Some(url) = self.address["baseUrl"].as_str() {
                        params["port"] = url::Url::parse(url)
                            .ok()
                            .and_then(|url| url.port())
                            .map(u64::from)
                            .map(Value::from)
                            .unwrap_or(Value::Null);
                    }
                    self.request("start", params, Request::Start)?;
                }
            }
            Callback::Host(request) => {
                if request["generation"] == self.generation
                    && matches!(self.phase, Phase::Running | Phase::Closing)
                {
                    let message = if let Some(error) = error {
                        json!({"kind":"host-response","requestId":request["requestId"],"error":{"message":error}})
                    } else {
                        json!({"kind":"host-response","requestId":request["requestId"],"result":result})
                    };
                    self.write(message);
                }
            }
            Callback::Recovery(budget_id) => {
                if self.budget["id"] != budget_id || !self.invalidated {
                    return Ok(());
                }
                self.recovering = false;
                self.recovery_error = input["error"].clone();
                for id in std::mem::take(&mut self.recovery_waiters) {
                    self.effect(
                        "recoverBudget",
                        json!({"budgetId":budget_id}),
                        Callback::RetryRecovery(id, budget_id.clone()),
                    )?;
                }
            }
            Callback::RetryRecovery(id, budget_id) => {
                if self.budget["id"] != budget_id {
                    self.reject(id, "bridge task budget changed during recovery");
                } else if let Some(error) = error {
                    self.reject(id, error);
                } else {
                    self.effect(
                        "completeBudget",
                        json!({"budgetId":budget_id,"outcome":"recovered_unknown"}),
                        Callback::CompleteBudget(id, budget_id, false.into()),
                    )?;
                }
            }
            Callback::CompleteBudget(id, budget_id, ended) => {
                if self.budget["id"] != budget_id {
                    self.reject(id, "bridge task budget changed during completion");
                } else if let Some(error) = error {
                    self.reject(id, error);
                } else {
                    self.budget = Value::Null;
                    self.invalidated = false;
                    self.recovery_error = Value::Null;
                    if self.phase == Phase::Retiring {
                        self.retired_completions.push((id, ended));
                    } else {
                        self.resolve(id, ended);
                    }
                }
            }
            Callback::CloseTransport => {
                if let Some(error) = error {
                    self.retirement_error = Some(error.into());
                }
                self.pid = Value::Null;
                self.phase = Phase::Stopped;
                if let Some(error) = self.retirement_error.take() {
                    for id in std::mem::take(&mut self.starts) {
                        self.reject(id, &error);
                    }
                    for id in std::mem::take(&mut self.closes) {
                        self.reject(id, &error);
                    }
                }
                self.closed();
                for (id, result) in std::mem::take(&mut self.retired_completions) {
                    self.resolve(id, result);
                }
            }
        }
        Ok(())
    }
    pub(super) fn tick(&mut self) -> Result<()> {
        let expired = self.rpc.apply("expire", &json!({"now":self.now}))?;
        for response in expired.as_array().into_iter().flatten() {
            self.response(response)?;
        }
        if self.phase == Phase::Running
            && self.options["heartbeatIntervalMs"] != 0
            && !self.heartbeat_pending
            && self.now >= self.next_heartbeat
        {
            self.heartbeat_pending = true;
            self.request("ping", json!({}), Request::Heartbeat)?;
        }
        Ok(())
    }
    pub(super) fn transport(&mut self, event: &Value) {
        if matches!(self.phase, Phase::Stopped | Phase::Retiring) {
            return;
        }
        let result = match event["kind"].as_str() {
            Some("message") => self.message(&event["value"]),
            Some("stderr") => Ok(()),
            Some("exit") => Err(format!("bridge process exited ({})", event["code"])),
            Some("error") => Err(event["message"]
                .as_str()
                .unwrap_or("bridge transport failed")
                .into()),
            _ => Err("invalid bridge transport event".into()),
        };
        if let Err(error) = result {
            let _ = self.retire(&error);
        }
    }
    fn message(&mut self, message: &Value) -> Result<()> {
        if message["kind"] == "host-request" {
            if !matches!(self.phase, Phase::Running | Phase::Closing) {
                return Ok(());
            }
            let method = message["method"].as_str().unwrap_or("");
            let mut params = message["params"].clone();
            let callback = match method {
                "request:observed" => "request",
                "response:observed" => "response",
                "qualification:failure" => {
                    self.lease = Value::Null;
                    "qualification"
                }
                "budget:reserve" | "budget:settle" | "budget:observe" => {
                    if self.budget.is_null() {
                        self.write(json!({"kind":"host-response","requestId":message["requestId"],"error":{"message":"host task budget is not active"}}));
                        return Ok(());
                    }
                    params["budgetId"] = self.budget["id"].clone();
                    method
                }
                _ => {
                    self.write(json!({"kind":"host-response","requestId":message["requestId"],"error":{"message":"unknown host budget method"}}));
                    return Ok(());
                }
            };
            return self.effect(
                callback,
                params,
                Callback::Host(
                    json!({"requestId":message["requestId"],"generation":self.generation}),
                ),
            );
        }
        let response = self.rpc.apply(
            "response",
            &json!({"generation":self.generation,"message":message}),
        )?;
        if !response.is_null() {
            self.response(&response)?;
        }
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
                Request::Start => {
                    return self.retire(&error.replace(
                        "Codex App Server request timed out: start",
                        "bridge start timed out",
                    ));
                }
                Request::Close => {
                    return self.retire(&error.replace(
                        "Codex App Server request timed out: close",
                        "bridge close timed out",
                    ));
                }
                Request::Heartbeat => return self.retire(error),
                Request::Command(id, method) => {
                    if method == "budget:start" {
                        self.budget = Value::Null;
                    } else if method == "lease:start" || method == "lease:end" {
                        self.lease = Value::Null;
                    }
                    self.reject(id, error);
                }
            }
            return Ok(());
        }
        let result = &response["result"];
        match purpose {
            Request::Start => {
                let url = result["baseUrl"]
                    .as_str()
                    .and_then(|s| url::Url::parse(s).ok())
                    .ok_or("invalid bridge listening address")?;
                if url.host_str() != Some("127.0.0.1")
                    || url.scheme() != "http"
                    || url.port().is_none()
                {
                    return Err("invalid bridge listening address".into());
                }
                self.address = result.clone();
                self.phase = Phase::Running;
                self.next_heartbeat =
                    self.now + self.options["heartbeatIntervalMs"].as_u64().unwrap_or(5000);
                for id in std::mem::take(&mut self.starts) {
                    self.resolve(id, result.clone());
                }
                if !self.closes.is_empty() {
                    self.phase = Phase::Closing;
                    self.request("close", json!({}), Request::Close)?;
                }
            }
            Request::Close => {
                self.phase = Phase::Retiring;
                self.effect("retire", json!({}), Callback::CloseTransport)?;
            }
            Request::Heartbeat => {
                self.heartbeat_pending = false;
                self.next_heartbeat =
                    self.now + self.options["heartbeatIntervalMs"].as_u64().unwrap_or(5000);
            }
            Request::Command(id, method) => match method.as_str() {
                "budget:end" => {
                    self.effect(
                        "completeBudget",
                        json!({"budgetId":self.budget["id"],"outcome":"terminal"}),
                        Callback::CompleteBudget(
                            id,
                            self.budget["id"].clone(),
                            result["ended"].clone(),
                        ),
                    )?;
                }
                "lease:end" => {
                    self.lease = Value::Null;
                    self.resolve(id, result["ended"].clone());
                }
                "budget:start" => self.resolve(id, self.budget.clone()),
                "lease:start" => self.resolve(id, self.lease.clone()),
                "cancel" => self.resolve(id, result.clone()),
                _ => return Err("invalid bridge command completion".into()),
            },
        }
        Ok(())
    }
    fn retire(&mut self, error: &str) -> Result<()> {
        if self.phase == Phase::Retiring {
            return Ok(());
        }
        self.phase = Phase::Retiring;
        self.pid = Value::Null;
        self.heartbeat_pending = false;
        self.retirement_error = Some(error.into());
        if !self.budget.is_null() && !self.invalidated {
            self.invalidated = true;
            self.recovering = true;
            self.effect(
                "recoverBudget",
                json!({"budgetId":self.budget["id"]}),
                Callback::Recovery(self.budget["id"].clone()),
            )?;
        }
        self.rpc.apply("clear", &json!({}))?;
        for (_, purpose) in std::mem::take(&mut self.requests) {
            if let Request::Command(id, _) = purpose {
                self.reject(id, error);
            }
        }
        self.effect("retire", json!({}), Callback::CloseTransport)
    }
    pub(super) fn closed(&mut self) {
        if !self.closes.is_empty() {
            self.budget = Value::Null;
            self.lease = Value::Null;
            self.invalidated = false;
            self.recovering = false;
            for id in std::mem::take(&mut self.recovery_waiters) {
                self.reject(id, "bridge process closed");
            }
        }
        for id in std::mem::take(&mut self.closes) {
            self.resolve(id, Value::Null);
        }
    }
}
