use crate::Result;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Executor {
    command: String,
    args: Vec<String>,
    cwd: PathBuf,
    timeout_ms: u64,
    max_output_bytes: usize,
}

struct OwnedCommand(Child);
impl Drop for OwnedCommand {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

enum Output {
    Stdout(std::result::Result<Vec<u8>, String>),
    Stderr(Vec<u8>),
    InputError,
}

/// Runs a frozen behavior executor without a shell, with explicit environment,
/// input/output limits and a deadline covering both process exit and pipe drain.
pub fn execute(input: &Value) -> Result<Value> {
    let executor: Executor = serde_json::from_value(input["executor"].clone())
        .map_err(|_| "invalid behavior executor")?;
    let environment: BTreeMap<String, String> =
        serde_json::from_value(input["environment"].clone())
            .map_err(|_| "invalid behavior executor environment")?;
    if executor.command.is_empty()
        || executor.timeout_ms == 0
        || executor.max_output_bytes == 0
        || executor.max_output_bytes > 32 * 1024 * 1024
    {
        return Err("invalid behavior executor limits".into());
    }
    let mut payload =
        serde_json::to_vec(&input["payload"]).map_err(|_| "invalid behavior executor payload")?;
    if payload.len() > 2 * 1024 * 1024 {
        return Err("behavior executor input exceeds byte limit".into());
    }
    payload.push(b'\n');
    let mut command = Command::new(executor.command);
    command
        .args(executor.args)
        .current_dir(executor.cwd)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = OwnedCommand(
        command
            .spawn()
            .map_err(|e| format!("cannot start behavior executor: {e}"))?,
    );
    let mut stdin = child.0.stdin.take().ok_or("missing executor stdin")?;
    let stdout = child.0.stdout.take().ok_or("missing executor stdout")?;
    let mut stderr = child.0.stderr.take().ok_or("missing executor stderr")?;
    let (sender, receiver) = mpsc::channel();
    let writer = sender.clone();
    std::thread::Builder::new()
        .name("engine-eval-input".into())
        .spawn(move || {
            if stdin.write_all(&payload).is_err() {
                let _ = writer.send(Output::InputError);
            }
        })
        .map_err(|e| e.to_string())?;
    let output_sender = sender.clone();
    std::thread::Builder::new()
        .name("engine-eval-output".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .take(executor.max_output_bytes as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "behavior executor output read failed".to_owned())
                .and_then(|_| {
                    if bytes.len() > executor.max_output_bytes {
                        Err("behavior executor output exceeded its byte limit".into())
                    } else {
                        Ok(bytes)
                    }
                });
            let _ = output_sender.send(Output::Stdout(result));
        })
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("engine-eval-stderr".into())
        .spawn(move || {
            let mut retained = Vec::new();
            let mut buffer = [0; 4096];
            while let Ok(count) = stderr.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                retained.extend_from_slice(&buffer[..count.min(16_384 - retained.len())]);
            }
            let _ = sender.send(Output::Stderr(retained));
        })
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(executor.timeout_ms))
        .ok_or("invalid behavior executor deadline")?;
    let mut output = None;
    let mut diagnostics = None;
    loop {
        if Instant::now() >= deadline {
            return Err("behavior executor timed out".into());
        }
        match receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(Output::Stdout(result)) => output = Some(result?),
            Ok(Output::Stderr(bytes)) => diagnostics = Some(bytes),
            Ok(Output::InputError) => return Err("behavior executor stdin is not writable".into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(5))
            }
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())?
            && let (Some(bytes), Some(stderr)) = (&output, &diagnostics)
        {
            if !status.success() {
                let detail: String = String::from_utf8_lossy(stderr).chars().take(500).collect();
                return Err(format!("behavior executor exited with {status}: {detail}"));
            }
            let result: Value = serde_json::from_slice(bytes)
                .map_err(|_| "behavior executor returned invalid JSON")?;
            if !matches!(result["status"].as_str(), Some("completed" | "failed")) {
                return Err(
                    "behavior executor returned invalid JSON: executor result status is invalid"
                        .into(),
                );
            }
            return Ok(result);
        }
    }
}
