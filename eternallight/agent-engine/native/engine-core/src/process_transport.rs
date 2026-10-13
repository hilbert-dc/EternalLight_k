use crate::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const MAX_LINE: usize = 256 * 1024;
const MAX_QUEUE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOptions {
    command: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
}

#[derive(Default)]
struct Inbox {
    events: VecDeque<(Value, usize)>,
    bytes: usize,
    failure: Option<String>,
    failed: bool,
    stdout_done: bool,
    exit_delivered: bool,
}

struct Shared {
    child: Mutex<Child>,
    inbox: Mutex<Inbox>,
}

impl Shared {
    fn fail(&self, message: &str) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        if !inbox.failed {
            inbox.failure = Some(message.to_owned());
            inbox.failed = true;
        }
        drop(inbox);
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if child.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = child.kill();
        }
    }

    fn push(&self, value: Value, size: usize) -> bool {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        if inbox.failed {
            return false;
        }
        if inbox.events.len() >= 1024 || inbox.bytes.saturating_add(size) > MAX_QUEUE_BYTES {
            drop(inbox);
            self.fail("Codex App Server event queue exceeds byte limit");
            return false;
        }
        inbox.bytes += size;
        inbox.events.push_back((value, size));
        true
    }
}

/// Owns one subprocess generation. Readers and writers have bounded queues;
/// closing reaps this exact child before resolving, never a replacement process.
pub struct ProcessTransport {
    shared: Arc<Shared>,
    sender: Mutex<Option<mpsc::SyncSender<Vec<u8>>>>,
    closing: Mutex<()>,
}

