use crate::{BridgePolicy, LocalHttpOptions, LocalHttpServer, Result, RouteLineage, lease};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "bridge_exchange.rs"]
mod exchange;
#[path = "bridge_failure.rs"]
mod failure;
#[path = "bridge_observation.rs"]
mod observation;
#[path = "bridge_output_failure.rs"]
mod output_failure;

fn clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|e| e.into_inner())
}

struct Execution {
    plan: Value,
    budget: Value,
    lease: Value,
    epoch: u64,
    lineage: Option<RouteLineage>,
    policy: BridgePolicy,
    budget_expired: bool,
}
struct Control {
    cancelled: AtomicBool,
}
struct Callbacks {
    next: u64,
    events: VecDeque<Value>,
    pending: HashMap<u64, mpsc::SyncSender<Value>>,
}
struct Shared {
    authority: Mutex<crate::mcp_authority::McpAuthority>,
    scheduler: Mutex<crate::request_scheduler::Scheduler>,
    resource_policy: Value,
    options: Value,
    server: Mutex<LocalHttpServer>,
    execution: Mutex<Execution>,
    active: Mutex<HashMap<u64, Arc<Control>>>,
    callbacks: Mutex<Callbacks>,
    stopping: AtomicBool,
    next: AtomicU64,
    settlement_failure: Mutex<Option<String>>,
}

