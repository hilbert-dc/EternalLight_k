use crate::{
    ProbeSession, Result,
    canonical::{array, string},
    contracts,
    digest::digest,
    probe, session_identity,
};
use serde_json::{Value, json};

/// Owns protocol negotiation, retry policy and evidence assembly. The embedding
/// performs only bounded transport/stream IO for the currently issued request.
pub struct CapabilityVerifier {
    input: Value,
    session: ProbeSession,
    protocol: Value,
    forced: bool,
    streaming: bool,
    canonical: Value,
    context: Value,
    records: Vec<Value>,
    pending: bool,
    terminal: Option<Value>,
}
impl CapabilityVerifier {
    pub fn new(mut input: Value) -> Result<Self> {
        if string(&input["apiKey"]).is_empty() {
            return Err("verification requires a credential value".into());
        }
        chrono::DateTime::parse_from_rfc3339(string(&input["now"]))
            .map_err(|_| "invalid verification timestamp")?;
        if input["engineContract"].is_null() {
            input["engineContract"] = contracts::definitions()["ENGINE_CONTRACT"].clone();
        }
        let session = ProbeSession::new(
            &json!({"protocol":input["connection"]["protocol"],"protocols":input["protocols"],"maxAttempts":input["maxAttempts"].as_u64().unwrap_or(4)}),
        )?;
        for (field, default, max) in [
            ("ttlMs", 86_400_000, 2_592_000_000u64),
            ("maxResponseBytes", 1_048_576, 32 * 1_048_576),
            ("timeoutMs", 20_000, 600_000),
        ] {
            let value = input[field].as_u64().unwrap_or(default);
            if value == 0 || value > max {
                return Err(format!("invalid verifier {field}"));
            }
            input[field] = value.into();
        }
        Ok(Self {
            input,
            session,
            protocol: Value::Null,
            forced: true,
            streaming: false,
            canonical: Value::Null,
            context: Value::Null,
            records: Vec::new(),
            pending: false,
            terminal: None,
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "next" => self.next(),
            "response" => {
                if !self.pending {
                    return Err("verification has no pending transport request".into());
                }
                self.pending = false;
                if let Err(error) = self.response(input) {
                    self.failure(&json!({"message":error,"status":422}))?;
                }
                Ok(Value::Null)
            }
            "result" => self
                .terminal
                .clone()
                .ok_or("verification is incomplete".into()),
            _ => Err("unknown capability verifier operation".into()),
        }
    }
    fn next(&mut self) -> Result<Value> {
        if self.pending {
            return Err("verification transport request is already pending".into());
        }
        if self.terminal.is_some() {
            return Ok(Value::Null);
        }
        if self.protocol.is_null() {
            self.protocol = self.session.apply("next", &Value::Null)?;
            self.forced = true;
            self.streaming = false;
            if self.protocol.is_null() {
                let attempts = self.session.apply("result", &Value::Null)?["attempts"].clone();
                let detail = array(&attempts)
                    .iter()
                    .map(|a| format!("{}:{}", string(&a["protocol"]), string(&a["category"])))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.terminal = Some(
                    json!({"error":format!("no protocol passed capability verification: {detail}"),"attempts":attempts,"records":self.records}),
                );
                return Ok(Value::Null);
            }
        }
        let request = probe::dispatch(
            "probe.request",
            &json!({"connection":self.input["connection"],"forceToolChoice":self.forced,"stream":self.streaming}),
        )?;
        let mut outbound = crate::dispatch(
            if self.protocol == "responses" {
                "responses.encode"
            } else {
                "codec.encode"
            },
            json!({"protocol":self.protocol,"request":request,"connection":self.input["connection"],"apiKey":self.input["apiKey"]}),
        )?;
        self.context = outbound["context"].clone();
        outbound["protocol"] = self.protocol.clone();
        outbound["stream"] = self.streaming.into();
        outbound["timeoutMs"] = self.input["timeoutMs"].clone();
        outbound["maxResponseBytes"] = self.input["maxResponseBytes"].clone();
        self.pending = true;
        Ok(outbound)
    }
    fn response(&mut self, input: &Value) -> Result<()> {
        if input["error"].is_object() {
            return self.failure(&input["error"]);
        }
        let status = input["status"]
            .as_u64()
            .ok_or("missing probe HTTP status")?;
        let text = string(&input["text"]);
        if text.len() as u64 > self.input["maxResponseBytes"].as_u64().unwrap() {
            return self.failure(&json!({"message":"probe response exceeds byte limit","status":status,"category":"transport"}));
        }
        let payload: Value = if text.is_empty() {
            json!({})
        } else {
            serde_json::from_str(text).unwrap_or(Value::Null)
        };
        if !self.streaming && payload.is_null() {
            return self.failure(&json!({"message":"probe response is not JSON","status":status}));
        }
        if !(200..300).contains(&status) {
            if !self.streaming
                && self.forced
                && probe::dispatch(
                    "probe.retryToolChoice",
                    &json!({"status":status,"payload":payload}),
                )? == true
            {
                self.forced = false;
                return Ok(());
            }
            let category = probe::dispatch(
                "probe.statusCategory",
                &json!({"status":status,"payload":payload}),
            )?;
            let message = payload["error"]["message"]
                .as_str()
                .or_else(|| payload["message"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("probe failed ({status})"));
            return self.failure(&json!({"status":status,"message":message,"category":category}));
        }
        if self.streaming {
            probe::dispatch("probe.roundTrip", &input["canonical"])?;
            self.success("verified", json!({"basis":"native-sse-tool-round-trip"}))
        } else {
            self.canonical = crate::dispatch(
                if self.protocol == "responses" {
                    "responses.decode"
                } else {
                    "codec.decode"
                },
                json!({"protocol":self.protocol,"payload":payload,"context":self.context}),
            )?;
            probe::dispatch("probe.roundTrip", &self.canonical)?;
            if matches!(
                self.protocol.as_str(),
                Some("responses" | "chat-completions")
            ) {
                self.streaming = true;
                Ok(())
            } else {
                self.success(
                    "unknown",
                    json!({"reason":"codec-does-not-support-native-streaming"}),
                )
            }
        }
    }
    fn failure(&mut self, error: &Value) -> Result<()> {
        let mut error = error.clone();
        error["message"] = string(&error["message"])
            .replace(string(&self.input["apiKey"]), "[redacted]")
            .chars()
            .take(500)
            .collect::<String>()
            .into();
        if self.streaming {
            let decision = probe::dispatch("probe.streamingFailure", &error)?;
            if decision["fatal"] != true {
                return self.success(
                    string(&decision["status"]),
                    json!({"reason":decision["category"],"message":error["message"]}),
                );
            }
        }
        let decision = self
            .session
            .apply("failure", &json!({"protocol":self.protocol,"error":error}))?;
        if decision["recordFailure"] == true {
            for capability in ["text", "structuredToolCalls"] {
                self.records.push(self.evidence(
                    capability,
                    "failed",
                    json!({"category":decision["category"],"message":decision["message"]}),
                    "buffered",
                    ":failed",
                )?);
            }
        }
        self.protocol = Value::Null;
        Ok(())
    }
    fn success(&mut self, streaming_status: &str, sample: Value) -> Result<()> {
        for capability in ["text", "structuredToolCalls"] {
            self.records.push(self.evidence(
                capability,
                "verified",
                Value::Null,
                "buffered",
                "",
            )?);
        }
        self.records.push(self.evidence(
            "streaming",
            streaming_status,
            sample,
            "native-sse",
            "",
        )?);
        self.records.push(self.evidence(
            "usage",
            if self.canonical["usage"].is_null() {
                "unknown"
            } else {
                "verified"
            },
            Value::Null,
            "buffered",
            "",
        )?);
        let attempts = self
            .session
            .apply("success", &json!({"protocol":self.protocol}))?["attempts"]
            .clone();
        self.terminal = Some(
            json!({"selectedProtocol":self.protocol,"records":self.records,"attempts":attempts}),
        );
        Ok(())
    }
    fn evidence(
        &self,
        capability: &str,
        status: &str,
        sample: Value,
        mode: &str,
        suffix: &str,
    ) -> Result<Value> {
        let connection = &self.input["connection"];
        let id = string(&connection["id"]);
        let protocol = string(&self.protocol);
        let now = string(&self.input["now"]);
        let checked = chrono::DateTime::parse_from_rfc3339(now)
            .map_err(|_| "invalid verification timestamp")?;
        let expires = (checked
            + chrono::Duration::milliseconds(self.input["ttlMs"].as_i64().unwrap()))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let scope = self
            .input
            .get("credentialScope")
            .unwrap_or(&connection["credentialRef"]);
        let realm = session_identity::dispatch("connection.credentialRealm", scope)?;
        let environment = session_identity::dispatch(
            "connection.environmentDigest",
            &json!({"connection":connection,"options":{"codecVersion":"3","credentialScope":scope,"engineContractHash":digest(&self.input["engineContract"]),"protocol":protocol,"qualificationContextDigest":self.input["qualificationContextDigest"].as_str().unwrap_or("default")}}),
        )?;
        let reference = format!("probe:{id}:{protocol}:{now}{suffix}");
        Ok(
            json!({"capability":capability,"status":status,"scope":{"protocol":protocol,"suite":"synthetic-tool-probe-v1","mode":mode,"toolTypes":["function"]},"source":"probe","credentialRealm":realm,"environmentDigest":environment,"evidenceRef":format!("{reference}:{capability}"),"checkedAt":now,"expiresAt":expires,"sample":sample,"assertions":[{"id":format!("model:{capability}:observed"),"result":match status{"verified"=>"passed","failed"=>"failed",_=>"unknown"},"expected":"capability observed through the selected wire protocol","observed":if sample.is_null(){json!(status)}else{sample},"traceRef":reference}],"dependencies":{"modelProtocol":{"digest":digest(&json!({"connectionDigest":session_identity::definition(connection),"protocol":protocol,"codecVersion":"3"})),"ref":format!("{id}:{protocol}:3")},"access":{"digest":realm,"ref":format!("{id}:credential-realm")}}}),
        )
    }
}