impl ProcessTransport {
    pub fn process_id(&self) -> Result<u32> {
        Ok(self
            .shared
            .child
            .lock()
            .map_err(|_| "process lock poisoned")?
            .id())
    }
    pub fn start(options: ProcessOptions) -> Result<Self> {
        if options.command.is_empty() || options.command.iter().any(String::is_empty) {
            return Err("App Server command is required".into());
        }
        let mut command = Command::new(&options.command[0]);
        command
            .args(&options.command[1..])
            .current_dir(options.cwd)
            .env_clear()
            .envs(options.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW for GUI-hosted engines.
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start Codex App Server: {e}"))?;
        let stdout = child.stdout.take().ok_or("missing App Server stdout")?;
        let stderr = child.stderr.take().ok_or("missing App Server stderr")?;
        let mut stdin = child.stdin.take().ok_or("missing App Server stdin")?;
        let shared = Arc::new(Shared {
            child: Mutex::new(child),
            inbox: Mutex::new(Inbox::default()),
        });
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(32);
        let transport = Self {
            shared: shared.clone(),
            sender: Mutex::new(Some(sender)),
            closing: Mutex::new(()),
        };
        let writer_state = shared.clone();
        std::thread::Builder::new()
            .name("engine-rpc-write".into())
            .spawn(move || {
                for bytes in receiver {
                    if stdin
                        .write_all(&bytes)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        writer_state.fail("Codex App Server stdin is not writable");
                        break;
                    }
                }
            })
            .map_err(|e| format!("cannot start App Server writer: {e}"))?;
        let reader_state = shared.clone();
        std::thread::Builder::new()
            .name("engine-rpc-read".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    match Read::by_ref(&mut reader)
                        .take((MAX_LINE + 1) as u64)
                        .read_until(b'\n', &mut line)
                    {
                        Ok(0) => break,
                        Ok(_) if line.len() > MAX_LINE => {
                            reader_state.fail("Codex App Server event exceeds byte limit");
                            break;
                        }
                        Ok(_) => {
                            if line.iter().all(u8::is_ascii_whitespace) {
                                continue;
                            }
                            let message = match serde_json::from_slice::<Value>(&line) {
                                Ok(message) if message.is_object() => message,
                                _ => {
                                    reader_state.fail("Codex App Server emitted invalid JSONL");
                                    break;
                                }
                            };
                            if !reader_state
                                .push(json!({"kind":"message","value":message}), line.len())
                            {
                                break;
                            }
                        }
                        Err(_) => {
                            reader_state.fail("Codex App Server stdout read failed");
                            break;
                        }
                    }
                }
                reader_state
                    .inbox
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .stdout_done = true;
            })
            .map_err(|e| format!("cannot start App Server reader: {e}"))?;
        let stderr_state = shared.clone();
        std::thread::Builder::new()
            .name("engine-rpc-stderr".into())
            .spawn(move || {
                let mut stderr = stderr;
                let mut bytes = [0; 4096];
                while let Ok(count) = stderr.read(&mut bytes) {
                    if count == 0 {
                        break;
                    }
                    if !stderr_state.push(
                        json!({"kind":"stderr","text":String::from_utf8_lossy(&bytes[..count])}),
                        count,
                    ) {
                        break;
                    }
                }
            })
            .map_err(|e| format!("cannot start App Server stderr reader: {e}"))?;
        Ok(transport)
    }

    pub fn send(&self, message: &str) -> Result<()> {
        if message.len() > 2 * 1024 * 1024 {
            return Err("Codex App Server request exceeds byte limit".into());
        }
        let value: Value =
            serde_json::from_str(message).map_err(|_| "invalid App Server request")?;
        if !value.is_object() {
            return Err("invalid App Server request".into());
        }
        let mut bytes = serde_json::to_vec(&value).map_err(|_| "invalid App Server request")?;
        bytes.push(b'\n');
        self.sender
            .lock()
            .map_err(|_| "App Server sender unavailable")?
            .as_ref()
            .ok_or("Codex App Server stdin is not writable")?
            .try_send(bytes)
            .map_err(|_| "Codex App Server write queue unavailable".into())
    }

    pub fn drain(&self) -> Result<Value> {
        let status = self
            .shared
            .child
            .lock()
            .map_err(|_| "App Server process unavailable")?
            .try_wait()
            .map_err(|e| e.to_string())?;
        let mut inbox = self
            .shared
            .inbox
            .lock()
            .map_err(|_| "App Server event queue unavailable")?;
        let mut events = Vec::new();
        if let Some(error) = inbox.failure.take() {
            inbox.events.clear();
            inbox.bytes = 0;
            events.push(json!({"kind":"error","message":error}));
        }
        for _ in 0..64 {
            let Some((event, size)) = inbox.events.pop_front() else {
                break;
            };
            inbox.bytes -= size;
            events.push(event);
        }
        if let Some(status) = status
            && inbox.stdout_done
            && inbox.events.is_empty()
            && !inbox.exit_delivered
        {
            inbox.exit_delivered = true;
            events.push(json!({"kind":"exit","code":status.code()}));
        }
        Ok(events.into())
    }

    pub fn close(&self, timeout_ms: u64) -> Result<()> {
        let _closing = self
            .closing
            .lock()
            .map_err(|_| "App Server closer unavailable")?;
        // Keep stdin open while the child handles SIGTERM; an early EOF can
        // bypass its graceful shutdown handler. New sends are already fenced.
        let _sender = self
            .sender
            .lock()
            .map_err(|_| "App Server sender unavailable")?
            .take();
        {
            let mut child = self
                .shared
                .child
                .lock()
                .map_err(|_| "App Server process unavailable")?;
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Ok(());
            }
            #[cfg(unix)]
            {
                // The child is unreaped and locked, so its PID cannot be reused here.
                unsafe {
                    libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
                }
            }
            #[cfg(not(unix))]
            child.kill().map_err(|e| e.to_string())?;
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.min(60_000));
        loop {
            {
                let mut child = self
                    .shared
                    .child
                    .lock()
                    .map_err(|_| "App Server process unavailable")?;
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    if let Err(error) = child.kill()
                        && child.try_wait().map_err(|e| e.to_string())?.is_none()
                    {
                        return Err(error.to_string());
                    }
                    child.wait().map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for ProcessTransport {
    fn drop(&mut self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        if let Ok(mut child) = self.shared.child.lock()
            && child.try_wait().is_ok_and(|status| status.is_none())
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
