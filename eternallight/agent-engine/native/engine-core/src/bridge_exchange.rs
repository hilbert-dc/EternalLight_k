use super::{
    Control, Shared,
    failure::{Failure, Outcome, status},
    lock, observation, output_failure,
};
use crate::{
    ChatStream, HttpEvent, HttpOptions, HttpTransport, ResponsesStream, SseDecoder, canonical,
    local_http::LocalRequest, portable, responses, responses_request,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

enum Decoder {
    Chat(ChatStream),
    Responses(ResponsesStream),
}
impl Decoder {
    fn push(&mut self, frame: Value) -> crate::Result<Vec<Value>> {
        match self {
            Self::Chat(stream) => stream.push(frame),
            Self::Responses(stream) => {
                if frame["done"] == true {
                    Ok(Vec::new())
                } else {
                    stream.push(frame["data"].clone())
                }
            }
        }
    }
    fn reported_usage(&self) -> Option<Value> {
        match self {
            Self::Chat(stream) => output_failure::chat_usage(stream.reported_usage()),
            Self::Responses(_) => None,
        }
    }
    fn finish(&mut self) -> crate::Result<Vec<Value>> {
        match self {
            Self::Chat(stream) => stream.finish(),
            Self::Responses(stream) => stream.finish(),
        }
    }
}
struct Exchange<'a> {
    shared: &'a Shared,
    control: &'a Control,
    id: u64,
    headers_sent: bool,
    stream_requested: bool,
    budget: Value,
    lease: Value,
    route: Value,
    identity: Value,
    authority_scope: String,
    discovery_catalog: Value,
    context: Value,
    reservation: Value,
    settled: bool,
    downstream_started: bool,
    chat_finish_seen: bool,
    reported_usage: Option<Value>,
    observed_usage: Value,
}

pub(super) fn run(shared: &Shared, request: LocalRequest, control: &Control) {
    if request.url.starts_with("/_engine/mcp/") {
        authority_request(shared, &request);
        return;
    }
    let mut exchange = Exchange {
        shared,
        control,
        id: request.id,
        headers_sent: false,
        stream_requested: false,
        budget: Value::Null,
        lease: Value::Null,
        route: Value::Null,
        identity: Value::Null,
        authority_scope: String::new(),
        discovery_catalog: Value::Null,
        context: Value::Null,
        reservation: Value::Null,
        settled: false,
        downstream_started: false,
        chat_finish_seen: false,
        reported_usage: None,
        observed_usage: Value::Null,
    };
    if let Err(error) = exchange.execute(&request) {
        lock(&shared.scheduler).feedback(request.id, 500);
        exchange.failure(error);
    }
    if !exchange.reservation.is_null()&&!exchange.settled
        && let Err(error)=shared.call("settle",json!({"budgetId":exchange.budget["id"],"id":exchange.reservation,"usage":exchange.reported_usage.as_ref().unwrap_or(&Value::Null),"options":{"outcome":if exchange.downstream_started{"unknown"}else{"not-sent"}}}),None) {
        *lock(&shared.settlement_failure)=Some(error);shared.cancel();
    }
    let _ = shared.call("releaseFetch", json!({"requestId":request.id}), None);
    lock(&shared.scheduler).release(request.id);
}

// These routes share the listener's authentication and bounded request body.
fn authority_request(shared: &Shared, request: &LocalRequest) {
    let result = (|| -> crate::Result<Value> {
        if shared.options["mcpAuthority"] != true { return Err("MCP authority is disabled".into()); }
        let input: Value = serde_json::from_str(&request.body).map_err(|_| "invalid MCP authority body")?;
        if request.url == "/_engine/mcp/catalog" { return lock(&shared.authority).register(&input); }
        if request.url != "/_engine/mcp/call" { return Err("unknown MCP authority route".into()); }
        let state = lock(&shared.execution);
        shared.validate(&state)?;
        lock(&shared.authority).resolve(&input, &state.lease)
    })();
    let (code, value) = match result { Ok(v) => (200,v), Err(e) => (403,json!({"error":e})) };
    let mut server = lock(&shared.server);
    if server.head(request.id, code, HashMap::from([("content-type".into(),"application/json".into())])).is_ok()
        && server.write(request.id, value.to_string().as_bytes()).unwrap_or(false) { server.end(request.id); }
    else { server.abort(request.id); }
}

