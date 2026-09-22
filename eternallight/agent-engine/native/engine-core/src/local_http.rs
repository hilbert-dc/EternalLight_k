use crate::{Result, http_transport};
use hyper::{
    Request, Response,
    body::{Body, Bytes, Frame, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    future::poll_fn,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHttpOptions {
    pub port: u16,
    pub token: String,
    pub max_body_bytes: usize,
    pub max_concurrent_requests: usize,
    pub request_timeout_ms: u64,
}

#[derive(Serialize)]
pub struct LocalRequest {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

struct StreamBody {
    receiver: mpsc::Receiver<Bytes>,
    _permit: Option<OwnedSemaphorePermit>,
    aborted: Arc<AtomicBool>,
}

impl Body for StreamBody {
    type Data = Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Bytes>, std::io::Error>>> {
        if self.aborted.load(Ordering::Acquire) {
            return Poll::Ready(Some(Err(std::io::Error::other("bridge response aborted"))));
        }
        self.receiver
            .poll_recv(cx)
            .map(|value| value.map(|bytes| Ok(Frame::data(bytes))))
    }
}

struct Slot {
    head: Option<oneshot::Sender<Response<StreamBody>>>,
    sender: mpsc::Sender<Bytes>,
    receiver: Option<mpsc::Receiver<Bytes>>,
    permit: Option<OwnedSemaphorePermit>,
    written: usize,
    aborted: Arc<AtomicBool>,
}

struct Shared {
    options: LocalHttpOptions,
    slots: Mutex<HashMap<u64, Slot>>,
    events: mpsc::Sender<LocalRequest>,
    capacity: Arc<Semaphore>,
    ids: AtomicU64,
    cancel_uploads: tokio::sync::watch::Sender<u64>,
}

fn failure(status: u16, message: &str, kind: &str) -> Response<StreamBody> {
    let (tx, rx) = mpsc::channel(1);
    let _ = tx.try_send(Bytes::from(
        serde_json::json!({"error":{"message":message,"type":kind}}).to_string(),
    ));
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(StreamBody {
            receiver: rx,
            _permit: None,
            aborted: Arc::new(AtomicBool::new(false)),
        })
        .expect("static HTTP response")
}

async fn serve(
    mut request: Request<Incoming>,
    shared: Arc<Shared>,
) -> std::result::Result<Response<StreamBody>, Infallible> {
    if request.method() != "POST" || !matches!(request.uri().path(), "/responses" | "/v1/responses")
    {
        return Ok(failure(404, "route not found", "invalid_request_error"));
    }
    if request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {}", shared.options.token))
    {
        return Ok(failure(
            401,
            "invalid bridge capability",
            "authentication_error",
        ));
    }
    let Ok(permit) = shared.capacity.clone().try_acquire_owned() else {
        return Ok(failure(
            429,
            "bridge concurrency limit reached",
            "rate_limit_error",
        ));
    };
    let method = request.method().to_string();
    let url = request.uri().to_string();
    let headers = request
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let mut bytes = Vec::new();
    let mut cancellation = shared.cancel_uploads.subscribe();
    let upload = async {
        while let Some(frame) = poll_fn(|cx| Pin::new(request.body_mut()).poll_frame(cx)).await {
            let frame = frame.map_err(|_| "request body read failed")?;
            if let Ok(data) = frame.into_data() {
                if bytes.len().saturating_add(data.len()) > shared.options.max_body_bytes {
                    return Err("request body exceeds bridge limit");
                }
                bytes.extend_from_slice(&data);
            }
        }
        String::from_utf8(bytes).map_err(|_| "request body is not valid UTF-8")
    };
    let upload_result = tokio::select! {
        result = tokio::time::timeout(Duration::from_millis(shared.options.request_timeout_ms),upload) => result,
        _ = cancellation.changed() => return Ok(failure(499,"request cancelled","cancelled")),
    };
    let body = match upload_result {
        Ok(Ok(body)) => body,
        Ok(Err(message)) => {
            return Ok(failure(
                if message.contains("exceeds") {
                    413
                } else {
                    400
                },
                message,
                "invalid_request_error",
            ));
        }
        Err(_) => return Ok(failure(504, "request body read timed out", "timeout")),
    };
    let id = shared.ids.fetch_add(1, Ordering::Relaxed);
    if id > 9_007_199_254_740_991 {
        return Ok(failure(503, "request identity exhausted", "bridge_error"));
    }
    let (head, response) = oneshot::channel();
    let (sender, receiver) = mpsc::channel(8);
    {
        let Ok(mut slots) = shared.slots.lock() else {
            return Ok(failure(503, "bridge state unavailable", "bridge_error"));
        };
        slots.insert(
            id,
            Slot {
                head: Some(head),
                sender,
                receiver: Some(receiver),
                permit: Some(permit),
                written: 0,
                aborted: Arc::new(AtomicBool::new(false)),
            },
        );
    }
    if shared
        .events
        .try_send(LocalRequest {
            id,
            method,
            url,
            headers,
            body,
        })
        .is_err()
    {
        if let Ok(mut slots) = shared.slots.lock() {
            slots.remove(&id);
        }
        return Ok(failure(503, "bridge request queue is full", "bridge_error"));
    }
    let result = tokio::time::timeout(
        Duration::from_millis(shared.options.request_timeout_ms),
        response,
    )
    .await;
    match result {
        Ok(Ok(response)) => Ok(response),
        _ => {
            if let Ok(mut slots) = shared.slots.lock() {
                slots.remove(&id);
            }
            Ok(failure(504, "bridge response timed out", "timeout"))
        }
    }
}

/// Loopback-only HTTP listener with bounded uploads, requests and response queues.
/// Dropping or closing the listener aborts all accepted connection tasks.
pub struct LocalHttpServer {
    shared: Arc<Shared>,
    events: mpsc::Receiver<LocalRequest>,
    task: Option<JoinHandle<()>>,
    port: u16,
}

impl LocalHttpServer {
    pub fn start(options: LocalHttpOptions) -> Result<Self> {
        if options.token.is_empty()
            || options.token.len() > 4096
            || options.max_body_bytes == 0
            || options.max_body_bytes > 4 * 1024 * 1024
            || !(1..=128).contains(&options.max_concurrent_requests)
            || !(1..=3_600_000).contains(&options.request_timeout_ms)
        {
            return Err("invalid local HTTP listener options".into());
        }
        let runtime = http_transport::runtime()?;
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, options.port))
            .map_err(|_| "unable to bind local HTTP listener")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "unable to configure local HTTP listener")?;
        let port = listener
            .local_addr()
            .map_err(|_| "local HTTP address unavailable")?
            .port();
        let _guard = runtime.enter();
        let listener =
            TcpListener::from_std(listener).map_err(|_| "unable to start local HTTP listener")?;
        let (tx, events) = mpsc::channel(128);
        let shared = Arc::new(Shared {
            capacity: Arc::new(Semaphore::new(options.max_concurrent_requests)),
            options,
            slots: Mutex::new(HashMap::new()),
            events: tx,
            ids: AtomicU64::new(1),
            cancel_uploads: tokio::sync::watch::channel(0).0,
        });
        let state = shared.clone();
        let task = runtime.spawn(async move {
            let connections = Arc::new(Semaphore::new(128));
            let mut tasks = JoinSet::new();
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                while tasks.try_join_next().is_some() {}
                let Ok(permit) = connections.clone().try_acquire_owned() else {
                    continue;
                };
                let shared = state.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    let service = service_fn(move |request| serve(request, shared.clone()));
                    let _ = tokio::time::timeout(
                        Duration::from_secs(3600),
                        http1::Builder::new()
                            .timer(hyper_util::rt::TokioTimer::new())
                            .header_read_timeout(Duration::from_secs(10))
                            .max_buf_size(32 * 1024)
                            .serve_connection(TokioIo::new(stream), service),
                    )
                    .await;
                });
            }
        });
        Ok(Self {
            shared,
            events,
            task: Some(task),
            port,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn poll(&mut self) -> Option<LocalRequest> {
        self.events.try_recv().ok()
    }
    pub fn closed(&self, id: u64) -> bool {
        self.shared.slots.lock().map_or(true, |slots| {
            slots.get(&id).is_none_or(|slot| {
                slot.head
                    .as_ref()
                    .map_or_else(|| slot.sender.is_closed(), oneshot::Sender::is_closed)
            })
        })
    }
    pub fn head(&mut self, id: u64, status: u16, headers: HashMap<String, String>) -> Result<()> {
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| "bridge state unavailable")?;
        let slot = slots.get_mut(&id).ok_or("bridge response closed")?;
        let mut response = Response::builder().status(status);
        for (name, value) in headers {
            response = response.header(name, value);
        }
        // Validate headers before consuming the one-shot body and ownership.
        let response = response
            .body(())
            .map_err(|_| "invalid bridge response headers")?;
        let (parts, ()) = response.into_parts();
        let head = slot
            .head
            .take()
            .ok_or("bridge response headers already sent")?;
        let body = StreamBody {
            receiver: slot
                .receiver
                .take()
                .ok_or("bridge response body unavailable")?,
            _permit: slot.permit.take(),
            aborted: slot.aborted.clone(),
        };
        head.send(Response::from_parts(parts, body))
            .map_err(|_| "bridge response closed".into())
    }
    pub fn write(&mut self, id: u64, bytes: &[u8]) -> Result<bool> {
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| "bridge state unavailable")?;
        let slot = slots.get_mut(&id).ok_or("bridge response closed")?;
        if slot.head.is_some() {
            return Err("bridge response headers are required".into());
        }
        if slot.written.saturating_add(bytes.len()) > 32 * 1024 * 1024 {
            return Err("bridge response exceeds limit".into());
        }
        match slot.sender.try_send(Bytes::copy_from_slice(bytes)) {
            Ok(()) => {
                slot.written += bytes.len();
                Ok(true)
            }
            Err(mpsc::error::TrySendError::Full(_)) => Ok(false),
            Err(mpsc::error::TrySendError::Closed(_)) => Err("bridge response closed".into()),
        }
    }
    pub fn end(&mut self, id: u64) {
        if let Ok(mut slots) = self.shared.slots.lock() {
            slots.remove(&id);
        }
    }
    pub fn abort(&mut self, id: u64) {
        if let Ok(mut slots) = self.shared.slots.lock()
            && let Some(slot) = slots.remove(&id)
        {
            slot.aborted.store(true, Ordering::Release);
        }
    }
    pub fn cancel_uploads(&self) {
        self.shared
            .cancel_uploads
            .send_modify(|epoch| *epoch = epoch.wrapping_add(1));
    }
    pub fn close(&mut self) -> Result<()> {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = http_transport::runtime()?.block_on(task);
        }
        self.shared
            .slots
            .lock()
            .map_err(|_| "bridge state unavailable")?
            .clear();
        Ok(())
    }
}

impl Drop for LocalHttpServer {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
