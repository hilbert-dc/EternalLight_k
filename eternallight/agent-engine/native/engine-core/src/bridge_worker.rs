use crate::{BridgeService, Result};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn write(output: &mut impl Write, value: Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(&value).map_err(|_| "invalid bridge output")?;
    if bytes.len() > 256 * 1024 {
        return Err("bridge output exceeds byte limit".into());
    }
    bytes.push(b'\n');
    output
        .write_all(&bytes)
        .and_then(|()| output.flush())
        .map_err(|_| "bridge parent disconnected".into())
}

struct Worker {
    bridge: Option<BridgeService>,
    credentials: Value,
    budget: Value,
    lease: Value,
    draining: Vec<(Value, Value)>,
    closing: Option<Value>,
    disconnected: bool,
}
impl Worker {
    fn handle(&mut self, message: &Value) -> Result<Option<Value>> {
        if message["kind"] == "host-response" {
            if let Some(bridge) = &self.bridge {
                bridge.apply("reply",&json!({"id":message["requestId"],"result":message["result"],"error":message["error"]["message"]}))?;
            }
            return Ok(None);
        }
        let params = &message["params"];
        let method = message["method"]
            .as_str()
            .ok_or("bridge control method is required")?;
        if method == "start" {
            if self.bridge.is_some() {
                return Err("bridge is already started".into());
            }
            self.credentials = params["credentials"].clone();
            let token = params["token"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    )
                });
            let options = json!({"sessionPlan":params["sessionPlan"],"connections":params["connections"],"toolPolicy":params["toolPolicy"],"responseBarrier":params["responseBarrier"],"token":token,"port":params["port"].as_u64().unwrap_or(0),"maxBodyBytes":2*1024*1024,"maxDownstreamBytes":32*1024*1024,"maxConcurrentRequests":8,"requestTimeoutMs":120_000,"customFetch":false});
            let bridge = BridgeService::start(options)?;
            let info = bridge.apply("inspect", &json!({}))?;
            self.bridge = Some(bridge);
            return Ok(Some(json!({"baseUrl":info["baseUrl"],"token":token})));
        }
        let bridge = self.bridge.as_ref().ok_or("bridge is not started")?;
        match method {
            "ping" => Ok(Some(json!({"alive":true}))),
            "budget:start" => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| "invalid system time")?
                    .as_millis() as u64;
                let deadline = params["startedAt"].as_u64().unwrap_or(now).saturating_add(
                    params["maxDurationMs"]
                        .as_u64()
                        .ok_or("invalid budget duration")?,
                );
                bridge.apply(
                    "budget:start",
                    &json!({"id":params["id"],"deadline":deadline}),
                )?;
                self.budget = params.clone();
                Ok(Some(json!({"id":params["id"]})))
            }
            "lease:start" => {
                bridge.apply("lease:start", params)?;
                self.lease = params.clone();
                Ok(Some(params.clone()))
            }
            "budget:end" => {
                let ended = bridge.apply("budget:end", &self.budget)?;
                self.budget = Value::Null;
                Ok(Some(json!({"ended":ended})))
            }
            "lease:end" => {
                if bridge.apply("lease:matches", &self.lease)? != true {
                    return Ok(Some(json!({"ended":false})));
                }
                bridge.apply("cancel", &json!({}))?;
                self.draining
                    .push((message["id"].clone(), json!({"ended":true})));
                Ok(None)
            }
            "cancel" => {
                let count = bridge.apply("cancel", &json!({}))?;
                self.draining.push((message["id"].clone(), count));
                Ok(None)
            }
            "close" => {
                bridge.apply("cancel", &json!({}))?;
                self.closing = Some(message["id"].clone());
                Ok(None)
            }
            _ => Err(format!("unknown bridge control method: {method}")),
        }
    }
    fn poll(&mut self, output: &mut impl Write) -> Result<bool> {
        let Some(bridge) = &self.bridge else {
            return Ok(self.disconnected);
        };
        for event in bridge
            .apply("poll", &json!({}))?
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = &event["id"];
            let params = &event["params"];
            if self.disconnected {
                bridge.apply(
                    "reply",
                    &json!({"id":id,"error":"bridge parent disconnected"}),
                )?;
                continue;
            }
            let method = match event["method"].as_str() {
                Some("credential") => {
                    bridge.apply(
                        "reply",
                        &json!({"id":id,"result":self.credentials[params.as_str().unwrap_or("")]}),
                    )?;
                    continue;
                }
                Some("releaseFetch") => {
                    bridge.apply("reply", &json!({"id":id,"result":null}))?;
                    continue;
                }
                Some("reserve") => "budget:reserve",
                Some("settle") => "budget:settle",
                Some("request") => "request:observed",
                Some("response") => "response:observed",
                Some("qualification") => "qualification:failure",
                _ => return Err("unsupported native bridge callback".into()),
            };
            write(
                output,
                json!({"kind":"host-request","requestId":id,"method":method,"params":params}),
            )?;
        }
        if bridge.apply("inspect", &json!({}))?["active"] == 0 {
            for (id, result) in std::mem::take(&mut self.draining) {
                if result["ended"] == true {
                    bridge.apply("lease:clear", &json!({}))?;
                    self.lease = Value::Null;
                }
                if !self.disconnected {
                    write(output, json!({"id":id,"result":result}))?;
                }
            }
            if self.closing.is_some() || self.disconnected {
                bridge.close()?;
                if let Some(id) = self.closing.take()
                    && !self.disconnected
                {
                    write(output, json!({"id":id,"result":{"closed":true}}))?;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Runs the isolated bridge control channel. All input and output are bounded;
/// no model or product code is evaluated by this transport worker.
pub fn run() -> Result<()> {
    let (tx, rx) = mpsc::sync_channel(32);
    std::thread::Builder::new()
        .name("bridge-control-reader".into())
        .spawn(move || {
            let mut input = BufReader::new(std::io::stdin());
            loop {
                let mut line = Vec::new();
                let result = Read::by_ref(&mut input)
                    .take(2 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut line);
                let value = match result {
                    Ok(0) => break,
                    Ok(_) if line.len() <= 2 * 1024 * 1024 => {
                        serde_json::from_slice::<Value>(&line)
                            .map_err(|_| "invalid bridge control JSON".to_owned())
                    }
                    _ => Err("bridge control input exceeds limit or failed".into()),
                };
                let failed = value.is_err();
                if tx.send(value).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|_| "cannot start bridge control reader")?;
    let mut worker = Worker {
        bridge: None,
        credentials: Value::Null,
        budget: Value::Null,
        lease: Value::Null,
        draining: Vec::new(),
        closing: None,
        disconnected: false,
    };
    let mut output = std::io::stdout().lock();
    loop {
        if !worker.disconnected {
            match rx.recv_timeout(Duration::from_millis(2)) {
                Ok(Ok(message)) => {
                    if worker.draining.len() >= 1024 {
                        return Err("bridge pending control limit exceeded".into());
                    }
                    match worker.handle(&message) {
                        Ok(Some(result)) => {
                            write(&mut output, json!({"id":message["id"],"result":result}))?
                        }
                        Ok(None) => {}
                        Err(error) => write(
                            &mut output,
                            json!({"id":message["id"],"error":{"message":error}}),
                        )?,
                    }
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    worker.disconnected = true;
                    if let Some(bridge) = &worker.bridge {
                        bridge.apply("cancel", &json!({}))?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
        if worker.poll(&mut output)? {
            return Ok(());
        }
    }
}