impl Exchange<'_> {
    fn check(&self) -> Outcome<()> {
        self.shared
            .cancelled(self.control, self.id)
            .map_err(Failure::from)?;
        if !self.lease.is_null() {
            self.shared.validate(&lock(&self.shared.execution))
                .map_err(|error| status(error, 403))?;
        }
        Ok(())
    }
    fn call(&self, method: &str, params: Value) -> Outcome<Value> {
        self.shared
            .call(method, params, Some((self.control, self.id)))
            .map_err(Into::into)
    }
    fn observe(&self, method: &str, payload: Value) {
        let _ = self
            .shared
            .call(method, payload, Some((self.control, self.id)));
    }
    fn head(&mut self, status: u16, content_type: &str) -> Outcome<()> {
        lock(&self.shared.server).head(
            self.id,
            status,
            HashMap::from([
                ("content-type".into(), content_type.into()),
                ("cache-control".into(), "no-store".into()),
            ]),
        )?;
        self.headers_sent = true;
        Ok(())
    }
    fn write(&self, bytes: &[u8]) -> Outcome<()> {
        let deadline = Instant::now() + Duration::from_millis(
            self.shared.options["requestTimeoutMs"].as_u64().unwrap_or(120_000));
        loop {
            if Instant::now() >= deadline { return Err("bridge response consumer timed out".into()); }
            self.check()?;
            if lock(&self.shared.server).write(self.id, bytes)? {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn record_mcp_call(&self, item: &Value) -> Outcome<()> {
        if self.shared.options["mcpAuthority"] != true { return Ok(()); }
        self.check()?;
        let state = lock(&self.shared.execution);
        self.shared.validate(&state).map_err(|e| status(e, 403))?;
        if state.lease["leaseId"] != self.lease["leaseId"] || state.lease["epoch"] != self.lease["epoch"] {
            return Err(status("MCP response execution lease changed", 403));
        }
        lock(&self.shared.authority).record(&self.authority_scope, &self.lease, &self.identity,
            self.context["requestId"].as_str().ok_or("MCP sampling request identity missing")?, item)
            .map_err(|e| status(e, 403))
    }
    fn event(&self, event: &Value) -> Outcome<()> {
        if event["type"] == "response.output_item.done" { self.record_mcp_call(&event["item"])?; }
        if event["type"] == "response.completed" {
            self.observe_discovery(&event["response"])?;
        }
        self.write(
            format!(
                "event: {}\ndata: {event}\n\n",
                canonical::string(&event["type"])
            )
            .as_bytes(),
        )
    }
    fn observe_discovery(&self, response: &Value) -> Outcome<()> {
        self.check()?;
        let mut state = lock(&self.shared.execution);
        let config = state.plan["toolDiscovery"].clone();
        if config.is_null() {
            return Ok(());
        }
        // An old in-flight exchange cannot register receipts in a new execution.
        if state.lease["leaseId"] != self.lease["leaseId"] || state.lease.is_null() {
            return Err(status("discovery response execution lease changed", 403));
        }
        state.policy.apply("observeDiscovery", &json!({"response":response,"toolDiscovery":config,"identity":self.identity,"catalog":self.discovery_catalog}))
            .map_err(|e|status(e,403))?;
        Ok(())
    }
    fn settle(&mut self, usage: &Value) -> Outcome<()> {
        if !usage.is_null() { self.observed_usage = usage.clone(); }
        if !self.reservation.is_null() && !self.settled {
            self.shared.call(
                "settle",
                json!({"budgetId":self.budget["id"],"id":self.reservation,"usage":usage}),
                None,
            )?;
            self.settled = true;
        }
        Ok(())
    }
    fn chat_usage_candidate(&self, frame: &Value) -> (bool, Option<Value>) {
        if self.route["protocol"] != "chat-completions" {
            return (self.chat_finish_seen, None);
        }
        let current_finish = frame["data"]["choices"][0]["finish_reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty());
        let finished = self.chat_finish_seen || current_finish;
        let usage_only = frame["data"]["choices"]
            .as_array()
            .is_some_and(Vec::is_empty);
        let usage = &frame["data"]["usage"];
        // Only raw final counts are evidence on an interrupted stream. Do not
        // let normal codec normalization invent a valid total from invalid data.
        let reported = (current_finish || self.chat_finish_seen && usage_only)
            .then(|| {
                output_failure::reported_usage(&json!({
                    "total_tokens": usage["total_tokens"],
                    "input_tokens": usage["prompt_tokens"],
                    "output_tokens": usage["completion_tokens"],
                }))
            })
            .flatten();
        (finished, reported)
    }
    fn barrier(&self, observation: &Value) -> Outcome<()> {
        let policy = &self.shared.options["responseBarrier"];
        if policy.is_null()
            || !self.identity["parentThreadId"].is_null()
            || !canonical::array(&observation["toolNames"]).contains(&policy["rootTool"])
        {
            return Ok(());
        }
        let file = Path::new(
            policy["releaseFile"]
                .as_str()
                .ok_or("invalid response barrier path")?,
        );
        let deadline = Instant::now()
            + Duration::from_millis(
                policy["timeoutMs"]
                    .as_u64()
                    .unwrap_or(10_000)
                    .clamp(1, 120_000),
            );
        while Instant::now() < deadline {
            self.check()?;
            if file.exists() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(status(
            format!(
                "tool response barrier timed out: {}",
                canonical::string(&policy["rootTool"])
            ),
            504,
        ))
    }
    fn execute(&mut self, request: &LocalRequest) -> Outcome<()> {
        self.check()?;
        let mut body: Value = serde_json::from_str(&request.body)
            .map_err(|_| status("request body is not valid JSON", 400))?;
        self.stream_requested = body["stream"] != false;
        {
            let mut state = lock(&self.shared.execution);
            self.budget = state.budget.clone();
            self.lease = state.lease.clone();
            if self.budget.is_null()
                || self.lease.is_null()
                || self.budget["id"] != self.lease["budgetId"]
            {
                return Err(status(
                    "an active execution lease and matching task budget are required",
                    403,
                ));
            }
            self.shared.validate(&state).map_err(|e| status(e, 403))?;
            let plan = state.plan.clone();
            self.route = state
                .policy
                .apply("route", &json!({"body":body,"sessionPlan":plan}))
                .map_err(|e| status(e, 403))?;
            self.identity = state
                .policy
                .apply(
                    "identity",
                    &json!(request.headers.get("x-codex-turn-metadata")),
                )
                .map_err(|e| status(e, 403))?;
            state.lineage.as_mut().ok_or("missing route lineage")?.authorize(&json!({"routeId":self.route["connectionId"],"threadId":self.identity["threadId"],"parentThreadId":self.identity["parentThreadId"]})).map_err(|e|status(e,403))?;
        }
        // Scope automatic HTTP retries to the same trusted Core operation. Hash
        // before policy mutation, which can change as discovery is observed.
        let request_identity = logical_request_identity(&self.lease, &self.identity, &self.route, &body);
        let connection = canonical::array(&self.shared.options["connections"])
            .iter()
            .find(|c| c["id"] == self.route["connectionId"])
            .ok_or_else(|| {
                status(
                    format!(
                        "connection is unavailable: {}",
                        canonical::string(&self.route["connectionId"])
                    ),
                    503,
                )
            })?;
        let key = self.call("credential", connection["credentialRef"].clone())?;
        let key = key.as_str().filter(|s| !s.is_empty()).ok_or_else(|| {
            status(
                format!(
                    "credential is unavailable: {}",
                    canonical::string(&connection["credentialRef"])
                ),
                503,
            )
        })?;
        let protocol = canonical::string(&self.route["protocol"]).to_owned();
        // Trusted Host lookup, snapshotted once per request. Restore registered
        // MCP schemas before discovery/admission; never change the route or budget.
        if self.shared.options["hasRequestConfigurationProvider"] == true {
            let selection = self.call("requestConfiguration", json!({
                "connectionId": self.route["connectionId"],
                "runId": self.lease["runId"], "threadId": self.identity["threadId"]
            }))?;
            if !selection.is_null() {
                let fields = selection.as_object().ok_or_else(|| status("request configuration must be an object or null", 400))?;
                if fields.keys().any(|key| !matches!(key.as_str(), "reasoningEffort" | "mcpIdentity" | "mcpScope")) {
                    return Err(status("request configuration contains unsupported fields", 400));
                }
                if self.shared.options["mcpAuthority"] == true {
                    if selection["mcpIdentity"]["threadId"] != self.identity["threadId"] {
                        return Err(status("MCP trusted thread identity mismatch", 403));
                    }
                    let role = &selection["mcpIdentity"]["agentRole"];
                    if !role.is_null() && role.as_str().is_none_or(|r| r.is_empty() || r.len() > 160 || r.chars().any(char::is_control)) {
                        return Err(status("MCP trusted execution role is invalid",403));
                    }
                    self.identity["agentRole"] = role.clone();
                    self.authority_scope = selection["mcpScope"].as_str().filter(|s| !s.is_empty()).ok_or("MCP scope is missing")?.to_owned();
                    lock(&self.shared.authority).prepare(&mut body, &self.authority_scope).map_err(|e|status(e,403))?;
                }
                if selection.get("reasoningEffort").is_some() {
                let effort = selection["reasoningEffort"].as_str().ok_or_else(|| status("request reasoning effort must be a string", 400))?;
                if protocol != "responses" || !canonical::array(&connection["protocolOptions"]["allowedReasoningEfforts"]).iter().any(|v| v == effort) {
                    return Err(status("request reasoning effort is outside the connection's allowed set", 400));
                }
                if body["reasoning"].is_null() { body["reasoning"] = json!({}); }
                let reasoning = body["reasoning"].as_object_mut().ok_or_else(|| status("reasoning must be an object", 400))?;
                reasoning.insert("effort".into(), effort.into());
                }
            }
        }
        if self.shared.options["mcpAuthority"] == true && self.authority_scope.is_empty() { return Err(status("MCP authority requires trusted Host identity", 403)); }
        let policy = &self.shared.options["toolPolicy"];
        let exists = |field: &str| {
            policy[field]
                .as_str()
                .is_some_and(|p| Path::new(p).exists())
        };
        {
            let mut state = lock(&self.shared.execution);
            let discovery = state.plan["toolDiscovery"].clone();
            body=state.policy.apply("prepare",&json!({"body":body,"outputBudget":self.route["outputBudget"],"identity":self.identity,"policy":policy,"toolDiscovery":discovery,"childComplete":exists("childCompletionFile"),"childPhaseReached":exists("childPhaseFile")})).map_err(|e|status(e,403))?;
            if !discovery.is_null() {
                self.discovery_catalog = state
                    .policy
                    .apply("discoverySnapshot", &json!({"identity":self.identity}))?;
            }
        }
        let (_, image_tokens) =
            crate::image_input::admission(&body["input"]).map_err(|e| status(e, 400))?;
        if image_tokens > 0
            && (protocol != "responses"
                || !canonical::array(&self.route["authorizedExtensions"])
                    .iter()
                    .any(|v| v == "imageInput"))
        {
            return Err(status(
                "image input is not verified and authorized for this route",
                403,
            ));
        }
        self.context = self.identity.clone();
        self.context.as_object_mut().ok_or("invalid route identity")?.extend(json!({"runId":self.lease["runId"],"requestId":format!("{}:{}", canonical::string(&self.lease["runId"]), self.id),"connectionId":self.route["connectionId"],"logicalModel":self.route["logicalModel"]}).as_object().unwrap().clone());
        let mut prepared = if protocol == "responses" {
            responses_request::encode(&body, connection, key)?
        } else {
            portable::encode(&protocol, &body, connection, key)?
        };
        // Private host policy, never accepted from model output or request metadata.
        prepared["context"]["executorValidatedTools"] =
            self.shared.options["executorValidatedTools"].clone();
        if prepared["context"]["requestId"].is_string() {
            self.context["requestId"] = prepared["context"]["requestId"].clone();
        }
        self.observe(
            "request",
            observation::request(&self.context, &body, &prepared),
        );
        let streaming = matches!(protocol.as_str(), "responses" | "chat-completions");
        if streaming
            && prepared["body"]["stream"] != false
            && !canonical::array(&self.route["enabledExtensions"])
                .iter()
                .any(|v| v == "streaming")
        {
            return Err(status(
                format!(
                    "streaming is not qualified for model route: {}",
                    canonical::string(&self.route["logicalModel"])
                ),
                403,
            ));
        }
        let estimated = prepared["context"]["estimatedInputTokens"]
            .as_u64()
            .ok_or_else(|| status("codec did not produce a bounded input estimate", 500))?;
        crate::context_capacity::check(estimated, self.route["inputBudget"].as_u64().unwrap_or(0), crate::context_capacity::Boundary::Route)?;
        // Reservation completion cannot be abandoned on cancellation: its id is
        // needed to refund a request that never reached the provider.
        let group = format!(
            "{}|{}",
            canonical::string(&connection["endpoint"]),
            canonical::string(&connection["credentialRef"])
        );
        let limits = &self.shared.resource_policy;
        let provider_limit = canonical::array(&self.shared.options["connections"])
            .iter()
            .filter(|c| {
                c["endpoint"] == connection["endpoint"]
                    && c["credentialRef"] == connection["credentialRef"]
            })
            .map(|c| {
                limits["connectionLimits"][canonical::string(&c["id"])]
                    .as_u64()
                    .unwrap_or(limits["maxRequestsPerProvider"].as_u64().unwrap())
            })
            .min()
            .unwrap_or(1) as usize;
        lock(&self.shared.scheduler)
            .enqueue(self.id, group, provider_limit)
            .map_err(|e| status(e, 429))?;
        while !lock(&self.shared.scheduler).admit(self.id) {
            self.check()?;
            std::thread::sleep(Duration::from_millis(10));
        }
        loop {
            self.check()?;
            let reservation = self.shared.call("reserve",json!({"budgetId":self.budget["id"],"estimatedInputTokens":estimated,"maxOutputTokens":body["max_output_tokens"],"deferIfBusy":true,"requestIdentity":request_identity}),None).map_err(Failure::reservation)?;
            if reservation["waiting"] == true {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            self.reservation = reservation;
            break;
        }
        self.check()?;
        self.shared
            .validate(&lock(&self.shared.execution))
            .map_err(|e| status(e, 403))?;
        self.downstream_started = true;
        let max = self.shared.options["maxDownstreamBytes"]
            .as_u64()
            .unwrap_or(32 * 1024 * 1024) as usize;
        let options = HttpOptions {
            url: canonical::string(&prepared["url"]).into(),
            method: "POST".into(),
            headers: serde_json::from_value(prepared["headers"].clone())
                .map_err(|_| "invalid provider headers")?,
            body: prepared["body"].to_string(),
            timeout_ms: None,
            idle_timeout_ms: self.shared.options["requestTimeoutMs"].as_u64(),
            max_bytes: max,
        };
        let mut native = if self.shared.options["customFetch"] == true {
            None
        } else {
            Some(HttpTransport::start(options)?)
        };
        let header = if let Some(native) = &mut native {
            self.next_native(native)?
        } else {
            let result=self.call("fetch",json!({"requestId":self.id,"url":prepared["url"],"headers":prepared["headers"],"body":prepared["body"].to_string(),"maxBytes":max,"timeoutMs":null,"idleTimeoutMs":self.shared.options["requestTimeoutMs"]}))?;
            HttpEvent::Headers {
                status: result["status"].as_u64().unwrap_or(502) as u16,
                headers: serde_json::from_value(result["headers"].clone())
                    .map_err(|_| "invalid provider headers")?,
            }
        };
        let HttpEvent::Headers {
            status: code,
            headers,
        } = header
        else {
            return Err("provider did not return response headers".into());
        };
        if !(200..300).contains(&code) {
            lock(&self.shared.scheduler).feedback(self.id, code);
        }
        if headers
            .get("content-length")
            .and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|size| size > max)
        {
            return Err("downstream response exceeds bridge limit".into());
        }
        let stream = body["stream"] != false
            && streaming
            && headers
                .get("content-type")
                .is_some_and(|s| s.contains("text/event-stream"))
            && (200..300).contains(&code);
        let mut decoder = if stream {
            Some(if protocol == "responses" {
                Decoder::Responses(ResponsesStream::new(prepared["context"].clone())?)
            } else {
                Decoder::Chat(ChatStream::new(prepared["context"].clone()))
            })
        } else {
            None
        };
        let mut sse = SseDecoder::new(8 * 1024 * 1024)?;
        let buffered = !self.shared.options["responseBarrier"].is_null();
        let mut frames = Vec::new();
        let mut bytes = Vec::new();
        let mut received = 0usize;
        if stream && !buffered {
            self.head(200, "text/event-stream")?;
        }
        loop {
            let event = self.next_provider_event(&mut native)?;
            match event {
                HttpEvent::Chunk(chunk) => {
                    received = received.saturating_add(chunk.len());
                    if received > max {
                        return Err("downstream response exceeds bridge limit".into());
                    }
                    if let Some(decoder) = &mut decoder {
                        let mut pending = sse.push(&chunk)?.into_iter();
                        while let Some(frame) = pending.next() {
                            if let Some(usage) = output_failure::terminal_usage(&frame).flatten() {
                                self.settle(&usage)?;
                            }
                            if let Some(failure) = Failure::stream(&frame["data"]) {
                                let data = &frame["data"];
                                let usage = data.get("response").unwrap_or(data).get("usage");
                                if let Some(usage) = usage.filter(|usage| !usage.is_null()) {
                                    self.settle(usage)?;
                                }
                                return Err(failure);
                            }
                            let (chat_finished, reported_usage) = self.chat_usage_candidate(&frame);
                            let events = match decoder.push(frame) {
                                Ok(events) => events,
                                Err(message) => {
                                    let failure = Failure::codec(message);
                                    if (failure.validation_code.is_some()
                                        || failure.item_limit_code() == Some("provider_output_item_limit"))
                                        && let Some(usage) = output_failure::drain_usage(
                                            pending.collect(),
                                            &mut sse,
                                            || self.next_provider_event(&mut native),
                                            max.saturating_sub(received),
                                        )
                                    {
                                        self.settle(&usage)?;
                                    }
                                    return Err(failure);
                                }
                            };
                            self.chat_finish_seen = chat_finished;
                            if let Some(usage) = reported_usage {
                                self.reported_usage = Some(usage);
                            }
                            for event in events {
                                if buffered {
                                    frames.push(event);
                                } else {
                                    self.event(&event)?;
                                }
                            }
                        }
                    } else {
                        bytes.extend(chunk);
                    }
                }
                HttpEvent::End => break,
                HttpEvent::Error(error) => return Err(error.into()),
                HttpEvent::Headers { .. } => {
                    return Err("duplicate provider response headers".into());
                }
            }
        }
        if let Some(decoder) = &mut decoder {
            for frame in sse.finish()? {
                if let Some(usage) = output_failure::terminal_usage(&frame).flatten() {
                    self.settle(&usage)?;
                }
                if let Some(failure) = Failure::stream(&frame["data"]) {
                    let data = &frame["data"];
                    let usage = data.get("response").unwrap_or(data).get("usage");
                    if let Some(usage) = usage.filter(|usage| !usage.is_null()) {
                        self.settle(usage)?;
                    }
                    return Err(failure);
                }
                let (chat_finished, reported_usage) = self.chat_usage_candidate(&frame);
                frames.extend(decoder.push(frame).map_err(Failure::codec)?);
                self.chat_finish_seen = chat_finished;
                if let Some(usage) = reported_usage {
                    self.reported_usage = Some(usage);
                }
            }
            if let Some(usage) = decoder.reported_usage() {
                self.settle(&usage)?;
            }
            frames.extend(decoder.finish().map_err(Failure::codec)?);
            for event in &frames {
                if matches!(
                    event["type"].as_str(),
                    Some("response.completed" | "response.incomplete" | "response.failed")
                ) {
                    self.settle(&event["response"]["usage"])?;
                    let observation = observation::response(&self.context, &event["response"]);
                    self.barrier(&observation)?;
                    self.observe("response", observation);
                }
            }
            if buffered {
                self.head(200, "text/event-stream")?;
            }
            for event in frames {
                self.event(&event)?;
            }
            self.write(b"data: [DONE]\n\n")?;
        } else {
            let payload = if bytes.is_empty() {
                json!({})
            } else {
                serde_json::from_slice(&bytes)
                    .map_err(|_| status(format!("downstream returned non-JSON ({code})"), 502))?
            };
            if !(200..300).contains(&code) {
                let marker = format!(
                    "{} {} {}",
                    canonical::string(&payload["error"]["type"]),
                    canonical::string(&payload["error"]["code"]),
                    canonical::string(&payload["code"])
                )
                .to_ascii_lowercase();
                return Err(Failure {
                    message: payload["error"]["message"]
                        .as_str()
                        .or(payload["message"].as_str())
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("downstream request failed ({code})")),
                    status: if (400..500).contains(&code) {
                        code
                    } else {
                        502
                    },
                    auth: code == 401
                        || code == 403
                            && [
                                "auth",
                                "api_key",
                                "api-key",
                                "apikey",
                                "credential",
                                "unauthorized",
                            ]
                            .iter()
                            .any(|word| marker.contains(word)),
                    downstream: payload,
                    budget_code: None,
                    stream_code: None,
                    validation_code: None,
                    retryable: true,
                });
            }
            if let Some(failure) = Failure::stream(&payload) {
                if !payload["usage"].is_null() {
                    self.settle(&payload["usage"])?;
                }
                return Err(failure);
            }
            if let Some(usage) = output_failure::payload_usage(&protocol, &payload) {
                self.settle(&usage)?;
            }
            let canonical = if protocol == "responses" {
                responses::decode(&payload, &prepared["context"])
            } else {
                portable::decode(&protocol, &payload, &prepared["context"])
            }
            .map_err(Failure::codec)?;
            let observation = observation::response(&self.context, &canonical);
            self.barrier(&observation)?;
            self.observe("response", observation);
            self.settle(&canonical["usage"])?;
            if body["stream"] == false {
                for item in canonical::array(&canonical["output"]) { self.record_mcp_call(item)?; }
                self.observe_discovery(&canonical)?;
                self.head(200, "application/json; charset=utf-8")?;
                self.write(canonical.to_string().as_bytes())?;
            } else {
                self.head(200, "text/event-stream")?;
                for event in canonical::array(&canonical::events(&canonical)) {
                    self.event(event)?;
                }
                self.write(b"data: [DONE]\n\n")?;
            }
        }
        lock(&self.shared.server).end(self.id);
        lock(&self.shared.scheduler).feedback(self.id, 200);
        Ok(())
    }
    fn next_provider_event(&self, native: &mut Option<HttpTransport>) -> Outcome<HttpEvent> {
        if let Some(native) = native {
            return self.next_native(native);
        }
        let result = self.call("fetchRead", json!({"requestId":self.id}))?;
        if result["done"] == true {
            return Ok(HttpEvent::End);
        }
        Ok(HttpEvent::Chunk(
            serde_json::from_value(result["bytes"].clone())
                .map_err(|_| "invalid provider response bytes")?,
        ))
    }

    fn next_native(&self, transport: &mut HttpTransport) -> Outcome<HttpEvent> {
        loop {
            self.check()?;
            if let Some(event) = transport.poll() {
                return match event {
                    HttpEvent::Error(error) => Err(error.into()),
                    event => Ok(event),
                };
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn failure(&mut self, error: Failure) {
        let timed_out = error.downstream.is_null() && matches!(error.message.as_str(),
            "engine HTTP idle timed out" | "bridge host callback timed out" | "bridge response consumer timed out");
        let cancelled = self.control.cancelled.load(Ordering::Acquire)
            || self.shared.stopping.load(Ordering::Acquire)
            || timed_out;
        if !self.context.is_null() {
            let mut observation = self.context.clone();
            observation.as_object_mut().unwrap().extend(json!({"status":"failed","errorCategory":if error.auth{"authentication"}else if timed_out{"timeout"}else if cancelled{"cancelled"}else{"bridge"},"errorMessage":error.message.chars().take(500).collect::<String>(),"errorStatusCode":error.status,"outputTypes":[],"toolNames":[],"toolCalls":[],"textCharacters":0,"textLooksLikeToolCall":false}).as_object().unwrap().clone());
            observation["normalizedUsage"] = observation::usage(self.reported_usage.as_ref().unwrap_or(&self.observed_usage));
            observation["errorCode"] = error.diagnostic_code().into();
            if let Some(capacity) = error.context_capacity() {
                observation["contextCapacity"] = capacity;
            }
            observation["retryable"] = error.retryable.into();
            if error.stream_code.is_some() {
                observation["errorCategory"] = "provider".into();
                observation["retryable"] = error.retryable.into();
            }
            if error.item_limit_code().is_some() || error.validation_code.is_some() {
                observation["retryable"] = false.into();
            }
            if error.validation_code.is_some() {
                observation["errorCategory"] = "bridge".into();
            }
            if let Some(code) = error.budget_code {
                observation["errorCategory"] = "budget".into();
                observation["errorCode"] = code.into();
            }
            let _ = self.shared.call("response", observation, None);
        }
        if error.auth {
            let mut state = lock(&self.shared.execution);
            state.epoch = state.epoch.saturating_add(1);
            drop(state);
            for (id, control) in lock(&self.shared.active).iter() {
                if *id != self.id {
                    control.cancelled.store(true, Ordering::Release);
                }
            }
            let _=self.shared.call("qualification",json!({"connectionId":self.route["connectionId"],"credentialRef":self.route["credentialRef"],"environmentDigest":self.route["environmentDigest"],"protocol":self.route["protocol"],"statusCode":error.status}),None);
        }
        if self.headers_sent {
            if error.validation_code.is_some() {
                // Only this bounded local diagnostic may be written after a drain deadline/cancel.
                // Provider content and executable frames retain every normal cancellation gate.
                let event = json!({"type":"response.failed","response":{"status":"failed","error":{"code":"invalid_prompt","message":error.message},"output":[]}});
                let bytes = format!("event: response.failed\ndata: {event}\n\n");
                let mut server = lock(&self.shared.server);
                if server.write(self.id, bytes.as_bytes()).unwrap_or(false) {
                    server.end(self.id);
                } else {
                    server.abort(self.id);
                }
                return;
            }
            if let Some(code) = error.stream_code.or_else(|| error.item_limit_code()) {
                // The upstream core understands invalid_prompt as terminal, and
                // server_error as retryable. Do not turn output limits into a
                // disconnected stream that blindly replays the same request.
                let message = if error.item_limit_code() == Some("provider_output_item_limit") {
                    let (_, diagnostic) = error
                        .message
                        .split_once("; outputItem=")
                        .expect("classified output item diagnostic");
                    format!("{code}; outputItem={diagnostic}")
                } else {
                    code.into()
                };
                let event = json!({"type":"response.failed","response":{"status":"failed","error":{"code":if error.retryable {"server_error"} else {"invalid_prompt"},"message":message},"output":[]}});
                if self.event(&event).is_ok() {
                    lock(&self.shared.server).end(self.id);
                    return;
                }
            }
            lock(&self.shared.server).abort(self.id);
            return;
        }
        let (code, message, kind) = if error.validation_code.is_some() {
            (400, error.message.as_str(), "bridge_error")
        } else if timed_out {
            (504, "downstream request timed out", "timeout")
        } else if cancelled {
            (499, "request cancelled", "cancelled")
        } else if error.budget_code.is_some() {
            (
                error.status,
                error.message.as_str(),
                if error.budget_code == Some("task_provider_usage_unknown") {
                    "budget_unknown"
                } else {
                    "budget_exhausted"
                },
            )
        } else {
            (error.status, error.message.as_str(), "bridge_error")
        };
        let mut payload = json!({"error":{"message":message,"type":kind}});
        if let Some(code) = error.budget_code {
            payload["error"]["code"] = code.into();
            payload["error"]["retryable"] = false.into();
        }
        if let Some(code) = error.stream_code {
            payload["error"]["type"] = "provider_stream_error".into();
            payload["error"]["code"] = code.into();
            payload["error"]["retryable"] = error.retryable.into();
        }
        if let Some(code) = error.item_limit_code().or(error.validation_code) {
            payload["error"]["code"] = code.into();
            payload["error"]["retryable"] = false.into();
        }
        if let Some(capacity) = error.context_capacity() {
            payload["error"]["contextCapacity"] = capacity;
            payload["error"]["code"] = "context_input_budget_exceeded".into();
            payload["error"]["retryable"] = false.into();
        }
        if !error.downstream.is_null() {
            payload["error"]["downstream"] = error.downstream;
        }
        // The official Responses stream already defines context-window errors.
        // Translate our local admission result here instead of teaching Core a
        // product-specific HTTP error. Diagnostics keep their original code.
        if self.stream_requested && !payload["error"]["contextCapacity"].is_null() && !cancelled {
            payload["error"]["diagnosticCode"] = payload["error"]["code"].clone();
            payload["error"]["code"] = "context_length_exceeded".into();
            let event = json!({"type":"response.failed","response":{
                "id":format!("engine-context-{}",self.id),"status":"failed",
                "error":payload["error"],"output":[],"usage":null}});
            if self.head(200, "text/event-stream").is_ok() && self.event(&event).is_ok() {
                lock(&self.shared.server).end(self.id);
            } else {
                lock(&self.shared.server).abort(self.id);
            }
            return;
        }
        if self.head(code, "application/json; charset=utf-8").is_ok() {
            let _ = lock(&self.shared.server).write(self.id, payload.to_string().as_bytes());
            lock(&self.shared.server).end(self.id);
        } else {
            lock(&self.shared.server).abort(self.id);
        }
    }
}

fn logical_request_identity(lease: &Value, identity: &Value, route: &Value, body: &Value) -> String {
    let scope = json!({"runId":lease["runId"],"identity":identity,
        "connectionId":route["connectionId"],"body":body});
    format!("{:x}", Sha256::digest(scope.to_string().as_bytes()))
}

#[cfg(test)]
mod request_identity_tests {
    use super::*;
    #[test]
    fn retries_share_identity_but_new_turn_body_or_run_does_not() {
        let lease = json!({"runId":"attempt-one"});
        let identity = json!({"threadId":"thread","turnId":"turn-one"});
        let route = json!({"connectionId":"route"});
        let body = json!({"input":[{"role":"user","content":"source"}]});
        let key = logical_request_identity(&lease, &identity, &route, &body);
        assert_eq!(key, logical_request_identity(&lease, &identity, &route, &body));
        for (run, who, input) in [
            (json!({"runId":"attempt-two"}), identity.clone(), body.clone()),
            (lease.clone(), json!({"threadId":"thread","turnId":"turn-two"}), body.clone()),
            (lease.clone(), identity.clone(), json!({"input":[{"role":"tool","content":"new receipt"}]})),
        ] { assert_ne!(key, logical_request_identity(&run, &who, &route, &input)); }
    }
}
