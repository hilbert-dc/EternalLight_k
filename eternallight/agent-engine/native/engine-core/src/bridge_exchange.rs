use super::{Control, Shared, lock, observation};
use crate::{
    ChatStream, HttpEvent, HttpOptions, HttpTransport, ResponsesStream, SseDecoder, canonical,
    local_http::LocalRequest, portable, responses, responses_request,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

struct Failure {
    message: String,
    status: u16,
    auth: bool,
    downstream: Value,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            status: 502,
            auth: false,
            downstream: Value::Null,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
type Outcome<T> = std::result::Result<T, Failure>;
fn status(error: impl Into<String>, status: u16) -> Failure {
    Failure {
        message: error.into(),
        status,
        auth: false,
        downstream: Value::Null,
    }
}
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
    budget: Value,
    lease: Value,
    route: Value,
    identity: Value,
    context: Value,
    reservation: Value,
    settled: bool,
    downstream_started: bool,
}

pub(super) fn run(shared: &Shared, request: LocalRequest, control: &Control) {
    let mut exchange = Exchange {
        shared,
        control,
        id: request.id,
        headers_sent: false,
        budget: Value::Null,
        lease: Value::Null,
        route: Value::Null,
        identity: Value::Null,
        context: Value::Null,
        reservation: Value::Null,
        settled: false,
        downstream_started: false,
    };
    if let Err(error) = exchange.execute(&request) {
        exchange.failure(error);
    }
    if !exchange.reservation.is_null()&&!exchange.settled
        && let Err(error)=shared.call("settle",json!({"budgetId":exchange.budget["id"],"id":exchange.reservation,"usage":null,"options":{"outcome":if exchange.downstream_started{"unknown"}else{"not-sent"}}}),None) {
        *lock(&shared.settlement_failure)=Some(error);shared.cancel();
    }
    let _ = shared.call("releaseFetch", json!({"requestId":request.id}), None);
}

impl Exchange<'_> {
    fn check(&self) -> Outcome<()> {
        self.shared
            .cancelled(self.control, self.id)
            .map_err(Into::into)
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
        loop {
            self.check()?;
            if lock(&self.shared.server).write(self.id, bytes)? {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn event(&self, event: &Value) -> Outcome<()> {
        self.write(
            format!(
                "event: {}\ndata: {event}\n\n",
                canonical::string(&event["type"])
            )
            .as_bytes(),
        )
    }
    fn settle(&mut self, usage: &Value) -> Outcome<()> {
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
            self.route = state
                .policy
                .apply(
                    "route",
                    &json!({"body":body,"sessionPlan":self.shared.options["sessionPlan"]}),
                )
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
        let policy = &self.shared.options["toolPolicy"];
        let exists = |field: &str| {
            policy[field]
                .as_str()
                .is_some_and(|p| Path::new(p).exists())
        };
        body=lock(&self.shared.execution).policy.apply("prepare",&json!({"body":body,"outputBudget":self.route["outputBudget"],"identity":self.identity,"policy":policy,"childComplete":exists("childCompletionFile"),"childPhaseReached":exists("childPhaseFile")})).map_err(|e|status(e,403))?;
        let protocol = canonical::string(&self.route["protocol"]).to_owned();
        let prepared = if protocol == "responses" {
            responses_request::encode(&body, connection, key)?
        } else {
            portable::encode(&protocol, &body, connection, key)?
        };
        self.context = self.identity.clone();
        self.context.as_object_mut().ok_or("invalid route identity")?.extend(json!({"runId":self.lease["runId"],"requestId":prepared["context"]["requestId"],"connectionId":self.route["connectionId"],"logicalModel":self.route["logicalModel"]}).as_object().unwrap().clone());
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
        if estimated > self.route["inputBudget"].as_u64().unwrap_or(0) {
            return Err(status(
                "encoded request exceeds the route input budget",
                413,
            ));
        }
        // Reservation completion cannot be abandoned on cancellation: its id is
        // needed to refund a request that never reached the provider.
        self.reservation=self.shared.call("reserve",json!({"budgetId":self.budget["id"],"estimatedInputTokens":estimated,"maxOutputTokens":body["max_output_tokens"]}),None)?;
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
            timeout_ms: self.shared.options["requestTimeoutMs"]
                .as_u64()
                .unwrap_or(120_000),
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
            let result=self.call("fetch",json!({"requestId":self.id,"url":prepared["url"],"headers":prepared["headers"],"body":prepared["body"].to_string(),"maxBytes":max,"timeoutMs":self.shared.options["requestTimeoutMs"]}))?;
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
            let event = if let Some(native) = &mut native {
                self.next_native(native)?
            } else {
                let result = self.call("fetchRead", json!({"requestId":self.id}))?;
                if result["done"] == true {
                    HttpEvent::End
                } else {
                    HttpEvent::Chunk(
                        serde_json::from_value(result["bytes"].clone())
                            .map_err(|_| "invalid provider response bytes")?,
                    )
                }
            };
            match event {
                HttpEvent::Chunk(chunk) => {
                    received = received.saturating_add(chunk.len());
                    if received > max {
                        return Err("downstream response exceeds bridge limit".into());
                    }
                    if let Some(decoder) = &mut decoder {
                        for frame in sse.push(&chunk)? {
                            for event in decoder.push(frame)? {
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
                frames.extend(decoder.push(frame)?);
            }
            frames.extend(decoder.finish()?);
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
                });
            }
            let canonical = if protocol == "responses" {
                responses::decode(&payload, &prepared["context"])?
            } else {
                portable::decode(&protocol, &payload, &prepared["context"])?
            };
            let observation = observation::response(&self.context, &canonical);
            self.barrier(&observation)?;
            self.observe("response", observation);
            self.settle(&canonical["usage"])?;
            if body["stream"] == false {
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
        Ok(())
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
        let timed_out = Instant::now() >= self.control.deadline;
        let cancelled = self.control.cancelled.load(Ordering::Acquire)
            || self.shared.stopping.load(Ordering::Acquire)
            || timed_out;
        if !self.context.is_null() {
            let mut observation = self.context.clone();
            observation.as_object_mut().unwrap().extend(json!({"status":"failed","errorCategory":if error.auth{"authentication"}else if timed_out{"timeout"}else if cancelled{"cancelled"}else{"bridge"},"errorMessage":error.message.chars().take(500).collect::<String>(),"errorStatusCode":error.status,"outputTypes":[],"toolNames":[],"toolCalls":[],"textCharacters":0,"textLooksLikeToolCall":false}).as_object().unwrap().clone());
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
            lock(&self.shared.server).abort(self.id);
            return;
        }
        let (code, message, kind) = if timed_out {
            (504, "downstream request timed out", "timeout")
        } else if cancelled {
            (499, "request cancelled", "cancelled")
        } else {
            (error.status, error.message.as_str(), "bridge_error")
        };
        let mut payload = json!({"error":{"message":message,"type":kind}});
        if !error.downstream.is_null() {
            payload["error"]["downstream"] = error.downstream;
        }
        if self.head(code, "application/json; charset=utf-8").is_ok() {
            let _ = lock(&self.shared.server).write(self.id, payload.to_string().as_bytes());
            lock(&self.shared.server).end(self.id);
        } else {
            lock(&self.shared.server).abort(self.id);
        }
    }
}
