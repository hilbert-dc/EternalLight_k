use crate::{Result, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{BufRead, Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Mode {
    Core,
    Delegation,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    mode: Mode,
    receipt: String,
    proof_path: PathBuf,
    nonce: Option<String>,
    instruction: Option<String>,
    followup: Option<String>,
    gate_path: Option<PathBuf>,
    release_path: Option<PathBuf>,
    trace_path: Option<PathBuf>,
}

fn trace(options: &Options, kind: &str, details: Value) -> Result<()> {
    let Some(path) = &options.trace_path else {
        return Ok(());
    };
    let mut open = OpenOptions::new();
    open.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let mut file = open
        .open(path)
        .map_err(|_| "unable to open conformance trace")?;
    if file
        .metadata()
        .map_err(|_| "unable to inspect conformance trace")?
        .len()
        > 1024 * 1024
    {
        return Err("conformance trace exceeds limit".into());
    }
    writeln!(file, "{}", json!({"type":kind,"details":details}))
        .map_err(|_| "unable to write conformance trace".into())
}

enum ToolOutcome {
    Success,
    Failure,
}

fn tool_result(text: &str, outcome: ToolOutcome) -> Value {
    let mut result = json!({"content":[{"type":"text","text":text}]});
    if matches!(outcome, ToolOutcome::Failure) {
        result["isError"] = true.into();
    }
    result
}

fn handle(options: &Options, message: &Value) -> Result<Option<Value>> {
    let result = match message["method"].as_str().unwrap_or("") {
        "notifications/initialized" => return Ok(None),
        "initialize" => {
            json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":if options.mode == Mode::Core {"codex-engine-core-conformance"} else {"codex-engine-conformance"},"version":"1"}})
        }
        "tools/list" if options.mode == Mode::Core => {
            json!({"tools":[{"name":"engine_core_echo","description":"Side-effect-free Codex core integration probe.","inputSchema":{"type":"object","properties":{"nonce":{"type":"string"},"instruction":{"type":"string"}},"required":["nonce","instruction"],"additionalProperties":false}}]})
        }
        "tools/list" => {
            json!({"tools":[{"name":"engine_conformance_gate","description":"Open the delegated conformance gate before consuming the parent's queued follow-up.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},{"name":"engine_conformance_echo","description":"Return the private receipt when followup exactly matches the token in the parent's message. Copy only the value after FOLLOWUP_TOKEN=, without the marker, whitespace, or punctuation.","inputSchema":{"type":"object","properties":{"followup":{"type":"string"}},"required":["followup"],"additionalProperties":false}}]})
        }
        "tools/call" => call(options, &message["params"])?,
        _ => {
            return Ok(Some(
                json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"method not found"}}),
            ));
        }
    };
    Ok(Some(
        json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
    ))
}

fn call(options: &Options, params: &Value) -> Result<Value> {
    let name = params["name"].as_str().unwrap_or("");
    let meta = &params["_meta"];
    let args = &params["arguments"];
    let identity = meta["callerThreadId"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
        && meta["rootThreadId"].as_str().is_some_and(|s| !s.is_empty())
        && meta["sessionId"] == meta["rootThreadId"];
    if options.mode == Mode::Core {
        if name != "engine_core_echo" {
            return Ok(tool_result(
                "unknown conformance tool",
                ToolOutcome::Failure,
            ));
        }
        if !identity
            || meta["callerThreadId"] != meta["rootThreadId"]
            || args["nonce"].as_str() != options.nonce.as_deref()
            || args["instruction"].as_str() != options.instruction.as_deref()
        {
            return Ok(tool_result(
                "core conformance values did not round-trip",
                ToolOutcome::Failure,
            ));
        }
        storage::durable_replace_json(
            &options.proof_path,
            &json!({"receipt":options.receipt,"callerThreadId":meta["callerThreadId"],"rootThreadId":meta["rootThreadId"]}),
        )?;
        return Ok(tool_result(
            &format!("CORE_CONFORMANCE_RECEIPT:{}", options.receipt),
            ToolOutcome::Success,
        ));
    }
    if !matches!(name, "engine_conformance_gate" | "engine_conformance_echo") {
        return Ok(tool_result(
            "unknown conformance tool",
            ToolOutcome::Failure,
        ));
    }
    let mut details = json!({"toolName":name,"callerThreadId":meta["callerThreadId"],"rootThreadId":meta["rootThreadId"]});
    if !meta["callId"].is_null() {
        details["callId"] = meta["callId"].clone();
    }
    if !identity || meta["callerThreadId"] == meta["rootThreadId"] {
        details["reason"] = "identity".into();
        trace(options, "mcp.call.rejected", details)?;
        return Ok(tool_result(
            "conformance identity did not round-trip",
            ToolOutcome::Failure,
        ));
    }
    let gate = options
        .gate_path
        .as_ref()
        .ok_or("conformance gate path is required")?;
    if name == "engine_conformance_gate" {
        storage::durable_replace_json(
            gate,
            &json!({"callerThreadId":meta["callerThreadId"],"rootThreadId":meta["rootThreadId"]}),
        )?;
        trace(options, "mcp.call.started", details.clone())?;
        if let Some(path) = &options.release_path {
            let started = Instant::now();
            while !path.exists() {
                if started.elapsed() >= Duration::from_secs(30) {
                    details["reason"] = "release-timeout".into();
                    trace(options, "mcp.call.rejected", details)?;
                    return Ok(tool_result(
                        "conformance parent message was not observed while child was running",
                        ToolOutcome::Failure,
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        trace(options, "mcp.call.completed", details)?;
        return Ok(tool_result("CONFORMANCE_GATE_OPEN", ToolOutcome::Success));
    }
    if !gate.exists() || args["followup"].as_str() != options.followup.as_deref() {
        details["reason"] = "arguments".into();
        details["gateObserved"] = gate.exists().into();
        details["followupMatched"] =
            (args["followup"].as_str() == options.followup.as_deref()).into();
        details["comparison"] =
            diagnostics(&args["followup"], options.followup.as_deref().unwrap_or(""));
        trace(options, "mcp.call.rejected", details)?;
        return Ok(tool_result(
            "Token mismatch. Read the parent's FOLLOWUP_TOKEN message and copy only its exact value into followup; do not invent or transform the value.",
            ToolOutcome::Failure,
        ));
    }
    let owner = storage::read_json(gate)?.ok_or("conformance gate is missing")?;
    if owner["callerThreadId"] != meta["callerThreadId"]
        || owner["rootThreadId"] != meta["rootThreadId"]
    {
        return Ok(tool_result(
            "conformance gate identity mismatch",
            ToolOutcome::Failure,
        ));
    }
    storage::durable_replace_json(
        &options.proof_path,
        &json!({"receipt":options.receipt,"callerThreadId":meta["callerThreadId"],"rootThreadId":meta["rootThreadId"]}),
    )?;
    trace(options, "mcp.call.completed", details)?;
    Ok(tool_result(
        &format!("CONFORMANCE_RECEIPT:{}", options.receipt),
        ToolOutcome::Success,
    ))
}

pub(crate) fn diagnostics(actual: &Value, expected: &str) -> Value {
    let text = actual.as_str();
    let units: Vec<_> = text.unwrap_or("").encode_utf16().collect();
    let difference = expected
        .encode_utf16()
        .enumerate()
        .position(|(i, c)| units.get(i) != Some(&c))
        .map_or(-1, |i| i as i64);
    json!({"receivedType":match actual { Value::String(_)=>"string",Value::Null=>"object",Value::Bool(_)=>"boolean",Value::Number(_)=>"number",Value::Array(_)|Value::Object(_)=>"object" },"receivedLength":text.map(|s|s.encode_utf16().count()),"expectedLength":expected.encode_utf16().count(),"exactMatch":text==Some(expected),"trimMatch":text.is_some_and(|s|s.trim()==expected),"includesExpected":text.is_some_and(|s|s.contains(expected)),"includesMarker":text.is_some_and(|s|s.contains("FOLLOWUP_TOKEN=")),"firstDifference":text.map(|_|difference)})
}

pub fn run(input: Value) -> Result<()> {
    let options: Options =
        serde_json::from_value(input).map_err(|_| "invalid qualification MCP options")?;
    if options.receipt.is_empty()
        || options.receipt.len() > 4096
        || !options.proof_path.is_absolute()
        || [
            &options.trace_path,
            &options.gate_path,
            &options.release_path,
        ]
        .into_iter()
        .flatten()
        .any(|path| !path.is_absolute())
        || (options.mode == Mode::Core
            && (options.nonce.as_deref().is_none_or(str::is_empty)
                || options.instruction.as_deref().is_none_or(str::is_empty)))
        || (options.mode == Mode::Delegation
            && (options.followup.as_deref().is_none_or(str::is_empty)
                || options.gate_path.is_none()))
    {
        return Err("qualification MCP requires bounded secrets and absolute paths".into());
    }
    let mut reader = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut bytes = Vec::new();
        let read = reader
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "MCP input failed")?;
        if read == 0 {
            return Ok(());
        }
        if bytes.len() > 1024 * 1024 {
            return Err("MCP request exceeds limit".into());
        }
        let response = match serde_json::from_slice::<Value>(&bytes) {
            Ok(message) => match handle(&options, &message) {
                Ok(response) => response,
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32603,"message":"qualification MCP operation failed"}}),
                ),
            },
            Err(_) => Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"invalid JSON"}}),
            ),
        };
        if let Some(response) = response {
            writeln!(output, "{response}")
                .and_then(|()| output.flush())
                .map_err(|_| "MCP output failed")?;
        }
    }
}