/// Native loopback bridge owner. Host callbacks are bounded RPC effects;
/// request policy, network streaming, cancellation and settlement run in Rust.
pub struct BridgeService {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Shared {
    fn validate(&self, execution: &Execution) -> Result<()> {
        let plan = &execution.plan;
        let deadline = chrono::DateTime::parse_from_rfc3339(
            execution.lease["deadline"].as_str().unwrap_or(""),
        )
        .map_err(|_| "invalid lease deadline")?
        .timestamp_millis();
        let expiry = chrono::DateTime::parse_from_rfc3339(plan["expiresAt"].as_str().unwrap_or(""))
            .map_err(|_| "invalid plan expiry")?
            .timestamp_millis();
        lease::validate(
            &json!({"lease":execution.lease,"sessionPlan":plan,"epoch":execution.epoch,"now":clock(),"deadline":deadline,"planExpiry":expiry}),
        )?;
        Ok(())
    }
    fn cancelled(&self, control: &Control, id: u64) -> Result<()> {
        if control.cancelled.load(Ordering::Acquire) || self.stopping.load(Ordering::Acquire) {
            return Err("request cancelled".into());
        }
        if lock(&self.server).closed(id) {
            return Err("bridge response closed".into());
        }
        Ok(())
    }
    fn call(&self, method: &str, params: Value, control: Option<(&Control, u64)>) -> Result<Value> {
        let (tx, rx) = mpsc::sync_channel(1);
        let id = {
            let mut callbacks = lock(&self.callbacks);
            if callbacks.pending.len() >= 256 {
                return Err("bridge host callback capacity exceeded".into());
            }
            callbacks.next = callbacks
                .next
                .checked_add(1)
                .ok_or("bridge callback identity exhausted")?;
            let id = callbacks.next;
            callbacks.pending.insert(id, tx);
            callbacks
                .events
                .push_back(json!({"id":id,"method":method,"params":params}));
            id
        };
        let deadline = Instant::now()
            + Duration::from_millis(self.options["requestTimeoutMs"].as_u64().unwrap_or(120_000));
        let result = loop {
            match rx.recv_timeout(Duration::from_millis(5)) {
                Ok(value) => {
                    break if let Some(error) = value["error"].as_str() {
                        Err(error.into())
                    } else {
                        Ok(value["result"].clone())
                    };
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("bridge host disconnected".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some((control, id)) = control
                && let Err(error) = self.cancelled(control, id)
            {
                break Err(error);
            }
            if Instant::now() >= deadline {
                break Err("bridge host callback timed out".into());
            }
        };
        let mut callbacks = lock(&self.callbacks);
        callbacks.pending.remove(&id);
        callbacks.events.retain(|event| event["id"] != id);
        result
    }
    fn cancel(&self) -> usize {
        lock(&self.server).cancel_uploads();
        let mut execution = lock(&self.execution);
        if !execution.lease.is_null() {
            execution.epoch = execution.epoch.saturating_add(1);
        }
        drop(execution);
        let active = lock(&self.active);
        for control in active.values() {
            control.cancelled.store(true, Ordering::Release);
        }
        active.len()
    }
}

impl BridgeService {
    pub fn start(mut options: Value) -> Result<Self> {
        let resource_policy =
            crate::request_scheduler::policy(&options["sessionPlan"]["resourcePolicy"])?;
        options["maxConcurrentRequests"] = json!(
            options["maxConcurrentRequests"]
                .as_u64()
                .unwrap_or(128)
                .min(
                    resource_policy["maxConcurrentRequests"].as_u64().unwrap()
                        + resource_policy["maxQueuedRequests"].as_u64().unwrap()
                )
        );
        crate::executor_validation::check(&options["executorValidatedTools"])?;
        let mut http: LocalHttpOptions = serde_json::from_value(options.clone())
            .map_err(|_| "invalid bridge listener options")?;
        if options["maxDownstreamBytes"]
            .as_u64()
            .is_none_or(|v| v == 0 || v > 32 * 1024 * 1024)
        {
            return Err("invalid bridge downstream limit".into());
        }
        // This owner bounds every provider/callback/backpressure wait and cancels
        // by the execution lease and author budget. A second whole-response
        // deadline in the loopback listener would truncate healthy buffered output.
        http.owner_manages_response = true;
        let server = LocalHttpServer::start(http)?;
        let plan = options["sessionPlan"].clone();
        let shared = Arc::new(Shared {
            authority: Mutex::new(crate::mcp_authority::McpAuthority::default()),
            scheduler: Mutex::new(crate::request_scheduler::Scheduler::new(&resource_policy)),
            resource_policy,
            options,
            server: Mutex::new(server),
            execution: Mutex::new(Execution {
                plan,
                budget: Value::Null,
                lease: Value::Null,
                epoch: 0,
                lineage: None,
                policy: BridgePolicy::default(),
                budget_expired: false,
            }),
            active: Mutex::new(HashMap::new()),
            callbacks: Mutex::new(Callbacks {
                next: 0,
                events: VecDeque::new(),
                pending: HashMap::new(),
            }),
            stopping: AtomicBool::new(false),
            next: AtomicU64::new(0),
            settlement_failure: Mutex::new(None),
        });
        let state = shared.clone();
        let worker = std::thread::Builder::new()
            .name("engine-model-bridge".into())
            .spawn(move || {
                while !state.stopping.load(Ordering::Acquire) {
                    let expired = {
                        let mut execution = lock(&state.execution);
                        let deadline = execution.budget["deadline"].as_u64();
                        let expired =
                            !execution.budget_expired && deadline.is_some_and(|v| v <= clock());
                        if expired {
                            execution.budget_expired = true;
                        }
                        expired
                    };
                    if expired {
                        state.cancel();
                    }
                    let request = lock(&state.server).poll();
                    if let Some(request) = request {
                        let request_id = request.id;
                        let control = Arc::new(Control {
                            cancelled: AtomicBool::new(false),
                        });
                        lock(&state.active).insert(request_id, control.clone());
                        let owner = state.clone();
                        let name = state.next.fetch_add(1, Ordering::Relaxed);
                        if std::thread::Builder::new()
                            .name(format!("engine-exchange-{name}"))
                            .spawn(move || {
                                exchange::run(&owner, request, &control);
                                lock(&owner.active).remove(&request_id);
                            })
                            .is_err()
                        {
                            lock(&state.server).abort(request_id);
                            lock(&state.active).remove(&request_id);
                        }
                    } else {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            })
            .map_err(|_| "cannot start bridge dispatcher")?;
        Ok(Self {
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn apply(&self, method: &str, input: &Value) -> Result<Value> {
        if matches!(method, "budget:start" | "lease:start")
            && let Some(error) = lock(&self.shared.settlement_failure).as_ref()
        {
            return Err(format!("bridge settlement is unresolved: {error}"));
        }
        match method {
            "inspect" => Ok(
                json!({"baseUrl":format!("http://127.0.0.1:{}",lock(&self.shared.server).port()),"active":lock(&self.shared.active).len(),"scheduling":lock(&self.shared.scheduler).snapshot(),"listening":!self.shared.stopping.load(Ordering::Acquire)}),
            ),
            "poll" => Ok(lock(&self.shared.callbacks).events.drain(..).collect()),
            "reply" => {
                if let Some(id) = input["id"].as_u64()
                    && let Some(sender) = lock(&self.shared.callbacks).pending.remove(&id)
                {
                    let _ = sender.try_send(input.clone());
                }
                Ok(Value::Null)
            }
            "budget:start" => {
                let mut state = lock(&self.shared.execution);
                if !state.budget.is_null() {
                    return Err("a task budget is already active".into());
                }
                if input["id"].as_str().is_none_or(str::is_empty)
                    || (!input["deadline"].is_null() && input["deadline"].as_u64().is_none())
                    || input.get("deadline").is_none()
                {
                    return Err("invalid task budget".into());
                }
                state.budget = input.clone();
                state.budget_expired = false;
                Ok(Value::Null)
            }
            "budget:end" => {
                let mut state = lock(&self.shared.execution);
                if state.budget["id"] != input["id"] || state.budget.is_null() {
                    return Ok(false.into());
                }
                state.budget = Value::Null;
                Ok(true.into())
            }
            "lease:start" => {
                let mut state = lock(&self.shared.execution);
                if !state.lease.is_null() {
                    return Err("an execution lease is already active".into());
                }
                let epoch = input["epoch"].as_u64().ok_or("invalid execution epoch")?;
                let candidate = Execution {
                    plan: state.plan.clone(),
                    budget: state.budget.clone(),
                    lease: input.clone(),
                    epoch,
                    lineage: None,
                    policy: BridgePolicy::default(),
                    budget_expired: state.budget_expired,
                };
                self.shared.validate(&candidate)?;
                state.lease = input.clone();
                state.epoch = epoch;
                state.lineage = Some(RouteLineage::new(input.clone()));
                state.policy.apply("beginExecution", &Value::Null)?;
                lock(&self.shared.authority).begin_execution();
                Ok(Value::Null)
            }
            "lease:renew" => {
                let mut state = lock(&self.shared.execution);
                self.shared.validate(&state)?;
                let lease = crate::renewal::execution(
                    &json!({"currentLease":state.lease,"previousPlan":state.plan,"sessionPlan":input["sessionPlan"],"lease":input["lease"],"epoch":state.epoch,"now":clock()}),
                )?;
                state.plan = input["sessionPlan"].clone();
                state.lease = lease.clone();
                Ok(lease)
            }
            "lease:matches" => {
                let state = lock(&self.shared.execution);
                Ok((!state.lease.is_null() && state.lease["leaseId"] == input["leaseId"]).into())
            }
            "lease:clear" => {
                let mut state = lock(&self.shared.execution);
                if !lock(&self.shared.active).is_empty() {
                    return Err("bridge requests have not drained".into());
                }
                state.lease = Value::Null;
                state.lineage = None;
                Ok(Value::Null)
            }
            "cancel" => Ok(self.shared.cancel().into()),
            _ => Err("unknown bridge service operation".into()),
        }
    }
    pub fn drain(&self) -> Result<()> {
        while !lock(&self.shared.active).is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(error) = lock(&self.shared.settlement_failure).as_ref() {
            return Err(format!("bridge settlement is unresolved: {error}"));
        }
        Ok(())
    }
    pub fn close(&self) -> Result<()> {
        self.shared.stopping.store(true, Ordering::Release);
        self.shared.cancel();
        if let Some(worker) = lock(&self.worker).take() {
            worker.join().map_err(|_| "bridge dispatcher failed")?;
        }
        let drained = self.drain();
        lock(&self.shared.server).close()?;
        drained
    }
}
impl Drop for BridgeService {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        self.shared.cancel();
    }
}
