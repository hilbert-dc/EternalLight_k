use crate::Result;
use reqwest::{
    Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::{Semaphore, mpsc},
    task::JoinHandle,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpOptions {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub timeout_ms: Option<u64>,
    pub idle_timeout_ms: Option<u64>,
    pub max_bytes: usize,
}

pub enum HttpEvent {
    Headers {
        status: u16,
        headers: BTreeMap<String, String>,
    },
    Chunk(Vec<u8>),
    End,
    Error(String),
}

struct Network {
    runtime: Runtime,
    client: Client,
    capacity: Arc<Semaphore>,
}

fn network() -> Result<&'static Network> {
    static NETWORK: OnceLock<Result<Network>> = OnceLock::new();
    NETWORK
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|_| "unable to start engine network runtime")?;
            let client = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(4)
                .build()
                .map_err(|_| "unable to initialize engine HTTP client")?;
            Ok(Network {
                runtime,
                client,
                capacity: Arc::new(Semaphore::new(128)),
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn runtime() -> Result<&'static Runtime> {
    Ok(&network()?.runtime)
}

/// A cancellable HTTP exchange. The bounded channel propagates consumer pressure
/// to the socket, and dropping the handle cancels the network task.
pub struct HttpTransport {
    events: mpsc::Receiver<HttpEvent>,
    task: JoinHandle<()>,
    terminal: bool,
}

// Bound each network or consumer wait without capping a healthy exchange's duration.
async fn wait_idle<T>(ms: Option<u64>, future: impl std::future::Future<Output = T>) -> std::result::Result<T, &'static str> {
    match ms {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), future)
            .await.map_err(|_| "engine HTTP idle timed out"),
        None => Ok(future.await),
    }
}

impl HttpTransport {
    pub fn start(options: HttpOptions) -> Result<Self> {
        if (options.timeout_ms.is_none() && options.idle_timeout_ms.is_none())
            || [options.timeout_ms, options.idle_timeout_ms].into_iter().flatten().any(|v| !(1..=3_600_000).contains(&v))
            || options.max_bytes == 0
            || options.max_bytes > 32 * 1024 * 1024
        {
            return Err("invalid engine HTTP limits".into());
        }
        if options.body.len() > 4 * 1024 * 1024 {
            return Err("engine HTTP request exceeds limit".into());
        }
        let url = Url::parse(&options.url).map_err(|_| "invalid engine HTTP URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("engine HTTP requires an HTTP(S) URL without embedded credentials".into());
        }
        let method = Method::from_bytes(options.method.as_bytes())
            .map_err(|_| "invalid engine HTTP method")?;
        if options.headers.len() > 128 {
            return Err("engine HTTP headers exceed limit".into());
        }
        let mut headers = HeaderMap::new();
        for (name, value) in options.headers {
            if name.len() + value.len() > 16_384 {
                return Err("engine HTTP header exceeds limit".into());
            }
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| "invalid engine HTTP header")?,
                HeaderValue::from_str(&value).map_err(|_| "invalid engine HTTP header")?,
            );
        }
        let network = network()?;
        let permit = network
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| "engine HTTP concurrency limit reached")?;
        let request = network
            .client
            .request(method, url)
            .headers(headers)
            .body(options.body)
            .build()
            .map_err(|_| "invalid engine HTTP request")?;
        let client = network.client.clone();
        let (tx, events) = mpsc::channel(8);
        let task = network.runtime.spawn(async move {
            let _permit = permit;
            let exchange = async {
                let mut response = wait_idle(options.idle_timeout_ms, client.execute(request))
                    .await?
                    .map_err(|_| "engine HTTP connection failed")?;
                if response
                    .content_length()
                    .is_some_and(|size| size > options.max_bytes as u64)
                {
                    return Err("downstream response exceeds bridge limit");
                }
                let headers = response
                    .headers()
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.to_string(),
                            String::from_utf8_lossy(value.as_bytes()).into_owned(),
                        )
                    })
                    .collect();
                wait_idle(options.idle_timeout_ms, tx.send(HttpEvent::Headers {
                    status: response.status().as_u16(),
                    headers,
                }))
                .await?
                .map_err(|_| "HTTP consumer closed")?;
                let mut bytes: usize = 0;
                while let Some(chunk) = wait_idle(options.idle_timeout_ms, response.chunk())
                    .await?
                    .map_err(|_| "engine HTTP response stream failed")?
                {
                    bytes = bytes.saturating_add(chunk.len());
                    if bytes > options.max_bytes {
                        return Err("downstream response exceeds bridge limit");
                    }
                    for part in chunk.chunks(16 * 1024) {
                        wait_idle(options.idle_timeout_ms, tx.send(HttpEvent::Chunk(part.to_vec())))
                            .await?
                            .map_err(|_| "HTTP consumer closed")?;
                    }
                }
                Ok(())
            };
            let result = match options.timeout_ms {
                Some(ms) => tokio::time::timeout(Duration::from_millis(ms), exchange)
                    .await.unwrap_or(Err("engine HTTP request timed out")),
                None => exchange.await,
            };
            let event = match result {
                Ok(()) => HttpEvent::End,
                Err(message) => HttpEvent::Error(message.into()),
            };
            let _ = tx.send(event).await;
        });
        Ok(Self {
            events,
            task,
            terminal: false,
        })
    }

    pub fn poll(&mut self) -> Option<HttpEvent> {
        if self.terminal {
            return None;
        }
        match self.events.try_recv() {
            Ok(event) => {
                self.terminal = matches!(event, HttpEvent::End | HttpEvent::Error(_));
                Some(event)
            }
            Err(mpsc::error::TryRecvError::Empty) => None,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.terminal = true;
                Some(HttpEvent::Error("engine HTTP transport closed".into()))
            }
        }
    }

    pub fn cancel(&mut self) {
        self.task.abort();
        self.events.close();
        self.terminal = true;
    }
}

impl Drop for HttpTransport {
    fn drop(&mut self) {
        self.task.abort();
    }
}
